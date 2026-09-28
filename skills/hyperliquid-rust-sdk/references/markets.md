# Markets, prices and market data (HTTP)

All methods are on `HttpClient` and return `anyhow::Result<…>`.

## Market metadata

```rust
pub async fn perps(&self) -> Result<Vec<PerpMarket>>                  // main perp DEX
pub async fn perp_dexes(&self) -> Result<Vec<Dex>>                     // HIP-3 DEXes
pub async fn perps_from(&self, dex: Dex) -> Result<Vec<PerpMarket>>    // one HIP-3 DEX
pub async fn spot(&self) -> Result<Vec<SpotMarket>>
pub async fn spot_tokens(&self) -> Result<Vec<SpotToken>>
pub async fn outcomes(&self) -> Result<Vec<OutcomeMarket>>             // HIP-4, one entry per side
```

`PerpMarket` (public fields): `name: String`, `index: usize`, `sz_decimals: i64`,
`collateral: SpotToken`, `max_leverage: u64`, `isolated_margin: bool`,
`margin_mode: Option<MarginMode>`, `deployer_fee_scale: Option<Decimal>`, `growth_mode: bool`,
`aligned_quote_token: bool`, `delisted: bool`, `table: PriceTick`.

`SpotMarket`: `name: String` (`"PURR/USDC"` or `"@123"`), `index: usize`,
`tokens: [SpotToken; 2]` (base, quote), `table: PriceTick`. Methods: `symbol()` →
`"BASE/QUOTE"`, `base()`, `quote()`. Spot size decimals are the base token's:
`market.base().sz_decimals`.

`SpotToken`: `name`, `index: u32`, `token_id: B128`, `evm_contract: Option<Address>`,
`cross_chain_address: Option<Address>`, `sz_decimals`, `wei_decimals`, `evm_extra_decimals`.

`OutcomeMarket`: `info: OutcomeInfo`, `side: String`, `market: usize` (use as the order asset);
`coin()` returns the `"#<n>"` name for info/WebSocket.

### Asset index vs. coin name

| Market | Order/cancel `asset` | Coin string for info + WebSocket |
| --- | --- | --- |
| Main-DEX perp | `PerpMarket.index` (0, 1, …) | `PerpMarket.name`, e.g. `"BTC"` |
| HIP-3 perp | `PerpMarket.index` = `100_000 + dex_index * 10_000 + i` | `PerpMarket.name`, e.g. `"xyz:XYZ100"` |
| Spot | `SpotMarket.index` = `10_000 + spot_index` | `SpotMarket.name`, e.g. `"PURR/USDC"`, `"@107"` |
| Outcome | `OutcomeMarket.market` = `100_000_000 + outcome * 10 + side` | `OutcomeMarket::coin()`, e.g. `"#42"` |

Look markets up once at startup and keep them; `perps()` makes two HTTP requests.

From `examples/hypercore/list-markets.rs` and `list-hip3.rs`:

```rust
use hypersdk::hypercore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = hypercore::mainnet();

    for market in client.spot().await? {
        println!("{}\t{}/{}\tasset={}", market.name, market.tokens[0].name, market.tokens[1].name, market.index);
    }

    for market in client.perps().await? {
        println!("{}\tasset={}\tmax_lev={}\tdelisted={}", market.name, market.index, market.max_leverage, market.delisted);
    }

    // HIP-3 DEXes: perps_from() takes the Dex by value.
    for dex in client.perp_dexes().await? {
        println!("markets for {dex}");
        for market in client.perps_from(dex).await? {
            println!("  {}\tasset={}", market.name, market.index);
        }
    }
    Ok(())
}
```

## Price ticks and sizes

```rust
// PerpMarket and SpotMarket both have:
pub fn round_price(&self, price: Decimal) -> Option<Decimal>   // nearest tick, ties toward zero
pub fn round_by_side(&self, side: Side, price: Decimal, conservative: bool) -> Option<Decimal>
pub fn tick_for(&self, price: Decimal) -> Option<Decimal>
```

`round_by_side` direction (from `PriceTick::round_by_side`):

| side | conservative | rounds | use |
| --- | --- | --- | --- |
| `Side::Ask` | `true` | up | resting sell, better price |
| `Side::Ask` | `false` | down | aggressive sell |
| `Side::Bid` | `true` | down | resting buy, better price |
| `Side::Bid` | `false` | up | aggressive buy |

Ticks keep 5 significant figures, capped at `6 - sz_decimals` decimals for perps and
`8 - sz_decimals` for spot. All return `None` for a zero or negative price.

