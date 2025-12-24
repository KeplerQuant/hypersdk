use std::fmt;

use alloy::{
    dyn_abi::{Eip712Domain, Eip712Types, Resolver, TypedData},
    primitives::{Address, B128, B256, U256, keccak256},
    signers::k256::ecdsa::RecoveryId,
    sol_types::{SolStruct, eip712_domain},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};

use crate::hypercore::{Cloid, OidOrCloid, SpotToken};

const HYPERLIQUID_EIP_PREFIX: &str = "HyperliquidTransaction:";

/// Domain for Core mainnet EIP‑712 signing.
/// This domain is used when creating signatures for transactions on the mainnet.
pub(super) const CORE_MAINNET_EIP712_DOMAIN: Eip712Domain = eip712_domain! {
    name: "Exchange",
    version: "1",
    chain_id: 1337,
    verifying_contract: Address::ZERO,
};

/// Domain for Arbitrum mainnet EIP‑712 signing.
/// This domain is used when creating signatures for transactions on Arbitrum.
pub(super) const ARBITRUM_MAINNET_EIP712_DOMAIN: Eip712Domain = eip712_domain! {
    name: "HyperliquidSignTransaction",
    version: "1",
    chain_id: 42161,
    verifying_contract: Address::ZERO,
};

/// Side for a trade or an order.
///
/// `Bid` represents a buy order, `Ask` represents a sell order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, derive_more::Display,
)]
pub enum Side {
    #[serde(rename = "B")]
    Bid,
    #[serde(rename = "A")]
    Ask,
}

/// WebSocket outgoing message.
///
/// This enum represents messages sent from the client to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method")]
#[serde(rename_all = "camelCase")]
pub enum Outgoing {
    Subscribe { subscription: Subscription },
    Unsubscribe { subscription: Subscription },
    Ping,
    Pong,
}

/// Subscription message.
///
/// Each variant corresponds to a subscription type that can be requested.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, derive_more::Display)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Subscription {
    #[display("bbo({coin})")]
    Bbo { coin: String },
    #[display("trades({coin})")]
    Trades { coin: String },
    #[display("l2Book({coin})")]
    L2Book { coin: String },
    // L2BookDeltas { coin: String },
    #[display("orderUpdates({user})")]
    OrderUpdates { user: Address },
    #[display("userFills({user})")]
    UserFills { user: Address },
}

/// Hyperliquid websocket message.
///
/// This enum represents all message types received from the server.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "channel", content = "data")]
pub enum Incoming {
    // we reply with the incoming msg
    SubscriptionResponse(Outgoing),
    Bbo(Bbo),
    L2Book(L2Book),
    // L2BookDeltas(L2Book),
    Trades(Vec<Trade>),
    OrderUpdates(Vec<OrderUpdate>),
    UserFills { user: Address, fills: Vec<Fill> },
    Ping,
    Pong,
}

/// WebSocket order update.
///
/// Contains status, timestamp, and the original order details.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderUpdate {
    pub status: OrderStatus,
    pub status_timestamp: u64,
    pub order: BasicOrder,
}

/// Best bid offer.
///
/// Provides the best bid and ask for a coin at a specific time.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Bbo {
    pub coin: String,
    pub time: u64,
    pub bbo: (Option<BookLevel>, Option<BookLevel>),
}

/// WebSocket book level.
///
/// Represents a single price level on the order book.
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BookLevel {
    pub px: Decimal,
    pub sz: Decimal,
    pub n: usize,
}

/// WebSocket trade.
///
/// Describes a single trade that occurred on the exchange.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub coin: String,
    pub side: Side,
    pub px: Decimal,
    pub sz: Decimal,
    pub time: u64,
    pub hash: String,
    pub tid: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidation: Option<Liquidation>,
}

/// WebSocket L2Book.
///
/// Contains the order book snapshot or deltas for a coin.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct L2Book {
    pub coin: String,
    pub time: u64,
    #[serde(default)]
    pub snapshot: Option<bool>,
    // bids & asks
    pub levels: [Vec<BookLevel>; 2],
}

