//! HyperCore interaction.

pub mod http;
pub mod types;
pub mod ws;

use std::{fmt, hash::Hash, ops::Range};

use alloy::primitives::{B128, U256, address};
/// Reimport signers.
pub use alloy::signers::local::PrivateKeySigner;
use either::Either;
use reqwest::IntoUrl;
use rust_decimal::{Decimal, MathematicalOps};
use serde::Deserialize;
use url::Url;

use crate::{
    Address,
    hyperevm::{from_wei, to_wei},
};

/// Client order id.
pub type Cloid = B128;

/// Order ID or client order ID.
pub type OidOrCloid = Either<u64, Cloid>;

/// Reimport the http::Client.
pub use http::Client as HttpClient;
/// Reimport the ws::Connection.
pub use ws::Connection as WebSocket;

/// Arbitrum signature chain id.
pub const ARBITRUM_SIGNATURE_CHAIN_ID: &str = "0xa4b1";

/// USDC's contract differs from the one linked in HyperCore.
pub const USDC_CONTRACT_IN_EVM: Address = address!("0xb88339CB7199b77E23DB6E890353E22632Ba630f");

/// Returns a mainnet client.
#[inline(always)]
pub fn mainnet() -> HttpClient {
    HttpClient::new(mainnet_url())
}

/// Returns a mainnet client.
#[inline(always)]
pub fn mainnet_ws() -> WebSocket {
    WebSocket::new(mainnet_websocket_url())
}

/// Returns the default mainnet base url.
#[inline(always)]
pub fn mainnet_url() -> Url {
    "https://api.hyperliquid.xyz".parse().unwrap()
}

/// Returns the default mainnet base url.
#[inline(always)]
pub fn mainnet_websocket_url() -> Url {
    "wss://api.hyperliquid.xyz/ws".parse().unwrap()
}

/// Price ticks
#[derive(Debug, Clone)]
pub struct PriceTickTable {
    values: Vec<(Range<Decimal>, Decimal)>,
}

impl PriceTickTable {
    /// Returns the tick size for a price.
    pub fn tick_for(&self, price: Decimal) -> Decimal {
        self.values
            .iter()
            .find_map(|(range, tick)| {
                if range.contains(&price) {
                    Some(*tick)
                } else {
                    None
                }
            })
            .expect("range")
    }
}

/// https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/tick-and-lot-size
fn build_price_ticks(sz_decimals: i64) -> PriceTickTable {
    let max_decimals = -8 + sz_decimals;
    let mut ticks = vec![];

    for i in max_decimals..=0 {
        let tick_size = Decimal::TEN.powi(i);
        let from_range = if i == -8 {
            Decimal::ZERO
        } else {
            Decimal::TEN.powi(i + 4)
        };
        let to_range = if i == 0 {
            Decimal::MAX
        } else {
            Decimal::TEN.powi(i + 5)
        };
        ticks.push((from_range..to_range, tick_size));
    }

    PriceTickTable { values: ticks }
}

/// Perpetual tradeable instrument.
#[derive(Debug, Clone)]
pub struct PerpMarket {
    /// Market name
    pub name: String,
    /// Market index
    pub index: usize,
    /// Decimals supported by the market
    pub sz_decimals: i64,
    /// Collateral currency
    pub collateral: SpotToken,
    /// Max allowed leverage
    pub max_leverage: u64,
    /// Margin isolated
    pub isolated_margin: bool,
    /// Margin mode
    pub margin_mode: Option<MarginMode>,
    // TDOO: implement margin tables
}

/// Spot tradeable instrument.
#[derive(Debug, Clone)]
pub struct SpotMarket {
    /// Market name
    pub name: String,
    /// Market index
    pub index: usize,
    /// Base and quote
    pub tokens: [SpotToken; 2],
    /// Price ticks table
    pub table: PriceTickTable,
}

