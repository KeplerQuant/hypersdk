//! # hypersdk
//!
//! A comprehensive Rust SDK for interacting with the Hyperliquid protocol.
//!
//! Hyperliquid is a high-performance decentralized exchange with two main components:
//! - **HyperCore**: The native L1 chain with perpetual and spot markets
//! - **HyperEVM**: An Ethereum-compatible layer for DeFi integrations
//!
//! ## Features
//!
//! - Full HyperCore API support (HTTP and WebSocket)
//! - Trading operations (orders, cancellations, modifications)
//! - Real-time market data via WebSocket subscriptions
//! - Asset transfers between perps, spot, and EVM
//! - HyperEVM contract interactions (Morpho, Uniswap)
//! - Type-safe EIP-712 signing for all operations
//!
//! ## Quick Start
//!
//! ### HyperCore - Place an Order
//!
//! Create a mainnet client with `hypercore::mainnet()` to query markets and user balances.
//!
//! ### HyperCore - WebSocket Subscriptions
//!
//! ```no_run
//! use hypersdk::hypercore::{self, types::*};
//! use futures::StreamExt;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let mut ws = hypercore::mainnet_ws();
//!
//! // Subscribe to market data
//! ws.subscribe(Subscription::Trades { coin: "BTC".into() });
//! ws.subscribe(Subscription::L2Book { coin: "ETH".into() });
//!
//! // Process incoming messages
//! while let Some(msg) = ws.next().await {
//!     match msg {
//!         Incoming::Trades(trades) => println!("Trades: {:?}", trades),
//!         Incoming::L2Book(book) => println!("Order book: {:?}", book),
//!         _ => {}
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ### HyperEVM - Morpho Lending
//!
//! Create a Morpho client with `morpho::Client::mainnet().await?` to query APY rates.
//!
//! ### HyperEVM - Uniswap V3
//!
//! Create a Uniswap client with `uniswap::Client::mainnet(contracts).await?` to query pool prices.
//!
//! ## Modules
//!
//! - [`hypercore`]: HyperCore L1 chain interactions (trading, transfers, WebSocket)
//! - [`hyperevm`]: HyperEVM contract interactions (Morpho, Uniswap)

pub mod hypercore;
pub mod hyperevm;

/// Re-exported Ethereum address type from Alloy.
///
/// Used throughout the SDK for representing Ethereum-compatible addresses.
pub use alloy::primitives::{Address, U160, U256, address};
/// Re-exported decimal type from rust_decimal.
///
/// Used for precise numerical operations, especially for prices and quantities.
pub use rust_decimal::Decimal;
