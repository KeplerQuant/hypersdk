//! Place a limit order on a perpetual market.
//!
//! This example demonstrates how to place a buy limit order on the BTC perpetual market.
//! It shows proper price handling, order configuration, and response parsing.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example send_order -- --private-key YOUR_PRIVATE_KEY
//! ```
//!
//! # What it does
//!
//! 1. Connects to Hyperliquid mainnet
//! 2. Finds the BTC perpetual market
//! 3. Places a buy limit order at $87,000 for 0.01 BTC
//! 4. Uses ALO (Add Liquidity Only) to ensure maker execution
//! 5. Prints the order response with order ID
//!
//! # Order Configuration
//!
//! - Market: BTC perpetual
//! - Side: Buy
//! - Price: $87,000
//! - Size: 0.01 BTC
//! - Type: Limit with ALO (Add Liquidity Only)
//! - Reduce Only: false (can increase position)

use std::{
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use hypersdk::hypercore::{
    self as hypercore, Cloid,
    types::{BatchOrder, OrderGrouping, OrderRequest, OrderTypePlacement, TimeInForce},
};
use rust_decimal::dec;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    private_key: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = simple_logger::init_with_level(log::Level::Debug);

    let args = Cli::parse();

    let client = hypercore::mainnet();
    let signer = hypercore::PrivateKeySigner::from_str(&args.private_key)?;

    let perps = client.perps().await?;
    let btc = perps.iter().find(|perp| perp.name == "BTC").expect("btc");

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
                    order_type: OrderTypePlacement::Limit {
                        tif: TimeInForce::Alo,
                    },
                    cloid: Cloid::random(),
                }],
                grouping: OrderGrouping::Na,
            },
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            None,
            None,
        )
        .await?;

    println!("{resp:?}");

    Ok(())
}
