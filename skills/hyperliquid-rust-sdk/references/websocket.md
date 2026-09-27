# WebSocket

`hypersdk::hypercore::WebSocket` (= `hypercore::ws::Connection`). Get one from
`client.websocket()`, `hypercore::mainnet_ws()` / `testnet_ws()`, or
`WebSocket::new(url)`.

## Lifecycle — read this before writing a loop

Taken from `src/hypercore/ws.rs`:

1. **Construction connects immediately** in a background task started with `tokio::spawn`. Create
   it inside a Tokio runtime.
2. **`subscribe` / `unsubscribe` / `post` are synchronous and return `()`.** They queue a
   command; nothing is awaited and no error is returned. Subscribing twice to the same
   `Subscription` is a no-op.
3. **The server's answer arrives on the stream:** `Incoming::SubscriptionResponse(_)` when
   accepted, `Incoming::Error(String)` when rejected. A rejected or removed subscription is
   otherwise silent: it just never sends data. Match `Incoming::Error`.
4. **Reconnects are automatic.** On a drop the stream yields `Event::Disconnected`, reconnects
   with backoff (500 ms doubling to a 5 s cap, 10 s connect timeout), yields `Event::Connected`,
   and **re-sends every active subscription**. Do not resubscribe yourself.
5. **Nothing is replayed.** Messages pushed while disconnected are lost, and `post` requests are
   never retried. After a `Disconnected` → `Connected` pair, re-sync state that must be exact
   (open orders, positions) over HTTP.
6. **Heartbeats are internal.** The task pings every 5 s and reconnects after 2 missed pongs;
   server `Ping`/`Pong` never reach you.
7. **The event channel is unbounded.** Keep draining the stream; if your loop blocks, memory
   grows. Move slow work (HTTP calls, signing) into spawned tasks.
8. **Unparseable frames are dropped** and logged at `warn` through the `log` crate. Install a
   logger (e.g. `env_logger`) to see them.
9. **Shutdown:** the background task exits when every `Connection` / `ConnectionHandle` /
   `ConnectionStream` is dropped (`close()` just drops). While you hold one, `next()` never
   returns `None`, so a `while let Some(..)` loop runs until you `break`.

```rust
pub enum Event { Connected, Disconnected, Message(Incoming) }

impl Connection {
    pub fn new(url: Url) -> Self
    pub fn subscribe(&self, subscription: Subscription)
    pub fn unsubscribe(&self, subscription: Subscription)
    pub fn post(&self, id: u64, request: PostRequest)
    pub fn close(self)
    pub fn split(self) -> (ConnectionHandle, ConnectionStream)
}
// Connection and ConnectionStream implement futures::Stream<Item = Event>  (use futures::StreamExt)
// ConnectionHandle: Clone, has subscribe / unsubscribe / post / close
```

## Subscriptions and what they deliver

