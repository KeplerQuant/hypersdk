# Orders

All signing methods take `signer: &S` where `S: SignerSync` (`PrivateKeySigner` qualifies),
a `nonce: u64` (use `NonceHandler::next()`), `vault_address: Option<Address>` (`None` for your
own account) and `expires_after: Option<chrono::DateTime<Utc>>` (`None` = no expiry).

Every type below is importable from `hypersdk::hypercore::types::*`; `Cloid`, `OidOrCloid`,
`NonceHandler`, `PrivateKeySigner`, `PerpMarket`, `SpotMarket` live in `hypersdk::hypercore`.

## Order types

```rust
pub struct BatchOrder {
    pub orders: Vec<OrderRequest>,
    pub grouping: OrderGrouping,        // Na | NormalTpsl | PositionTpsl | PriorityRate(u32)
    pub builder: Option<Builder>,       // Builder { builder_address: Address, fee: u32 /* tenths of bps */ }
}

pub struct OrderRequest {
    pub asset: usize,                   // market.index
    pub is_buy: bool,
    pub limit_px: Decimal,              // round with market.round_price / round_by_side
    pub sz: Decimal,                    // truncate to sz_decimals
    pub reduce_only: bool,
    pub order_type: OrderTypePlacement,
    pub cloid: Cloid,                   // Cloid::random(), or Default::default() for none
}

pub enum OrderTypePlacement {
    Limit { tif: TimeInForce },         // TimeInForce::{Alo, Ioc, Gtc, FrontendMarket}
    Trigger { is_market: bool, trigger_px: Decimal, tpsl: TpSl },   // TpSl::{Tp, Sl}
}
```

- `TimeInForce::Alo` = post-only (rejected if it would cross), `Ioc` = fill-or-cancel the rest,
  `Gtc` = rest until filled or canceled, `FrontendMarket` = what `market_open` uses.
- `Cloid` = `B128`. `Cloid::random()` makes one; a zero/default cloid is left out of the
  request. To derive your own: `Cloid::from(id_u128.to_be_bytes())`.
- `OrderGrouping::PriorityRate(p)` pays a priority tip, `p` in 1/10_000_000 of filled notional
  (max `80_000` = 8 bps); per its doc every order in the batch must be IOC.

### Response: `OrderResponseStatus`

```rust
pub enum OrderResponseStatus {
    Success,
    WaitingForTrigger,
    WaitingForFill,
    Resting { oid: u64, cloid: Option<B128> },
    Filled { total_sz: Decimal, avg_px: Decimal, oid: u64 },
    Error(String),                      // this order was rejected; the batch as a whole was not
}
// helpers: is_ok(), is_err(), error() -> Option<&str>, oid() -> Option<u64>
```

There is one status per submitted order; the examples read `statuses[0]` for a one-order batch.

## place

```rust
pub fn place<S: SignerSync>(
    &self, signer: &S, batch: BatchOrder, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<Cloid>>> + Send + 'static
```

Resting post-only buy, adapted from `examples/hypercore/send_order.rs`:

```rust
use hypersdk::hypercore::{
    self, Cloid, NonceHandler, PerpMarket, PrivateKeySigner,
    types::{BatchOrder, OrderGrouping, OrderRequest, OrderResponseStatus, OrderTypePlacement, TimeInForce},
};
use rust_decimal::Decimal;

/// Returns the oid if the order is resting.
async fn post_only_buy(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    market: &PerpMarket,
    px: Decimal,
    sz: Decimal,
) -> anyhow::Result<Option<u64>> {
    let limit_px = market.round_price(px).ok_or_else(|| anyhow::anyhow!("bad price {px}"))?;
    let cloid = Cloid::random();

    let batch = BatchOrder {
        orders: vec![OrderRequest {
            asset: market.index,
            is_buy: true,
            limit_px,
            sz,
            reduce_only: false,
            order_type: OrderTypePlacement::Limit { tif: TimeInForce::Alo },
            cloid,
        }],
        grouping: OrderGrouping::Na,
        builder: None,
    };

    let statuses = match client.place(signer, batch, nonces.next(), None, None).await {
        Ok(statuses) => statuses,
        Err(err) => {
            // Whole request failed. Outcome may be unknown on a timeout: reconcile by cloid.
            anyhow::bail!("place failed: {} (cloids {:?})", err.message(), err.ids());
        }
    };

    match &statuses[0] {
        OrderResponseStatus::Resting { oid, .. } => Ok(Some(*oid)),
        OrderResponseStatus::Filled { total_sz, avg_px, oid } => {
            println!("#{oid} filled {total_sz} @ {avg_px}");
            Ok(None)
        }
        OrderResponseStatus::Error(msg) => anyhow::bail!("order rejected: {msg}"),
        other => {
            println!("status: {other:?}");
            Ok(None)
        }
    }
}

fn main() {}
```