/// WebSocket fill.
///
/// Describes a filled order for a user.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fill {
    pub coin: String,
    pub px: Decimal,
    pub sz: Decimal,
    pub side: Side,
    pub time: u64,
    pub start_position: Decimal,
    pub dir: String,
    pub closed_pnl: Decimal,
    pub hash: String,
    pub oid: u64,
    pub crossed: bool,
    pub fee: Decimal,
    pub tid: u64,
    pub cloid: Option<B128>,
    pub fee_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub liquidation: Option<Liquidation>,
}

/// Order details.
///
/// Basic information needed for creating or updating an order.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BasicOrder {
    pub timestamp: u64,
    pub coin: String,
    pub side: Side,
    pub limit_px: Decimal,
    pub sz: Decimal,
    pub oid: u64,
    pub orig_sz: Decimal,
    pub cloid: Option<B128>,
    pub order_type: OrderType,
    pub tif: Option<TimeInForce>,
    pub reduce_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Liquidation {
    pub liquidated_user: String,
    pub mark_px: Decimal,
    pub method: String,
}

/// Order type.
///
/// Determines the behaviour of the order (limit, market, or trigger).
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub enum OrderType {
    Limit,
    Market,
    Trigger,
}

/// Time‑in‑force.
///
/// Specifies how long an order remains active.
#[derive(Debug, Deserialize, Clone, Copy, Serialize)]
#[serde(rename = "PascalCase")]
pub enum TimeInForce {
    Alo,
    Ioc,
    Gtc,
    FrontendMarket,
}

/// Order status.
///
/// Represents the lifecycle state of an order.
#[derive(Debug, Copy, Clone, Hash, Deserialize, Serialize, derive_more::Display)]
#[serde(rename_all = "camelCase")]
pub enum OrderStatus {
    Open,
    Filled,
    Canceled,
    Triggered,
    Rejected,
    MarginCanceled,
    VaultWithdrawalCanceled,
    OpenInterestCapCanceled,
    SelfTradeCanceled,
    ReduceOnlyCanceled,
    SiblingFilledCanceled,
    DelistedCanceled,
    LiquidatedCanceled,
    ScheduledCancel,
    TickRejected,
    MinTradeNtlRejected,
    PerpMarginRejected,
    ReduceOnlyRejected,
    BadAloPxRejected,
    IocCancelRejected,
    BadTriggerPxRejected,
    MarketOrderNoLiquidityRejected,
    PositionIncreaseAtOpenInterestCapRejected,
    PositionFlipAtOpenInterestCapRejected,
    TooAggressiveAtOpenInterestCapRejected,
    OpenInterestIncreaseRejected,
    InsufficientSpotBalanceRejected,
    OracleRejected,
    PerpMaxPositionRejected,
}

impl OrderStatus {
    /// Returns whether the order is finished.
    pub fn is_finished(&self) -> bool {
        !matches!(self, OrderStatus::Open)
    }
}

/// Request for an action.
///
/// Contains the action, a nonce, signature, optional vault address, and optional expiry.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActionRequest {
    /// Action.
    pub action: Action,
    /// Nonce of the message.
    pub nonce: u64,
    /// Signature
    pub signature: Signature,
    /// Trading on behalf of
    pub vault_address: Option<Address>,
    /// Timestamp in milliseconds
    pub expires_after: Option<u64>,
}

/// API response wrapper.
///
/// The `Ok` variant contains a successful response, while `Err` holds an error message.
#[derive(Debug, Deserialize)]
#[serde(tag = "status", content = "response")]
#[serde(rename_all = "camelCase")]
pub(super) enum ApiResponse {
    Ok(OkResponse),
    Err(String),
}