| `Subscription` | `Incoming` variant |
| --- | --- |
| `Bbo { coin }` | `Bbo(Bbo)` — `bid()`, `ask()`, `mid()`, `spread()` |
| `Trades { coin }` | `Trades(Vec<Trade>)` — `side` is the taker side |
| `L2Book { coin, n_sig_figs: Option<u8>, mantissa: Option<u8>, fast: bool }` | `L2Book(L2Book)` — `fast: true` = 5 levels ~every 0.5 s |
| `Candle { coin, interval: String }` | `Candle(Candle)` — interval as `"1m"`, `"15m"`, … |
| `AllMids { dex: Option<String> }` | `AllMids { dex, mids: HashMap<String, Decimal> }` |
| `ActiveAssetCtx { coin }` | `ActiveAssetCtx { coin, ctx: AssetContext }` (`funding`, `open_interest`, `mark_px`, `oracle_px`, `mid_px`, …); `Incoming::ActiveSpotAssetCtx` models the spot form |
| `AssetCtxs { dex }` / `SpotAssetCtxs` / `AllDexsAssetCtxs` | `AssetCtxs { dex, ctxs }` / `SpotAssetCtxs(..)` / `AllDexsAssetCtxs { ctxs }` |
| `FastAssetCtxs` | `FastAssetCtxs(HashMap<String, FastAssetCtx>)` — first message is a snapshot, then changes only |
| `OrderUpdates { user }` | `OrderUpdates(Vec<OrderUpdate<WsBasicOrder>>)` |
| `UserFills { user }` | `UserFills { is_snapshot, user, fills: Vec<Fill> }` |
| `UserEvents { user }` | `UserEvents(UserEvent)` — `Fills`, `Funding`, `Liquidation`, `NonUserCancel`, `Unknown` |
| `OpenOrders { user, dex }` | `OpenOrders { dex, user, orders: Vec<OpenOrder> }` |
| `ClearinghouseState { user, dex }` | `ClearinghouseState { dex, user, clearinghouse_state }` |
| `AllDexsClearinghouseState { user }` | `AllDexsClearinghouseState { user, clearinghouse_states }` |
| `SpotState { user, is_portfolio_margin }` | `SpotState { user, spot_state }` |
| `ActiveAssetData { user, coin }` | `ActiveAssetData(ActiveAssetData)` |
| `UserHistoricalOrders { user }` | `UserHistoricalOrders { is_snapshot, user, order_history }` |
| `UserFundings { user }`, `UserNonFundingLedgerUpdates { user }` | same-named variants with `is_snapshot` |
| `UserTwapSliceFills { user }`, `UserTwapHistory { user }`, `TwapStates { user, dex }` | same-named variants |
| `Notification { user }`, `WebData3 { user }`, `OutcomeMetaUpdates` | same-named variants (`WebData3` is untyped JSON) |

- `coin` strings are the same as for HTTP info: `"BTC"`, `"xyz:XYZ100"`, spot `market.name`,
  outcome `market.coin()`.
- `OrderUpdate<WsBasicOrder>`: `status: OrderStatus`, `status_timestamp`, `order` with
  `coin`, `side`, `limit_px`, `sz` (remaining), `oid`, `orig_sz`, `cloid`. No tif/order type.
- Feeds carrying `is_snapshot` send an initial snapshot flagged `true`. When reacting to new
  fills, skip messages where `is_snapshot` is `true`.
- There is no `WebData2` subscription (removed by the exchange); use `WebData3`.

## Market-data loop

Adapted from `examples/hypercore/websocket-candles.rs` and `websocket.rs`:

```rust
use futures::StreamExt;
use hypersdk::hypercore::{
    self,
    types::{Incoming, Subscription},
    ws::Event,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = hypercore::mainnet();
    let mut ws = client.websocket();

    ws.subscribe(Subscription::Bbo { coin: "BTC".to_string() });
    ws.subscribe(Subscription::Trades { coin: "BTC".to_string() });
    ws.subscribe(Subscription::Candle { coin: "BTC".to_string(), interval: "1m".to_string() });
    ws.subscribe(Subscription::AllMids { dex: None });

    while let Some(event) = ws.next().await {
        match event {
            Event::Connected => println!("WebSocket connected"),
            // Subscriptions are restored automatically; anything sent meanwhile is lost.
            Event::Disconnected => println!("WebSocket disconnected, reconnecting"),
            Event::Message(msg) => match msg {
                Incoming::SubscriptionResponse(_) => {}
                Incoming::Error(err) => eprintln!("server rejected a request: {err}"),
                Incoming::Bbo(bbo) => {
                    if let (Some(mid), Some(spread)) = (bbo.mid(), bbo.spread()) {
                        println!("{} mid={mid} spread={spread}", bbo.coin);
                    }
                }
                Incoming::Trades(trades) => {
                    for trade in trades {
                        println!("{} {} {} @ {}", trade.coin, trade.side, trade.sz, trade.px);
                    }
                }
                Incoming::Candle(candle) => {
                    println!("{} {} close={} vol={}", candle.coin, candle.interval, candle.close, candle.volume);
                }
                Incoming::AllMids { dex: _, mids } => {
                    if let Some(price) = mids.get("ETH") {
                        println!("ETH mid {price}");
                    }
                }
                _ => {}
            },
        }
    }
    Ok(())
}
```

## Account feeds

Adapted from `examples/hypercore/websocket-user-events.rs`:

```rust
use futures::StreamExt;
use hypersdk::{
    Address,
    hypercore::{
        self,
        types::{Incoming, Subscription, UserEvent},
        ws::Event,
    },
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let user: Address = std::env::var("ACCOUNT")?.parse()?;
    let client = hypercore::mainnet();
    let mut ws = client.websocket();

    ws.subscribe(Subscription::OrderUpdates { user });
    ws.subscribe(Subscription::UserFills { user });
    ws.subscribe(Subscription::UserEvents { user });
    ws.subscribe(Subscription::ClearinghouseState { user, dex: None });

    while let Some(event) = ws.next().await {
        match event {
            Event::Connected => println!("Connected"),
            Event::Disconnected => println!("Disconnected, reconnecting..."),
            Event::Message(msg) => match msg {
                Incoming::OrderUpdates(updates) => {
                    for update in updates {
                        println!("order {} {:?} {} left={}", update.order.oid, update.order.cloid, update.status, update.order.sz);
                    }
                }
                Incoming::UserFills { is_snapshot, fills, .. } => {
                    if is_snapshot {
                        continue; // history sent on subscribe, not new fills
                    }
                    for fill in fills {
                        println!("fill {} {} @ {} oid={} fee={}", fill.coin, fill.sz, fill.px, fill.oid, fill.fee);
                    }
                }
                Incoming::UserEvents(user_event) => match user_event {
                    UserEvent::Fills { fills } => println!("userEvents.fills: {} fill(s)", fills.len()),
                    UserEvent::Funding { funding } => {
                        println!("funding {} usdc={} rate={}", funding.coin, funding.usdc, funding.funding_rate)
                    }
                    UserEvent::Liquidation { liquidation } => {
                        println!("liquidation lid={} ntl_pos={}", liquidation.lid, liquidation.liquidated_ntl_pos)
                    }
                    UserEvent::NonUserCancel { non_user_cancel } => {
                        for c in non_user_cancel {
                            println!("exchange canceled {} #{}", c.coin, c.oid);
                        }
                    }
                    UserEvent::Unknown(raw) => println!("userEvents.unknown: {raw}"),
                },
                Incoming::ClearinghouseState { clearinghouse_state, .. } => {
                    println!("account value {}", clearinghouse_state.margin_summary.account_value);
                }
                Incoming::Error(err) => eprintln!("rejected: {err}"),
                _ => {}
            },
        }
    }
    Ok(())
}
```

## Driving the stream and subscriptions from different tasks

`split()` gives a cloneable `ConnectionHandle` for subscribing and a `ConnectionStream` for
reading. From the `ConnectionHandle` doc example:

```rust
use std::time::Duration;

use futures::StreamExt;
use hypersdk::hypercore::{self, types::*, ws::Event};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ws = hypercore::mainnet_ws();
    let (handle, mut stream) = ws.split();

    tokio::spawn(async move {
        handle.subscribe(Subscription::Trades { coin: "BTC".into() });
        handle.subscribe(Subscription::L2Book { coin: "ETH".into(), n_sig_figs: None, mantissa: None, fast: false });

        tokio::time::sleep(Duration::from_secs(60)).await;
        handle.unsubscribe(Subscription::Trades { coin: "BTC".into() });
        // Dropping `handle` here is fine: `stream` keeps the connection alive.
    });

    while let Some(event) = stream.next().await {
        match event {
            Event::Message(Incoming::Trades(trades)) => println!("Received {} trades", trades.len()),
            Event::Message(Incoming::L2Book(book)) => println!("{} best bid {:?}", book.coin, book.best_bid().map(|l| l.px)),
            _ => {}
        }
    }
    Ok(())
}
```

## Checking that a subscription was accepted

The pattern the crate's own live audit (`subscriptions_are_still_accepted`) uses:

```rust
use std::time::Duration;

use futures::StreamExt;
use hypersdk::hypercore::{self, types::{Incoming, Subscription}, ws::Event};

async fn confirm(sub: Subscription) -> anyhow::Result<()> {
    let mut ws = hypercore::mainnet().websocket();
    ws.subscribe(sub.clone());

    let outcome = tokio::time::timeout(Duration::from_secs(15), async {
        while let Some(event) = ws.next().await {
            match event {
                Event::Message(Incoming::SubscriptionResponse(_)) => return Ok(()),
                Event::Message(Incoming::Error(err)) => return Err(err),
                _ => continue,
            }
        }
        Err("stream ended".to_string())
    })
    .await;

    match outcome {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => anyhow::bail!("{sub} rejected: {err}"),
        Err(_) => anyhow::bail!("{sub}: no subscription response in 15s"),
    }
}

fn main() {}
```