Closing a position is an opposite-side order with `reduce_only: true` and `sz` = `|szi|`
(see account.md for reading `szi`).

## market_open

```rust
pub async fn market_open<S: SignerSync>(
    &self, signer: &S, market: impl Market, is_buy: bool, limit_px: Decimal, size: Decimal,
    nonce: u64, vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
    builder: Option<Builder>,
) -> Result<Vec<OrderResponseStatus>>        // anyhow::Result
```

`market` accepts `&PerpMarket`, `&SpotMarket`, `&OutcomeMarket` (or owned). `limit_px` is the
worst acceptable price and must already be on a tick. It always sends `reduce_only: false`
and no cloid; build an IOC `place` yourself if you need either.

From `examples/hypercore/market_order.rs`:

```rust
use std::str::FromStr;

use hypersdk::hypercore::{self as hypercore, NonceHandler, PrivateKeySigner, types::Side};
use rust_decimal::dec;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let client = hypercore::testnet();
    let nonce_handler = NonceHandler::default();

    let perps = client.perps().await?;
    let market = perps
        .iter()
        .find(|m| m.name == "ETH")
        .ok_or_else(|| anyhow::anyhow!("market 'ETH' not found"))?;

    // Worst acceptable price, rounded aggressively for a buy.
    let worst = market
        .round_by_side(Side::Bid, dec!(3500), false)
        .ok_or_else(|| anyhow::anyhow!("bad price"))?;

    let statuses = client
        .market_open(&signer, market, true, worst, dec!(0.01), nonce_handler.next(), None, None, None)
        .await?;

    for status in &statuses {
        match status {
            hypercore::OrderResponseStatus::Filled { avg_px, total_sz, oid } => {
                println!("Filled #{oid}: {total_sz} @{avg_px}");
            }
            other => println!("Status: {other:?}"),
        }
    }
    Ok(())
}
```

## cancel / cancel_by_cloid

```rust
pub fn cancel<S: SignerSync>(&self, signer: &S, batch: BatchCancel, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<u64>>> + Send + 'static

pub fn cancel_by_cloid<S: SignerSync>(&self, signer: &S, batch: BatchCancelCloid, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<Cloid>>> + Send + 'static

pub struct BatchCancel      { pub cancels: Vec<Cancel>,        pub fast: bool }
pub struct Cancel           { pub asset: usize, pub oid: u64 }
pub struct BatchCancelCloid { pub cancels: Vec<CancelByCloid>, pub fast: bool }
pub struct CancelByCloid    { pub asset: u32,   pub cloid: B128 }   // note: u32
```

`fast: true` is rejected if any cancel refers to a trigger order; use `false` by default.

```rust
use hypersdk::hypercore::{
    self, Cloid, NonceHandler, PerpMarket, PrivateKeySigner,
    types::{BatchCancel, BatchCancelCloid, Cancel, CancelByCloid, OrderResponseStatus},
};

async fn cancel_oid(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    market: &PerpMarket,
    oid: u64,
) -> anyhow::Result<()> {
    let statuses = client
        .cancel(
            signer,
            BatchCancel { cancels: vec![Cancel { asset: market.index, oid }], fast: false },
            nonces.next(),
            None,
            None,
        )
        .await?; // ActionError<u64> converts into anyhow::Error
    if let Some(OrderResponseStatus::Error(msg)) = statuses.first() {
        anyhow::bail!("cancel of {oid} rejected: {msg}");
    }
    Ok(())
}

async fn cancel_cloid(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    market: &PerpMarket,
    cloid: Cloid,
) -> anyhow::Result<()> {
    let batch = BatchCancelCloid {
        cancels: vec![CancelByCloid { asset: market.index as u32, cloid }],
        fast: false,
    };
    let statuses = client.cancel_by_cloid(signer, batch, nonces.next(), None, None).await?;
    for status in statuses.iter().filter(|s| s.is_err()) {
        println!("cancel rejected: {:?}", status.error());
    }
    Ok(())
}

fn main() {}
```

