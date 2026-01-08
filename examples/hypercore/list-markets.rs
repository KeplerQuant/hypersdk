//! List all available spot markets on Hyperliquid.
//!
//! This example demonstrates how to query all spot trading pairs and display
//! their basic information including market name and token pairs.
//!
//! # Usage
//!
//! ```bash
//! cargo run --example list-markets
//! ```
//!
//! # Output
//!
//! ```text
//! PURR-SPOT    PURR/USDC
//! HYPE-SPOT    HYPE/USDC
//! ...
//! ```

use hypersdk::hypercore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Create a mainnet client
    let client = hypercore::mainnet();

    // Fetch all spot markets
    let markets = client.spot().await?;

    // Display market information
    for market in markets {
        println!(
            "{}\t{}/{}",
            market.name, market.tokens[0].name, market.tokens[1].name
        );
    }

    Ok(())
}