## Posting requests over the socket

```rust
pub enum PostRequest { Info(serde_json::Value), Action(Box<ActionRequest>) }
// reply: Incoming::Post(PostResponse { id: u64, response: PostResponsePayload })
pub enum PostResponsePayload { Info(serde_json::Value), Action(Response), Error(String) }
```

`id` is echoed on the reply; use distinct ids. Replies are not guaranteed across reconnects, so
time out and retry. `PostResponsePayload::Info` wraps the result one level deeper than HTTP:
read `value["data"]`. Info requests, from `examples/hypercore/websocket_post.rs` (needs
`serde_json`):

```rust
use std::collections::HashMap;

use futures::StreamExt;
use hypersdk::hypercore::{
    self as hypercore,
    types::{Incoming, PostRequest, PostResponsePayload},
    ws::Event,
};
use serde_json::json;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut ws = hypercore::mainnet().websocket();

    let mut pending: HashMap<u64, &str> = HashMap::new();
    pending.insert(1, "meta");
    pending.insert(2, "l2Book(BTC)");

    ws.post(1, PostRequest::Info(json!({ "type": "meta" })));
    ws.post(2, PostRequest::Info(json!({ "type": "l2Book", "coin": "BTC" })));

    while let Some(event) = ws.next().await {
        let Event::Message(Incoming::Post(post)) = event else {
            continue;
        };
        let label = pending.remove(&post.id).unwrap_or("<unknown id>");
        match post.response {
            PostResponsePayload::Info(value) => println!("[{}] {label}: {}", post.id, value["data"]),
            PostResponsePayload::Action(response) => println!("[{}] {label}: {response:?}", post.id),
            PostResponsePayload::Error(err) => println!("[{}] {label} failed: {err}", post.id),
        }
        if pending.is_empty() {
            break;
        }
    }
    Ok(())
}
```

Signed actions: sign with `Action::sign_sync` and post the `ActionRequest`. This is public API
but **no example or test exercises it**; it is compile-checked only.

```rust
use hypersdk::hypercore::{
    self, Cloid, NonceHandler, PerpMarket, PrivateKeySigner, WebSocket,
    types::{
        Action, BatchOrder, OkResponse, OrderGrouping, OrderRequest, OrderTypePlacement, PostRequest,
        PostResponse, PostResponsePayload, Response, TimeInForce,
    },
};
use rust_decimal::Decimal;

/// Signs and queues the order; returns the post id. The reply arrives on the event stream.
fn post_order(
    ws: &WebSocket,
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    market: &PerpMarket,
    px: Decimal,
    sz: Decimal,
) -> anyhow::Result<u64> {
    let batch = BatchOrder {
        orders: vec![OrderRequest {
            asset: market.index,
            is_buy: true,
            limit_px: px,
            sz,
            reduce_only: false,
            order_type: OrderTypePlacement::Limit { tif: TimeInForce::Alo },
            cloid: Cloid::random(),
        }],
        grouping: OrderGrouping::Na,
        builder: None,
    };
    let nonce = nonces.next();
    let request = Action::Order(batch).sign_sync(signer, nonce, None, None, client.chain())?;
    ws.post(nonce, PostRequest::Action(Box::new(request)));
    Ok(nonce)
}

/// Call from your main event loop on `Event::Message(Incoming::Post(post))`.
fn on_post_reply(post: PostResponse) {
    match post.response {
        PostResponsePayload::Action(Response::Ok(OkResponse::Order { statuses })) => {
            println!("post {}: {statuses:?}", post.id)
        }
        PostResponsePayload::Action(Response::Err(err)) => eprintln!("post {} rejected: {err}", post.id),
        PostResponsePayload::Action(other) => eprintln!("post {}: unexpected {other:?}", post.id),
        PostResponsePayload::Error(err) => eprintln!("post {} failed: {err}", post.id),
        PostResponsePayload::Info(value) => println!("post {}: {}", post.id, value["data"]),
    }
}

fn main() {}
```