/// Successful API response data.
///
/// Currently supports order responses and a default placeholder.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "data")]
#[serde(rename_all = "camelCase")]
pub(super) enum OkResponse {
    Order { statuses: Vec<OrderResponseStatus> },
    // should be ok?
    Default,
}

/// Response to an order insertion.
///
/// Contains either a success indicator, the resting order, a filled order, or an error message.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderResponseStatus {
    Success,
    Resting {
        oid: u64,
        cloid: Option<B128>,
    },
    Filled {
        #[serde(rename = "totalSz")]
        total_sz: Decimal,
        #[serde(rename = "avgPx")]
        avg_px: Decimal,
        oid: u64,
    },
    Error(String),
}

/// Signature.
///
/// Represents an EIP‑712 signature split into its components.
#[derive(Debug, Serialize)]
pub(super) struct Signature {
    pub r: U256,
    pub s: U256,
    pub v: u64,
}

impl From<Signature> for alloy::signers::Signature {
    fn from(sig: Signature) -> Self {
        let recid = RecoveryId::from_byte((sig.v - 27) as u8).expect("recid");
        Self::new(sig.r, sig.s, recid.is_y_odd())
    }
}

impl From<alloy::signers::Signature> for Signature {
    fn from(signature: alloy::signers::Signature) -> Self {
        let recid = signature.recid().to_byte() as u64 + 27;
        Self {
            r: signature.r(),
            s: signature.s(),
            v: recid,
        }
    }
}

/// Batch order.
///
/// A collection of orders sent together, optionally grouped.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BatchOrder {
    pub orders: Vec<OrderRequest>,
    pub grouping: OrderGrouping,
}

/// Order grouping strategy.
///
/// Determines how orders are grouped when sent in a batch.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub enum OrderGrouping {
    Na,
    NormalTpsl,
    PositionTpsl,
}

/// Order request.
///
/// Represents a single order within a batch.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct OrderRequest {
    #[serde(rename = "a")]
    pub asset: usize,
    #[serde(rename = "b")]
    pub is_buy: bool,
    #[serde(rename = "p", with = "rust_decimal::serde::str")]
    pub limit_px: Decimal,
    #[serde(rename = "s", with = "rust_decimal::serde::str")]
    pub sz: Decimal,
    #[serde(rename = "r")]
    pub reduce_only: bool,
    #[serde(rename = "t")]
    pub order_type: OrderTypePlacement,
    #[serde(rename = "c")]
    #[serde(with = "const_hex")]
    pub cloid: Cloid,
}

/// Order type for the placement.
///
/// Specifies whether the order is limit or trigger and its associated parameters.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum OrderTypePlacement {
    Limit {
        tif: TimeInForce,
    },
    Trigger {
        #[serde(with = "rust_decimal::serde::str")]
        trigger_px: Decimal,
        is_market: bool,
        tpsl: TpSl,
    },
}

/// Trigger type.
///
/// Indicates whether the trigger is a take‑profit (`Tp`) or stop‑loss (`Sl`).
#[derive(PartialEq, Eq, Deserialize, Serialize, Copy, Clone, Debug)]
#[serde(rename = "lowercase")]
pub enum TpSl {
    Tp,
    Sl,
}

/// Batch modify request.
///
/// Contains a list of order modifications to be applied.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BatchModify {
    pub modifies: Vec<Modify>,
}

/// Individual order modification.
///
/// References the order by ID and includes the new order data.
#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Modify {
    pub oid: OidOrCloid,
    pub order: OrderRequest,
}

/// Batch cancel request.
///
/// Contains a list of order IDs to cancel.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BatchCancel {
    pub cancels: Vec<Cancel>,
}

/// Batch cancel by cloid request.
///
/// Contains a list of cloid values to cancel.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BatchCancelCloid {
    pub cancels: Vec<CancelByCloid>,
}

/// Cancel request.
///
/// References an order by asset and ID.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Cancel {
    #[serde(rename = "a")]
    pub asset: u32,
    #[serde(rename = "o")]
    pub oid: u64,
}