impl PartialEq for SpotMarket {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for SpotMarket {}

/// A tradeable spot market in HyperCore.
#[derive(Debug, Clone)]
pub struct SpotToken {
    /// Standard name.
    pub name: String,
    /// Token index.
    pub index: u32,
    /// Token Id in core.
    pub token_id: B128,
    /// EVM contract address if any.
    pub evm_contract: Option<Address>,
    /// The address to send the funds from core to evm and viceversa.
    ///
    /// HYPE is a special case and it will not have an [`SpotToken::evm_contract`] set
    /// but will have [`SpotToken::cross_chain_address`] set.
    pub cross_chain_address: Option<Address>,
    /// Decimals supported by the token
    pub sz_decimals: i64,
    /// Wei decimals
    pub wei_decimals: i64,
    /// Additional decimals supported by the token in EVM.
    ///
    /// Total decimals of the contract should be sz_decimals + evm_extra_decimals.
    pub evm_extra_decimals: i64,
}

impl SpotToken {
    /// Converts any size to wei.
    pub fn to_wei(&self, size: Decimal) -> U256 {
        to_wei(size, (self.wei_decimals + self.evm_extra_decimals) as u32)
    }

    /// Converts wei to Decimal.
    pub fn from_wei(&self, size: U256) -> Decimal {
        from_wei(size, (self.wei_decimals + self.evm_extra_decimals) as u32)
    }

    /// Returns whether the token is linked to EVM or not.
    #[inline(always)]
    pub fn is_evm_linked(&self) -> bool {
        self.evm_contract.is_some()
    }
}

impl Hash for SpotToken {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.token_id.hash(state);
    }
}

impl PartialEq for SpotToken {
    fn eq(&self, other: &Self) -> bool {
        self.token_id == other.token_id
    }
}

impl Eq for SpotToken {}

impl fmt::Display for SpotToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

async fn raw_spot_markets(
    core_url: impl IntoUrl,
    client: reqwest::Client,
) -> anyhow::Result<SpotTokens> {
    let mut url = core_url.into_url()?;
    url.set_path("/info");

    let resp = client
        .post(url)
        .json(&serde_json::json!({
            "type": "spotMeta"
        }))
        .send()
        .await?;
    Ok(resp.json().await?)
}

/// Gather spot tokens from HyperCore.
pub async fn spot_tokens(
    core_url: impl IntoUrl,
    client: reqwest::Client,
) -> anyhow::Result<Vec<SpotToken>> {
    let data = raw_spot_markets(core_url, client).await?;

    let spot_tokens: Vec<_> = data.tokens.iter().cloned().map(SpotToken::from).collect();
    Ok(spot_tokens)
}

/// Gather spot markets from HyperCore.
pub async fn spot_markets(
    core_url: impl IntoUrl,
    client: reqwest::Client,
) -> anyhow::Result<Vec<SpotMarket>> {
    let data = raw_spot_markets(core_url, client).await?;
    let mut markets = Vec::with_capacity(data.universe.len());

    let spot_tokens: Vec<_> = data.tokens.iter().cloned().map(SpotToken::from).collect();

    for item in data.universe {
        let (_, base) = spot_tokens
            .iter()
            .enumerate()
            .find(|(index, _)| *index as u32 == item.tokens[0])
            .expect("base");
        let (_, quote) = spot_tokens
            .iter()
            .enumerate()
            .find(|(index, _)| *index as u32 == item.tokens[1])
            .expect("quote");

        markets.push(SpotMarket {
            name: item.name,
            index: 10_000 + item.index,
            tokens: [base.clone(), quote.clone()],
            table: build_price_ticks(base.sz_decimals),
        });
    }

    Ok(markets)
}

/// Gather perp markets from HyperCore.
pub async fn perp_markets(
    core_url: impl IntoUrl,
    client: reqwest::Client,
) -> anyhow::Result<Vec<PerpMarket>> {
    let mut url = core_url.into_url()?;
    url.set_path("/info");

    // get it to gather the collateral token
    let spot = raw_spot_markets(url.clone(), client.clone()).await?;

    let resp = client
        .post(url)
        .json(&serde_json::json!({
            "type": "meta"
        }))
        .send()
        .await?;
    let data: PerpTokens = resp.json().await?;
    let collateral = &spot.tokens[data.collateral_token];
    let collateral = SpotToken::from(collateral.clone());

    let perps = data
        .universe
        .into_iter()
        .enumerate()
        .map(|(index, perp)| PerpMarket {
            name: perp.name,
            index,
            max_leverage: perp.max_leverage,
            sz_decimals: perp.sz_decimals,
            collateral: collateral.clone(),
            isolated_margin: perp.only_isolated,
            margin_mode: perp.margin_mode,
        })
        .collect();

    Ok(perps)
}

