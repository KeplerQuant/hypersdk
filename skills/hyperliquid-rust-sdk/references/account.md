# Positions, balances, orders and fills (HTTP)

All methods are on `HttpClient`, unsigned, and return `anyhow::Result<…>`. `user` is the
**account** address. When you sign with an API agent, that is the owning account, not the
agent's address (`user_role(agent)` → `UserRole::Agent { user }`).

## Perp positions and margin

```rust
pub async fn clearinghouse_state(&self, user: Address, dex_name: Option<String>) -> Result<ClearinghouseState>
```

`dex_name: None` is the main perp DEX; HIP-3 positions need `Some("<dex>")`.

```rust
pub struct ClearinghouseState {
    pub margin_summary: MarginSummary,
    pub cross_margin_summary: MarginSummary,
    pub cross_maintenance_margin_used: Decimal,
    pub withdrawable: Decimal,
    pub asset_positions: Vec<AssetPosition>,   // AssetPosition { position_type, position: PositionData }
    pub time: u64,
}
pub struct MarginSummary { pub account_value: Decimal, pub total_ntl_pos: Decimal,
                           pub total_raw_usd: Decimal, pub total_margin_used: Decimal }
// MarginSummary: available_margin() = account_value - total_margin_used,
//                margin_utilization() = percent (0 when account_value is 0)

pub struct PositionData {
    pub coin: String,
    pub szi: Decimal,                    // signed: > 0 long, < 0 short
    pub leverage: Leverage,              // { leverage_type: LeverageType::{Cross, Isolated}, value: u32, raw_usd: Option<Decimal> }
    pub entry_px: Option<Decimal>,
    pub position_value: Decimal,
    pub unrealized_pnl: Decimal,
    pub return_on_equity: Decimal,
    pub liquidation_px: Option<Decimal>,
    pub margin_used: Decimal,
    pub max_leverage: u32,
    pub cum_funding: CumulativeFunding,  // { all_time, since_open, since_change }
}
// PositionData: is_long(), is_short(), abs_size(), side() -> "long" | "short"
// Leverage: is_cross(), is_isolated()
```

From the `clearinghouse_state` doc and `examples/hypercore/subaccounts.rs`:

```rust
use hypersdk::{Address, hypercore};
use rust_decimal::Decimal;

async fn show_positions(client: &hypercore::HttpClient, user: Address) -> anyhow::Result<()> {
    let state = client.clearinghouse_state(user, None).await?;

    println!("Account value: {}", state.margin_summary.account_value);
    println!("Withdrawable: {}", state.withdrawable);
    println!("Margin utilization: {}%", state.margin_summary.margin_utilization());

    for asset_position in &state.asset_positions {
        let pos = &asset_position.position;
        println!(
            "{} {}: {} @ {:?} (PnL: {}, liq: {:?}, {}x {})",
            pos.side(),
            pos.coin,
            pos.abs_size(),
            pos.entry_px,
            pos.unrealized_pnl,
            pos.liquidation_px,
            pos.leverage.value,
            pos.leverage.leverage_type,
        );
    }
    Ok(())
}

/// Signed size for one coin; zero when flat.
async fn position_size(client: &hypercore::HttpClient, user: Address, coin: &str) -> anyhow::Result<Decimal> {
    let state = client.clearinghouse_state(user, None).await?;
    Ok(state
        .asset_positions
        .iter()
        .find(|p| p.position.coin == coin)
        .map(|p| p.position.szi)
        .unwrap_or(Decimal::ZERO))
}

fn main() {}
```

## Spot balances

```rust
pub async fn user_balances(&self, user: Address) -> Result<Vec<UserBalance>>

pub struct UserBalance { pub coin: String, pub token: Option<usize>, pub hold: Decimal,
                         pub total: Decimal, pub entry_ntl: Decimal }
// available() = total - hold, can_trade(amount), has_held(), held_percentage()
```

From `examples/hypercore/user_balances.rs`:

```rust
use hypersdk::{Address, hypercore};

async fn show_balances(client: &hypercore::HttpClient, user: Address) -> anyhow::Result<()> {
    let balances = client.user_balances(user).await?;
    if balances.is_empty() {
        println!("No spot balances found for {user:?}.");
        return Ok(());
    }
    for balance in &balances {
        println!("{:<12} total={} hold={} available={}", balance.coin, balance.total, balance.hold, balance.available());
    }
    Ok(())
}

fn main() {}
```

## Open orders and order status

```rust
pub async fn open_orders(&self, user: Address, dex_name: Option<String>) -> Result<Vec<BasicOrder>>
pub async fn order_status(&self, user: Address, oid: OidOrCloid) -> Result<Option<OrderUpdate<BasicOrder>>>
pub async fn historical_orders(&self, user: Address) -> Result<Vec<OrderUpdate<BasicOrder>>>
```

- `open_orders` uses the `frontendOpenOrders` request, so trigger fields are populated.
- `order_status` returns `Ok(None)` when the oid/cloid is unknown.