/// Cancel request by cloid.
///
/// References an order by asset and cloid.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CancelByCloid {
    pub asset: u32,
    #[serde(with = "const_hex")]
    pub cloid: B128,
}

/// Schedule cancellation of all orders.
///
/// The optional `time` field can be used to delay the cancellation.
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleCancel {
    pub time: Option<u64>,
}

/// User balance
///
/// References an order by asset and cloid.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserBalance {
    /// Coin's name.
    pub coin: String,
    /// The token index.
    pub token: usize,
    /// Amount held
    pub hold: Decimal,
    /// Total amount
    pub total: Decimal,
    /// Entry notional
    pub entry_ntl: Decimal,
}

/// Abstraction over a token to be sent out.
///
/// This is to prevent users from f*cking it up.
#[derive(Debug, Clone)]
pub struct SendToken(pub SpotToken);

impl fmt::Display for SendToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{:x}", self.0.name, self.0.token_id)
    }
}

/// Send USDC from the perp balance.
///
/// <https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint#core-usdc-transfer>
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UsdSend {
    /// The chain this action is being executed on.
    pub hyperliquid_chain: HyperliquidChain,
    /// Signature chain ID.
    ///
    /// For arbitrum use [`super::ARBITRUM_SIGNATURE_CHAIN_ID`].
    pub signature_chain_id: &'static str,
    /// The destination address.
    pub destination: Address,
    /// The amount.
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    /// Current time, should match the nonce
    pub time: u64,
}

impl UsdSend {
    #[inline(always)]
    pub(super) fn typed_data(&self, msg: &impl Serialize) -> TypedData {
        get_typed_data::<solidity::UsdSend>(msg)
    }
}

/// Send spot tokens.
///
/// <https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint#core-spot-transfer>
#[serde_as]
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpotSend {
    /// The chain this action is being executed on.
    pub hyperliquid_chain: HyperliquidChain,
    /// Signature chain ID.
    ///
    /// For arbitrum use [`super::ARBITRUM_SIGNATURE_CHAIN_ID`].
    pub signature_chain_id: &'static str,
    /// The destination address.
    pub destination: Address,
    /// Token
    #[serde_as(as = "DisplayFromStr")]
    pub token: SendToken,
    /// The amount.
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    /// Current time, should match the nonce
    pub time: u64,
}

impl SpotSend {
    #[inline(always)]
    pub(super) fn typed_data(&self, msg: &impl Serialize) -> TypedData {
        get_typed_data::<solidity::SpotSend>(msg)
    }
}

/// Send asset.
///
/// <https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint#send-asset>
#[serde_as]
#[derive(Serialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SendAsset {
    /// The chain this action is being executed on.
    pub hyperliquid_chain: HyperliquidChain,
    /// Signature chain ID.
    ///
    /// For arbitrum use [`super::ARBITRUM_SIGNATURE_CHAIN_ID`].
    pub signature_chain_id: &'static str,
    /// The destination address.
    pub destination: Address,
    /// Source DEX, can be empty
    pub source_dex: String,
    /// Destiation DEX, can be empty
    pub destination_dex: String,
    /// Token
    #[serde_as(as = "DisplayFromStr")]
    pub token: SendToken,
    /// From subaccount, can be empty
    pub from_sub_account: String,
    /// The amount.
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    /// Request nonce
    pub nonce: u64,
}

impl SendAsset {
    #[inline(always)]
    pub(super) fn typed_data(&self, msg: &impl Serialize) -> TypedData {
        get_typed_data::<solidity::SendAsset>(msg)
    }
}

/// Chain for Hyperliquid transactions.
///
/// Indicates whether the transaction is on mainnet or testnet.
#[derive(Serialize, Debug, Copy, Clone, derive_more::Display)]
#[serde(rename_all = "PascalCase")]
pub enum HyperliquidChain {
    #[display("Mainnet")]
    Mainnet,
    #[display("Testnet")]
    Testnet,
}