The crate has **no size-rounding function**. Truncate with `rust_decimal` directly:

```rust
use hypersdk::hypercore::{PerpMarket, SpotMarket, types::Side};
use rust_decimal::{Decimal, RoundingStrategy, dec};

/// Size truncated to the market's lot size (never rounds up past what you can afford).
fn perp_size(market: &PerpMarket, size: Decimal) -> Decimal {
    size.round_dp_with_strategy(market.sz_decimals as u32, RoundingStrategy::ToZero)
}

fn spot_size(market: &SpotMarket, size: Decimal) -> Decimal {
    size.round_dp_with_strategy(market.base().sz_decimals as u32, RoundingStrategy::ToZero)
}

fn quote(market: &PerpMarket) -> anyhow::Result<(Decimal, Decimal)> {
    let bid = market
        .round_by_side(Side::Bid, dec!(93231.23), true)
        .ok_or_else(|| anyhow::anyhow!("invalid price"))?;
    let ask = market
        .round_by_side(Side::Ask, dec!(93240.77), true)
        .ok_or_else(|| anyhow::anyhow!("invalid price"))?;
    Ok((bid, ask))
}

fn main() {}
```

## Prices and books

```rust
pub async fn all_mids(&self, dex_name: Option<String>) -> Result<HashMap<String, Decimal>>
pub async fn l2_book(&self, coin: String, n_sig_figs: Option<u8>, mantissa: Option<u8>) -> Result<L2Book>
pub async fn recent_trades(&self, coin: String) -> Result<Vec<Trade>>
pub async fn candle_snapshot(&self, coin: impl Into<String>, interval: CandleInterval, start_time: u64, end_time: u64) -> Result<Vec<Candle>>
pub async fn funding_history(&self, coin: impl Into<String>, start_time: u64, end_time: Option<u64>) -> Result<Vec<FundingRate>>
pub async fn predicted_fundings(&self) -> Result<Vec<(String, Vec<(String, Option<PredictedFundingVenue>)>)>>
pub async fn active_asset_data(&self, user: Address, coin: String) -> Result<ActiveAssetData>
pub async fn meta_and_asset_ctxs(&self, dex: Option<String>) -> Result<serde_json::Value>   // untyped
```

- `all_mids` keys are coin strings (perp name, spot market name). `None` = main DEX.
- `L2Book`: `coin`, `time`, `snapshot: bool`, `levels: [Vec<BookLevel>; 2]`. Helpers:
  `bids()` (high→low), `asks()` (low→high), `best_bid()`, `best_ask()`, `mid()`, `spread()`,
  all returning `Option` where a side can be empty. `BookLevel { px, sz, n }`.
- `candle_snapshot` returns at most the most recent 5000 candles; times are milliseconds.
  `Candle` fields: `open_time`, `close_time`, `coin`, `interval`, `open`, `high`, `low`,
  `close`, `volume`, `num_trades`.
- `funding_history` returns at most 500 records; paginate by using the last `time` as the next
  `start_time`. `FundingRate::annualized_rate()` multiplies by 24 × 365.
- In `predicted_fundings`, `None` means the coin is not listed on that venue, not a zero rate.

```rust
use chrono::{Duration, Utc};
use hypersdk::hypercore::{self, CandleInterval};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = hypercore::mainnet();

    let mids = client.all_mids(None).await?;
    let btc_mid = mids.get("BTC").copied();

    let book = client.l2_book("BTC".to_string(), None, None).await?;
    println!("mid={btc_mid:?} best_bid={:?} best_ask={:?} spread={:?}",
        book.best_bid().map(|l| l.px), book.best_ask().map(|l| l.px), book.spread());

    let end_time = Utc::now().timestamp_millis() as u64;
    let start_time = (Utc::now() - Duration::hours(25)).timestamp_millis() as u64;
    let candles = client
        .candle_snapshot("BTC", CandleInterval::FifteenMinutes, start_time, end_time)
        .await?;
    if let Some(last) = candles.last() {
        println!("last 15m close {}", last.close);
    }
    Ok(())
}
```

`CandleInterval` variants: `OneMinute`, `ThreeMinutes`, `FiveMinutes`, `FifteenMinutes`,
`ThirtyMinutes`, `OneHour`, `TwoHours`, `FourHours`, `EightHours`, `TwelveHours`, `OneDay`,
`ThreeDays`, `OneWeek`, `OneMonth`. It implements `Display` / `FromStr` as `"15m"` etc. The
WebSocket `Subscription::Candle` takes that string, not the enum.
