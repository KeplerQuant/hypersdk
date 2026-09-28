# Recipes (complete programs from `examples/`)

Each program below is an example from `examples/hypercore/`, with the example-only `clap` /
`credentials.rs` plumbing replaced by environment variables. Everything else is as the example
does it.

`examples/` has **no** program that combines a live WebSocket price with order placement, or
that manages a position over time. For those, combine websocket.md with orders.md and keep
the lifecycle rules in websocket.md in mind (drain the stream, spawn HTTP calls).

## 1. Place → amend → cancel (`send_order.rs`)

Places a post-only BTC bid, moves it with `modify`, then cancels the new oid. Note the new oid
after a modify: the cancel uses the oid from the modify response, not the original.

```rust
use std::str::FromStr;

use hypersdk::hypercore::{
    self as hypercore, BatchCancel, BatchModify, Cancel, Cloid, Modify, NonceHandler, OidOrCloid,
    PrivateKeySigner,
    types::{BatchOrder, OrderGrouping, OrderRequest, OrderTypePlacement, TimeInForce},
};
use rust_decimal::dec;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let client = hypercore::mainnet();

    let perps = client.perps().await?;
    let btc = perps.iter().find(|perp| perp.name == "BTC").expect("btc");

    let nonce = NonceHandler::default();

    let resp = client
        .place(
            &signer,
            BatchOrder {
                orders: vec![OrderRequest {
                    asset: btc.index,
                    is_buy: true,
                    limit_px: dec!(87_000),
                    sz: dec!(0.01),
                    reduce_only: false,
                    order_type: OrderTypePlacement::Limit { tif: TimeInForce::Alo },
                    cloid: Cloid::random(),
                }],
                grouping: OrderGrouping::Na,
                builder: None,
            },
            nonce.next(),
            None,
            None,
        )
        .await?;

    match &resp[0] {
        hypercore::OrderResponseStatus::Resting { oid, cloid: _cloid } => {
            let resp = client
                .modify(
                    &signer,
                    BatchModify {
                        modifies: vec![Modify {
                            oid: OidOrCloid::Left(*oid),
                            order: OrderRequest {
                                asset: btc.index,
                                is_buy: true,
                                limit_px: dec!(88_000),
                                sz: dec!(0.01),
                                reduce_only: false,
                                order_type: OrderTypePlacement::Limit { tif: TimeInForce::Alo },
                                cloid: Cloid::random(),
                            },
                        }],
                        always_place: false,
                    },
                    nonce.next(),
                    None,
                    None,
                )
                .await?;

            match &resp[0] {
                hypercore::OrderResponseStatus::Resting { oid, cloid: _cloid } => {
                    client
                        .cancel(
                            &signer,
                            BatchCancel { cancels: vec![Cancel { asset: btc.index, oid: *oid }], fast: false },
                            nonce.next(),
                            None,
                            None,
                        )
                        .await?;
                }
                _ => println!("failed amending order: {resp:?}"),
            }
        }
        _ => println!("failed placing order: {resp:?}"),
    }

    Ok(())
}
```

The original example also calls `user_role(signer.address())` and passes
`vault_address = Some(master)` when the signer's role is `SubAccount`. That contradicts the
`subaccounts()` doc (master signs, `vault_address` = the subaccount) and is left out here.

## 2. Market order and fill report (`market_order.rs`)

```rust
use std::str::FromStr;

use hypersdk::hypercore::{self as hypercore, NonceHandler, PrivateKeySigner};
use rust_decimal::Decimal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let coin = std::env::var("COIN").unwrap_or_else(|_| "ETH".to_string());
    let buy = true;
    let size = 0.01_f64;
    let price: Decimal = std::env::var("WORST_PRICE")?.parse()?; // worst acceptable, on-tick

    let client = hypercore::testnet();
    let nonce_handler = NonceHandler::default();

    let perps = client.perps().await?;
    let market = perps
        .iter()
        .find(|m| m.name == coin)
        .ok_or_else(|| anyhow::anyhow!("market '{coin}' not found"))?;

    let statuses = client
        .market_open(
            &signer,
            market,
            buy,
            price,
            Decimal::try_from(size)?,
            nonce_handler.next(),
            None,
            None,
            None,
        )
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

## 3. IOC spot buy, then bridge the filled amount (`buy_and_transfer.rs`)

Place the IOC order, inspect its result, then submit one bridge transfer for the amount that
filled. A timeout during the transfer leaves its outcome unknown; reconcile before resending.

```rust
use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use hypersdk::hypercore::{
    self as hypercore, Cloid, PrivateKeySigner,
    types::{BatchOrder, OrderGrouping, OrderRequest, OrderTypePlacement, TimeInForce},
};
use rust_decimal::Decimal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let token = std::env::var("TOKEN")?;
    let price: Decimal = std::env::var("PRICE")?.parse()?;
    let amount: Decimal = std::env::var("AMOUNT")?.parse()?;

    let client = hypercore::mainnet();

    let markets = client.spot().await?;
    let market = markets
        .iter()
        .find(|market| market.tokens[0].name == token && market.tokens[1].name == "USDC")
        .ok_or(anyhow::anyhow!("{token} not found"))?
        .clone();

    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

    let statuses = client
        .place(
            &signer,
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
            nonce,
            None,
            None,
        )
        .await?;

    if let Some(hypercore::types::OrderResponseStatus::Filled { total_sz, .. }) = statuses.first() {
        client
            .transfer_to_evm(&signer, market.tokens[0].clone(), *total_sz, nonce + 1)
            .await?;
        println!("Successful taker order, sent {total_sz} to EVM");
    }
    Ok(())
}
```