/// An action that requires signing.
///
/// Represents a request to the exchange that must be signed by the user.
#[derive(Clone, Serialize, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "camelCase")]
pub(super) enum Action {
    /// Order insertion.
    Order(BatchOrder),
    /// Order modification.
    BatchModify(BatchModify),
    /// Order cancellation by oid.
    Cancel(BatchCancel),
    /// Order cancellation by cloid.
    CancelByCloid(BatchCancelCloid),
    /// Schedule cancellation of all orders.
    ScheduleCancel(ScheduleCancel),
    /// Core USDC transfer.
    UsdSend(UsdSend),
    /// Send asset.
    SendAsset(SendAsset),
    /// Spot send.
    SpotSend(SpotSend),
    /// EVM user modify.
    EvmUserModify { using_big_blocks: bool },
    /// Invalidate a request.
    Noop,
}

impl Action {
    /// Hash the action for signing.
    ///
    /// The hash is generated by serializing the action to MessagePack, appending the nonce,
    /// optional vault address, and optional expiry, then Keccak256 hashing.
    #[inline]
    pub fn hash(
        &self,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<u64>,
    ) -> Result<B256, rmp_serde::encode::Error> {
        let mut bytes = rmp_serde::to_vec_named(self)?;
        bytes.extend(nonce.to_be_bytes());

        if let Some(vault_address) = maybe_vault_address {
            bytes.push(1);
            bytes.extend(vault_address.as_slice());
        } else {
            bytes.push(0);
        }

        if let Some(expires_after) = maybe_expires_after {
            bytes.push(0);
            bytes.extend(expires_after.to_be_bytes());
        }

        let signature = keccak256(bytes);
        Ok(B256::from(signature))
    }
}

pub(super) mod solidity {
    use alloy::sol;

    sol! {
        struct Agent {
            string source;
            bytes32 connectionId;
        }

        struct UsdSend {
            string hyperliquidChain;
            string destination;
            string amount;
            uint64 time;
        }

        struct SpotSend {
            string hyperliquidChain;
            string destination;
            string token;
            string amount;
            uint64 time;
        }

        struct SendAsset {
            string hyperliquidChain;
            string destination;
            string sourceDex;
            string destinationDex;
            string token;
            string amount;
            string fromSubAccount;
            uint64 nonce;
        }
    }
}

/// Returns the EIP712 domain and EIP712 types of the `T` message.
///
/// The returned `TypedData` can be used to sign the message with an Ethereum signer.
fn get_typed_data<T: SolStruct>(msg: &impl Serialize) -> TypedData {
    let mut resolver = Resolver::from_struct::<T>();
    resolver
        .ingest_string(T::eip712_encode_type())
        .expect("agent");

    let mut types = Eip712Types::from(&resolver);
    let agent_type = types.remove(T::NAME).unwrap();
    types.insert(format!("{HYPERLIQUID_EIP_PREFIX}{}", T::NAME), agent_type);

    TypedData {
        domain: ARBITRUM_MAINNET_EIP712_DOMAIN,
        resolver: Resolver::from(types),
        primary_type: format!("{HYPERLIQUID_EIP_PREFIX}{}", T::NAME),
        message: serde_json::to_value(msg).unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_response() {
        let text = r#"{
           "status":"ok",
           "response":{
              "type":"order",
              "data":{
                 "statuses":[
                    {
                       "error":"Order must have minimum value of $10."
                    }
                 ]
              }
           }
        }"#;
        let res = serde_json::from_str::<ApiResponse>(text);
        assert!(res.is_ok());
    }

    #[test]
    fn test_api_order_response() {
        let text = r#"{
           "status":"ok",
           "response":{
              "type":"order",
              "data":{
                 "statuses":[
                    {
                       "resting":{
                          "oid":77738308
                       }
                    }
                 ]
              }
           }
        }"#;
        let res = serde_json::from_str::<ApiResponse>(text);
        assert!(res.is_ok());
    }
}