```rust
pub struct BasicOrder {
    pub timestamp: u64, pub coin: String, pub side: Side /* Bid | Ask */, pub limit_px: Decimal,
    pub sz: Decimal /* remaining */, pub oid: u64, pub orig_sz: Decimal, pub cloid: Option<B128>,
    pub order_type: OrderType, pub tif: Option<TimeInForce>, pub reduce_only: bool,
    pub is_trigger: Option<bool>, pub trigger_px: Option<Decimal>,
    pub trigger_condition: Option<String>, pub is_position_tpsl: Option<bool>,
}
pub struct OrderUpdate<T> { pub status: OrderStatus, pub status_timestamp: u64, pub order: T }
// OrderStatus: Open, Filled, Canceled, Triggered, Rejected, MarginCanceled, TickRejected, … (29 variants)
// helpers: is_finished() (anything but Open), is_filled(), is_cancelled(), is_rejected()
```

```rust
use hypersdk::{
    Address,
    hypercore::{self, Cloid, OidOrCloid, types::Side},
};

async fn show_open_orders(client: &hypercore::HttpClient, user: Address) -> anyhow::Result<()> {
    for order in client.open_orders(user, None).await? {
        let side = match order.side {
            Side::Bid => "buy",
            Side::Ask => "sell",
        };
        println!("#{} {} {} {} @ {} (orig {})", order.oid, order.coin, side, order.sz, order.limit_px, order.orig_sz);
    }
    Ok(())
}

/// Look an order up by the cloid you placed it with.
async fn is_done(client: &hypercore::HttpClient, user: Address, cloid: Cloid) -> anyhow::Result<bool> {
    match client.order_status(user, OidOrCloid::Right(cloid)).await? {
        Some(update) => {
            println!("{} status={} filled={}", update.order.oid, update.status, update.status.is_filled());
            Ok(update.status.is_finished())
        }
        None => anyhow::bail!("unknown order {cloid}"),
    }
}

fn main() {}
```

## Fills

```rust
pub async fn user_fills(&self, user: Address) -> Result<Vec<Fill>>
pub async fn user_fills_by_time(&self, user: Address, start_time: u64, end_time: Option<u64>) -> Result<Vec<Fill>>
```

Times are milliseconds; `end_time: None` means now.

```rust
pub struct Fill {
    pub coin: String, pub px: Decimal, pub sz: Decimal, pub side: Side, pub time: u64,
    pub start_position: Decimal, pub dir: FillDirection /* OpenLong, CloseShort, … */,
    pub closed_pnl: Decimal, pub hash: String, pub oid: u64, pub crossed: bool /* taker */,
    pub fee: Decimal, pub tid: u64, pub cloid: Option<B128>, pub fee_token: String,
    pub builder_fee: Option<Decimal>, pub liquidation: Option<Liquidation>,
}
// notional(), is_opening() / is_closing() (by closed_pnl == 0), is_maker(), is_taker(),
// is_liquidation(), net_proceeds() = notional - fee
```

```rust
use chrono::Utc;
use hypersdk::{Address, hypercore};
use rust_decimal::Decimal;

async fn realized_pnl_last_day(client: &hypercore::HttpClient, user: Address) -> anyhow::Result<Decimal> {
    let end_time = Utc::now().timestamp_millis() as u64;
    let start_time = end_time - 24 * 60 * 60 * 1000;
    let fills = client.user_fills_by_time(user, start_time, Some(end_time)).await?;
    Ok(fills.iter().map(|f| f.closed_pnl - f.fee).sum())
}

fn main() {}
```

## Other account queries

| Method | Returns | Notes |
| --- | --- | --- |
| `user_fees(user)` | `UserFees` | `maker_rate`, `taker_rate`, `spot_maker_rate`, `spot_taker_rate`, `active_referral_discount` |
| `user_rate_limit(user)` | `UserRateLimit` | `cum_vlm`, `n_requests_used`, `n_requests_cap`, `n_requests_surplus` |
| `active_asset_data(user, coin: String)` | `ActiveAssetData` | leverage, `max_trade_szs_pair()`, `available_to_trade_pair()`, `mark_px` |
| `user_funding(user, start_time, end_time)` | `Vec<UserFundingEntry>` | funding payments |
| `user_role(user)` | `UserRole` | `User`, `Agent { user }`, `Vault`, `SubAccount { master }`, `Missing` |
| `subaccounts(user)` | `Vec<SubAccount>` | `name`, `sub_account_user`, `master`, `clearinghouse_state`, `spot_state` |
| `api_agents(user)` | `Vec<ApiAgent>` | `name`, `address`, `valid_until` |
| `user_vault_equities(user)` | `Vec<UserVaultEquity>` | |
| `max_builder_fee(user, builder)` | `u32` | tenths of a basis point |
| `portfolio`, `referral`, `twap_history`, `user_non_funding_ledger_updates`, `web_data2` | `serde_json::Value` | untyped |

```rust
use hypersdk::{Address, hypercore::{self, UserRole}};

/// Resolve the account an address trades for.
async fn owning_account(client: &hypercore::HttpClient, addr: Address) -> anyhow::Result<Address> {
    Ok(match client.user_role(addr).await? {
        UserRole::Agent { user } => user,
        UserRole::User | UserRole::Vault | UserRole::SubAccount { .. } => addr,
        UserRole::Missing => anyhow::bail!("{addr} is unknown to the exchange"),
    })
}

fn main() {}
```