// TODO: perpDexs

// TODO: ideally we use something like Address:from(U256::from(0x100000000..) + index);
fn generate_evm_transfer_address(mut index: usize) -> Address {
    let mut raw = [32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut offset = raw.len() - 1;
    while index != 0 {
        raw[offset] = index as u8;
        index >>= 8;
        offset -= 1;
    }

    Address::from_slice(&raw[..])
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PerpTokens {
    universe: Vec<PerpUniverseItem>,
    collateral_token: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PerpUniverseItem {
    name: String,
    max_leverage: u64,
    #[serde(default)]
    only_isolated: bool,
    margin_mode: Option<MarginMode>,
    sz_decimals: i64,
    // margin_table_id: u64,
}

#[derive(Debug, Copy, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarginMode {
    StrictIsolated,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotTokens {
    universe: Vec<SpotUniverseItem>,
    tokens: Vec<Token>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpotUniverseItem {
    // base and quote
    tokens: [u32; 2],
    name: String,
    index: usize,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Token {
    name: String,
    index: usize,
    token_id: B128,
    sz_decimals: i64,
    wei_decimals: i64,
    evm_contract: Option<EvmContract>,
}

impl From<Token> for SpotToken {
    fn from(token: Token) -> Self {
        let (evm_contract, cross_chain_address, evm_extra_decimals) =
            if let Some(contract) = token.evm_contract {
                (
                    Some(if token.name == "USDC" {
                        // map it to the contract in EVM
                        USDC_CONTRACT_IN_EVM
                    } else {
                        contract.address
                    }),
                    Some(generate_evm_transfer_address(token.index)),
                    contract.evm_extra_wei_decimals,
                )
            } else if token.name == "HYPE" {
                // map it to WHYPE
                (
                    Some(Address::repeat_byte(85)),
                    Some(Address::repeat_byte(34)),
                    10,
                )
            } else {
                (None, None, 0)
            };

        Self {
            name: token.name.clone(),
            token_id: token.token_id,
            index: token.index as u32,
            evm_contract,
            evm_extra_decimals,
            wei_decimals: token.wei_decimals,
            cross_chain_address: if token.name == "HYPE" {
                Some(Address::repeat_byte(34))
            } else {
                cross_chain_address
            },
            sz_decimals: token.sz_decimals,
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct EvmContract {
    address: Address,
    evm_extra_wei_decimals: i64,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use alloy::primitives::address;

    use super::*;
    use crate::hypercore;

    #[tokio::test]
    async fn test_spot_markets() {
        let client = reqwest::Client::new();
        let markets = spot_markets("https://api.hyperliquid.xyz", client)
            .await
            .unwrap();
        assert!(!markets.is_empty());
    }

    #[tokio::test]
    async fn test_evm_send_addresses() {
        let expected_addresses = HashMap::from([
            // PURR
            (
                "PURR/USDC",
                address!("0x2000000000000000000000000000000000000001"),
            ),
            // HFUN
            ("@1", address!("0x2000000000000000000000000000000000000002")),
            // USDT0
            (
                "@166",
                address!("0x200000000000000000000000000000000000010C"),
            ),
            // JEFF
            ("@4", address!("0x2000000000000000000000000000000000000005")),
            // HYPE
            (
                "@107",
                address!("0x2222222222222222222222222222222222222222"),
            ),
            // kHYPE
            (
                "@250",
                address!("0x2000000000000000000000000000000000000079"),
            ),
            // UBTC
            (
                "@142",
                address!("0x20000000000000000000000000000000000000c5"),
            ),
        ]);
        let spot = hypercore::spot_markets(mainnet_url(), reqwest::Client::new())
            .await
            .unwrap();
        for (key, value) in expected_addresses {
            let market = spot.iter().find(|market| market.name == key).unwrap();
            let address = market.tokens[0].cross_chain_address.unwrap();
            assert_eq!(address, value, "unexpected {address} <> {value}");
        }
    }
}