## modify (batch) / modify_order (single)

```rust
pub fn modify<S: SignerSync>(&self, signer: &S, batch: BatchModify, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<OidOrCloid>>> + Send + 'static

pub async fn modify_order<S: SignerSync>(&self, signer: &S, modify: ModifyAction, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> Result<Vec<OrderResponseStatus>>        // anyhow::Result

pub struct BatchModify  { pub modifies: Vec<Modify>, pub always_place: bool }
pub struct Modify       { pub oid: OidOrCloid, pub order: OrderRequest }
// hypercore::types::api::ModifyAction { pub oid: OidOrCloid, pub order: OrderRequest, pub always_place: bool }
```

- `OidOrCloid` = `either::Either<u64, Cloid>`: `OidOrCloid::Left(oid)` or `OidOrCloid::Right(cloid)`.
- `order` is the complete replacement order (send every field, not only the changed one).
- `always_place: false` (what the example uses): the new order must be a non-trigger ALO, or a
  GTC that would not execute (its TIF is then overridden to ALO). `true` places the new order
  even if the cancel failed.
- A successful modify returns a new `Resting { oid, .. }`; the examples cancel with that oid.

From `examples/hypercore/send_order.rs` (full program in recipes.md):

```rust
use hypersdk::hypercore::{
    self, Cloid, NonceHandler, OidOrCloid, PerpMarket, PrivateKeySigner,
    types::{BatchModify, Modify, OrderRequest, OrderResponseStatus, OrderTypePlacement, TimeInForce},
};
use rust_decimal::Decimal;

/// Moves a resting ALO buy to `new_px`. Returns the new oid.
async fn reprice(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    market: &PerpMarket,
    oid: u64,
    new_px: Decimal,
    sz: Decimal,
) -> anyhow::Result<u64> {
    let resp = client
        .modify(
            signer,
            BatchModify {
                modifies: vec![Modify {
                    oid: OidOrCloid::Left(oid),
                    order: OrderRequest {
                        asset: market.index,
                        is_buy: true,
                        limit_px: new_px,
                        sz,
                        reduce_only: false,
                        order_type: OrderTypePlacement::Limit { tif: TimeInForce::Alo },
                        cloid: Cloid::random(),
                    },
                }],
                always_place: false,
            },
            nonces.next(),
            None,
            None,
        )
        .await?;

    match &resp[0] {
        OrderResponseStatus::Resting { oid, .. } => Ok(*oid),
        other => anyhow::bail!("failed amending order: {other:?}"),
    }
}

fn main() {}
```

## Leverage and margin

```rust
pub async fn update_leverage<S: SignerSync>(&self, signer: &S, asset: usize, is_cross: bool,
    leverage: u32, nonce: u64, vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> Result<()>

pub async fn update_isolated_margin<S: SignerSync>(&self, signer: &S, asset: usize, is_buy: bool,
    ntli: u64, nonce: u64, vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> Result<()>

pub async fn top_up_isolated_only_margin<S: SignerSync>(&self, signer: &S, asset: u32,
    leverage: Decimal, nonce: u64, vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> Result<()>
```

`update_leverage` doc: set it before opening the position. `asset` is `PerpMarket.index`.
`update_isolated_margin`'s `ntli` has no doc comment beyond "Updates isolated margin for a
position"; no example uses it.

```rust
use hypersdk::hypercore::{self, NonceHandler, PerpMarket, PrivateKeySigner};

async fn set_cross_leverage(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    market: &PerpMarket,
    leverage: u32,
) -> anyhow::Result<()> {
    anyhow::ensure!(u64::from(leverage) <= market.max_leverage, "above max leverage");
    client
        .update_leverage(signer, market.index, true, leverage, nonces.next(), None, None)
        .await
}

fn main() {}
```

## Dead-man switch: schedule_cancel

```rust
pub async fn schedule_cancel<S: SignerSync>(&self, signer: &S, nonce: u64, when: DateTime<Utc>,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>,
) -> Result<()>
```

Cancels all open orders at `when`. There is no method to clear it; re-arming just sends a later
time.

```rust
use chrono::{TimeDelta, Utc};
use hypersdk::hypercore::{self, NonceHandler, PrivateKeySigner};

async fn arm_dead_man_switch(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
) -> anyhow::Result<()> {
    let when = Utc::now() + TimeDelta::seconds(60);
    client.schedule_cancel(signer, nonces.next(), when, None, None).await
}

fn main() {}
```

## Trigger orders (TP/SL) — shape only

No example or test in the repo places a trigger order. The type is:

```rust
use hypersdk::hypercore::types::{OrderTypePlacement, TpSl};
use rust_decimal::dec;

fn stop_loss_type() -> OrderTypePlacement {
    OrderTypePlacement::Trigger { is_market: true, trigger_px: dec!(90000), tpsl: TpSl::Sl }
}

fn main() {}
```

Bracket grouping exists as `OrderGrouping::NormalTpsl` / `OrderGrouping::PositionTpsl`. Test
on testnet before relying on either.

## TWAP

```rust
pub async fn twap_order<S: SignerSync>(&self, signer: &S, params: TwapOrderParams, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>) -> Result<Response>
pub async fn twap_cancel<S: SignerSync>(&self, signer: &S, asset: usize, twap_id: u64, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>) -> Result<Response>

// hypercore::types::TwapOrderParams
pub struct TwapOrderParams { pub a: usize /* asset */, pub b: bool /* is_buy */, pub s: Decimal /* size */,
                             pub r: bool /* reduce_only */, pub m: u32 /* minutes */, pub t: bool /* randomize */ }
```

Caution: the response is parsed into `Response`, and `OkResponse` only has `Order`, `Cancel`,
`CreateSubAccount`, `CreateVault` and `Default` variants. A successful TWAP reply that uses a
different `type` will fail to deserialize and surface as an `Err` with a `body=…` context even
though the TWAP was accepted. No example exercises TWAP; verify on testnet, and track TWAPs via
the `UserTwapHistory` WebSocket feed or `twap_history` rather than this return value.

## Spawning order futures

`place`, `cancel`, `cancel_by_cloid` and `modify` sign when called and return a `'static`
future, so you can fire the request and keep working. From
`examples/hypercore/buy_and_transfer.rs`:

```rust
use hypersdk::hypercore::{
    self, Cloid, NonceHandler, PrivateKeySigner, SpotMarket,
    types::{BatchOrder, OrderGrouping, OrderRequest, OrderResponseStatus, OrderTypePlacement, TimeInForce},
};
use rust_decimal::Decimal;

fn fire_ioc_buy(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    market: &SpotMarket,
    price: Decimal,
    amount: Decimal,
) -> tokio::task::JoinHandle<()> {
    // Signing happens here; the returned future owns everything it needs.
    let future = client.place(
        signer,
        BatchOrder {
            orders: vec![OrderRequest {
                asset: market.index,
                is_buy: true,
                limit_px: price,
                sz: amount,
                reduce_only: false,
                order_type: OrderTypePlacement::Limit { tif: TimeInForce::Ioc },
                cloid: Cloid::random(),
            }],
            grouping: OrderGrouping::Na,
            builder: None,
        },
        nonces.next(),
        None,
        None,
    );

    tokio::spawn(async move {
        match future.await {
            Ok(placements) => {
                if let OrderResponseStatus::Filled { total_sz, .. } = &placements[0] {
                    println!("Successful taker order, filled {total_sz}");
                }
            }
            Err(err) => eprintln!("place failed: {err}"),
        }
    })
}

fn main() {}
```
