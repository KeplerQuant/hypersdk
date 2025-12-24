use std::{collections::HashMap, fmt, time::Duration};

use alloy::{
    dyn_abi::TypedData,
    primitives::Address,
    signers::{Signer, SignerSync},
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::header;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use url::Url;

use super::types::HyperliquidChain;
use crate::hypercore::{
    ARBITRUM_SIGNATURE_CHAIN_ID, Cloid, OidOrCloid, PerpMarket, SpotMarket, SpotToken,
    types::{
        Action, ActionRequest, ApiResponse, BasicOrder, BatchCancel, BatchCancelCloid, BatchModify,
        BatchOrder, CORE_MAINNET_EIP712_DOMAIN, Fill, OkResponse, OrderResponseStatus, OrderUpdate,
        ScheduleCancel, SendAsset, SendToken, Signature, SpotSend, UsdSend, UserBalance, solidity,
    },
};

/// Error for a set of order ids.
#[derive(Debug)]
pub struct ActionError<T> {
    pub ids: Vec<T>,
    pub err: String,
}

impl<T> fmt::Display for ActionError<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, ids: {:?}", self.err, self.ids)
    }
}

impl<T> std::error::Error for ActionError<T> where T: fmt::Display + fmt::Debug {}

/// HTTP client.
pub struct Client {
    http_client: reqwest::Client,
    base_url: Url,
}

impl Client {
    /// Creates a new http client.
    pub fn new(base_url: Url) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .tcp_nodelay(true)
            .build()
            .unwrap();

        Self {
            http_client,
            base_url,
        }
    }

    /// Creates a new WebSocket connection from the hypercore client.
    pub fn websocket(&self) -> super::WebSocket {
        let mut url = self.base_url.clone();
        let _ = url.set_scheme("wss");
        url.set_path("/ws");
        super::WebSocket::new(url)
    }

    /// Creates a new WebSocket connection from the hypercore client disabling TLS.
    pub fn websocket_no_tls(&self) -> super::WebSocket {
        let mut url = self.base_url.clone();
        let _ = url.set_scheme("ws");
        url.set_path("/ws");
        super::WebSocket::new(url)
    }

    /// Query the perpetual future contracts.
    #[inline(always)]
    pub async fn perps(&self) -> Result<Vec<PerpMarket>> {
        super::perp_markets(self.base_url.clone(), self.http_client.clone()).await
    }

    /// Query the spot instruments.
    #[inline(always)]
    pub async fn spot(&self) -> Result<Vec<SpotMarket>> {
        super::spot_markets(self.base_url.clone(), self.http_client.clone()).await
    }

    /// Query the spot tokens.
    #[inline(always)]
    pub async fn spot_tokens(&self) -> Result<Vec<SpotToken>> {
        super::spot_tokens(self.base_url.clone(), self.http_client.clone()).await
    }

    /// Returns the user's open orders.
    pub async fn open_orders(&self, user: Address) -> Result<Vec<BasicOrder>> {
        let mut api_url = self.base_url.clone();
        api_url.set_path("/info");

        let data = self
            .http_client
            .post(api_url)
            .json(&InfoRequest::FrontendOpenOrders { user })
            .send()
            .await?
            .json()
            .await?;

        Ok(data)
    }

    /// Returns all perp prices
    pub async fn all_mids(&self) -> Result<HashMap<String, Decimal>> {
        let mut api_url = self.base_url.clone();
        api_url.set_path("/info");

        let data = self
            .http_client
            .post(api_url)
            .json(&InfoRequest::AllMids)
            .send()
            .await?
            .json()
            .await?;

        Ok(data)
    }

    /// Returns the user's historical orders.
    pub async fn historical_orders(&self, user: Address) -> Result<Vec<BasicOrder>> {
        let mut api_url = self.base_url.clone();
        api_url.set_path("/info");

        let data = self
            .http_client
            .post(api_url)
            .json(&InfoRequest::HistoricalOrders { user })
            .send()
            .await?
            .json()
            .await?;

        Ok(data)
    }

    /// Returns the user's fills.
    pub async fn user_fills(&self, user: Address) -> Result<Vec<Fill>> {
        let mut api_url = self.base_url.clone();
        api_url.set_path("/info");

        let data = self
            .http_client
            .post(api_url)
            .json(&InfoRequest::UserFills { user })
            .send()
            .await?
            .json()
            .await?;

        Ok(data)
    }

    /// Returns the status of an order.
    pub async fn order_status(
        &self,
        user: Address,
        oid: OidOrCloid,
    ) -> Result<Option<OrderUpdate>> {
        let mut api_url = self.base_url.clone();
        api_url.set_path("/info");

        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        #[serde(tag = "status")]
        enum Response {
            Order { order: OrderUpdate },
            UnknownOid,
        }

        let data: Response = self
            .http_client
            .post(api_url)
            .json(&InfoRequest::OrderStatus { user, oid })
            .send()
            .await?
            .json()
            .await?;

        Ok(match data {
            Response::Order { order } => Some(order),
            Response::UnknownOid => None,
        })
    }

    /// Retrieve user spot balances.
    pub async fn user_balances(&self, user: Address) -> Result<Vec<UserBalance>> {
        let mut api_url = self.base_url.clone();
        api_url.set_path("/info");

        #[derive(Deserialize)]
        struct Balances {
            balances: Vec<UserBalance>,
        }

        let data: Balances = self
            .http_client
            .post(api_url)
            .json(&InfoRequest::SpotClearinghouseState { user })
            .send()
            .await?
            .json()
            .await?;

        Ok(data.balances)
    }

    /// Schedule cancellation.
    pub async fn schedule_cancel<S: SignerSync>(
        &self,
        signer: &S,
        nonce: u64,
        when: DateTime<Utc>,
        vault_address: Option<Address>,
        expires_after: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let resp = self
            .send_sign_rmp(
                signer,
                Action::ScheduleCancel(ScheduleCancel {
                    time: Some(when.timestamp_millis() as u64),
                }),
                nonce,
                vault_address,
                expires_after,
            )
            .await?;

        match resp {
            ApiResponse::Ok(OkResponse::Default) => Ok(()),
            ApiResponse::Err(err) => {
                anyhow::bail!("schedule_cancel: {err}")
            }
            _ => panic!("unexpected response: {resp:?}"),
        }
    }

    /// Place a batch of orders.
    pub fn place<S: SignerSync>(
        &self,
        signer: &S,
        batch: BatchOrder,
        nonce: u64,
        vault_address: Option<Address>,
        expires_after: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<Cloid>>> + Send + 'static
    {
        let cloids: Vec<_> = batch.orders.iter().map(|req| req.cloid).collect();

        let future = self.send_sign_rmp(
            signer,
            Action::Order(batch),
            nonce,
            vault_address,
            expires_after,
        );

        async move {
            let resp = future.await.map_err(|err| ActionError {
                ids: cloids.clone(),
                err: err.to_string(),
            })?;

            match resp {
                ApiResponse::Ok(OkResponse::Order { statuses }) => Ok(statuses),
                ApiResponse::Err(err) => Err(ActionError { ids: cloids, err }),
                _ => panic!("unexpected response: {resp:?}"),
            }
        }
    }

    /// Cancel a batch of orders.
    pub fn cancel<S: SignerSync>(
        &self,
        signer: &S,
        batch: BatchCancel,
        nonce: u64,
        vault_address: Option<Address>,
        expires_after: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<u64>>> + Send + 'static
    {
        let oids: Vec<_> = batch.cancels.iter().map(|req| req.oid).collect();

        let future = self.send_sign_rmp(
            signer,
            Action::Cancel(batch),
            nonce,
            vault_address,
            expires_after,
        );

        async move {
            let resp = future.await.map_err(|err| ActionError {
                ids: oids.clone(),
                err: err.to_string(),
            })?;

            match resp {
                ApiResponse::Ok(OkResponse::Order { statuses }) => Ok(statuses),
                ApiResponse::Err(err) => Err(ActionError { ids: oids, err }),
                _ => panic!("unexpected response: {resp:?}"),
            }
        }
    }

    /// Cancel a batch of orders by cloid.
    pub fn cancel_by_cloid<S: SignerSync>(
        &self,
        signer: &S,
        batch: BatchCancelCloid,
        nonce: u64,
        vault_address: Option<Address>,
        expires_after: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<Cloid>>> + Send + 'static
    {
        let cloids: Vec<_> = batch.cancels.iter().map(|req| req.cloid).collect();

        let future = self.send_sign_rmp(
            signer,
            Action::CancelByCloid(batch),
            nonce,
            vault_address,
            expires_after,
        );

        async move {
            let resp = future.await.map_err(|err| ActionError {
                ids: cloids.clone(),
                err: err.to_string(),
            })?;

            match resp {
                ApiResponse::Ok(OkResponse::Order { statuses }) => Ok(statuses),
                ApiResponse::Err(err) => Err(ActionError { ids: cloids, err }),
                _ => panic!("unexpected response: {resp:?}"),
            }
        }
    }

    /// Modify a batch of orders.
    pub fn modify<S: SignerSync>(
        &self,
        signer: &S,
        batch: BatchModify,
        nonce: u64,
        vault_address: Option<Address>,
        expires_after: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<Vec<OrderResponseStatus>, ActionError<OidOrCloid>>> + Send + 'static
    {
        let cloids: Vec<_> = batch.modifies.iter().map(|req| req.oid).collect();

        let future = self.send_sign_rmp(
            signer,
            Action::BatchModify(batch),
            nonce,
            vault_address,
            expires_after,
        );

        async move {
            let resp = future.await.map_err(|err| ActionError {
                ids: cloids.clone(),
                err: err.to_string(),
            })?;

            match resp {
                ApiResponse::Ok(OkResponse::Order { statuses }) => Ok(statuses),
                ApiResponse::Err(err) => Err(ActionError { ids: cloids, err }),
                _ => panic!("unexpected response: {resp:?}"),
            }
        }
    }

    /// Helper function to transfer from spot core to EVM.
    pub async fn transfer_to_evm<S: Send + SignerSync>(
        &self,
        signer: &S,
        token: SpotToken,
        amount: Decimal,
        nonce: u64,
    ) -> Result<()> {
        let destination = token
            .cross_chain_address
            .ok_or_else(|| anyhow::anyhow!("token {token} doesn't have a cross chain address"))?;

        self.spot_send(
            &signer,
            SpotSend {
                hyperliquid_chain: HyperliquidChain::Mainnet,
                signature_chain_id: ARBITRUM_SIGNATURE_CHAIN_ID,
                destination,
                token: SendToken(token),
                amount,
                time: nonce,
            },
            nonce,
        )
        .await
    }

    /// Helper function to transfer from perps to spot.
    ///
    /// Only USDC is accepted as `token`.
    pub async fn transfer_to_spot<S: Signer + SignerSync>(
        &self,
        signer: &S,
        token: SpotToken,
        amount: Decimal,
        nonce: u64,
    ) -> Result<()> {
        if token.name != "USDC" {
            return Err(anyhow::anyhow!(
                "only USDC is accepted, tried to transfer {}",
                token.name
            ));
        }

        self.send_asset(
            signer,
            SendAsset {
                hyperliquid_chain: HyperliquidChain::Mainnet,
                signature_chain_id: ARBITRUM_SIGNATURE_CHAIN_ID,
                destination: signer.address(),
                source_dex: "".into(),
                destination_dex: "spot".into(),
                token: SendToken(token),
                from_sub_account: "".into(),
                amount,
                nonce,
            },
            nonce,
        )
        .await
    }

    /// Helper function to transfer from spot to perps.
    ///
    /// Only USDC is accepted as `token`.
    pub async fn transfer_to_perps<S: Signer + SignerSync>(
        &self,
        signer: &S,
        token: SpotToken,
        amount: Decimal,
        nonce: u64,
    ) -> Result<()> {
        if token.name != "USDC" {
            return Err(anyhow::anyhow!(
                "only USDC is accepted, tried to transfer {}",
                token.name
            ));
        }

        self.send_asset(
            signer,
            SendAsset {
                hyperliquid_chain: HyperliquidChain::Mainnet,
                signature_chain_id: ARBITRUM_SIGNATURE_CHAIN_ID,
                destination: signer.address(),
                source_dex: "spot".into(),
                destination_dex: "".into(),
                token: SendToken(token),
                from_sub_account: "".into(),
                amount,
                nonce,
            },
            nonce,
        )
        .await
    }

    /// Send USDC to another address.
    ///
    /// Perp <> Perp transfers.
    ///
    /// <https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint#core-usdc-transfer>
    pub async fn send_usdc<S: SignerSync>(
        &self,
        signer: &S,
        send: UsdSend,
        nonce: u64,
    ) -> Result<()> {
        let typed_data = send.typed_data(&send);
        let resp = self
            .send_sign_eip712(signer, Action::UsdSend(send), typed_data, nonce)
            .await?;
        match resp {
            ApiResponse::Ok(OkResponse::Default) => Ok(()),
            ApiResponse::Err(err) => {
                anyhow::bail!("send_usdc: {err}")
            }
            _ => panic!("unexpected response: {resp:?}"),
        }
    }

    /// Send USDC to another address.
    ///
    /// Spot <> DEX or Subaccount.
    ///
    /// <https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint#send-asset>
    pub fn send_asset<S: SignerSync>(
        &self,
        signer: &S,
        send: SendAsset,
        nonce: u64,
    ) -> impl Future<Output = Result<()>> + Send + 'static {
        let typed_data = send.typed_data(&send);
        let future = self.send_sign_eip712(signer, Action::SendAsset(send), typed_data, nonce);
        async move {
            let resp = future.await?;
            match resp {
                ApiResponse::Ok(OkResponse::Default) => Ok(()),
                ApiResponse::Err(err) => {
                    anyhow::bail!("send_asset: {err}")
                }
                _ => panic!("unexpected response: {resp:?}"),
            }
        }
    }

    /// Spot transfer.
    ///
    /// Spot <> Spot.
    ///
    /// <https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/exchange-endpoint#core-spot-transfer>
    pub async fn spot_send<S: SignerSync>(
        &self,
        signer: &S,
        send: SpotSend,
        nonce: u64,
    ) -> Result<()> {
        let typed_data = send.typed_data(&send);
        let resp = self
            .send_sign_eip712(signer, Action::SpotSend(send), typed_data, nonce)
            .await?;
        match resp {
            ApiResponse::Ok(OkResponse::Default) => Ok(()),
            ApiResponse::Err(err) => {
                anyhow::bail!("spot send: {err}")
            }
            _ => panic!("unexpected response: {resp:?}"),
        }
    }

    /// Toggle big blocks or not idk.
    pub async fn evm_user_modify<S: SignerSync>(
        &self,
        signer: &S,
        toggle: bool,
        nonce: u64,
        vault_address: Option<Address>,
        expires_after: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let resp = self
            .send_sign_rmp(
                signer,
                Action::EvmUserModify {
                    using_big_blocks: toggle,
                },
                nonce,
                vault_address,
                expires_after,
            )
            .await?;

        match resp {
            ApiResponse::Ok(OkResponse::Default) => Ok(()),
            ApiResponse::Err(err) => {
                anyhow::bail!("evm_user_modify: {err}")
            }
            _ => panic!("unexpected response: {resp:?}"),
        }
    }

    /// Invalidate a nonce.
    pub async fn noop<S: SignerSync>(
        &self,
        signer: &S,
        nonce: u64,
        vault_address: Option<Address>,
        expires_after: Option<DateTime<Utc>>,
    ) -> Result<()> {
        let resp = self
            .send_sign_rmp(signer, Action::Noop, nonce, vault_address, expires_after)
            .await?;

        match resp {
            ApiResponse::Ok(OkResponse::Default) => Ok(()),
            ApiResponse::Err(err) => {
                anyhow::bail!("noop: {err}")
            }
            _ => panic!("unexpected response: {resp:?}"),
        }
    }

    /// Send a signed action hashing with typed data.
    fn send_sign_eip712<S: SignerSync>(
        &self,
        signer: &S,
        action: Action,
        typed_data: TypedData,
        nonce: u64,
    ) -> impl Future<Output = Result<ApiResponse>> + Send + 'static {
        let res = sign_eip712(signer, action, typed_data, nonce);

        let http_client = self.http_client.clone();
        let mut url = self.base_url.clone();
        url.set_path("/exchange");

        async move {
            let req = res?;
            let text = serde_json::to_string(&req).expect("text");

            let res = http_client
                .post(url)
                .timeout(Duration::from_secs(5))
                .header(header::CONTENT_TYPE, "application/json")
                .body(text)
                .send()
                .await?
                .json()
                .await?;
            Ok(res)
        }
    }

    /// Send a signed action hashing with rmp.
    fn send_sign_rmp<S: SignerSync>(
        &self,
        signer: &S,
        action: Action,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
    ) -> impl Future<Output = Result<ApiResponse>> + Send + 'static {
        let res = sign_rmp(
            signer,
            action,
            nonce,
            maybe_vault_address,
            maybe_expires_after,
        );

        let http_client = self.http_client.clone();
        let mut url = self.base_url.clone();
        url.set_path("/exchange");

        async move {
            let req = res?;
            let text = serde_json::to_string(&req).expect("serde");
            let res = http_client
                .post(url)
                .timeout(Duration::from_secs(5))
                .header(header::CONTENT_TYPE, "application/json")
                .body(text)
                .send()
                .await?
                .json()
                .await?;
            Ok(res)
        }
    }

    // TODO: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/info-endpoint#retrieve-a-users-subaccounts
}

/// Send a signed action hashing with typed data.
fn sign_eip712<S: SignerSync>(
    signer: &S,
    action: Action,
    typed_data: TypedData,
    nonce: u64,
) -> Result<ActionRequest> {
    let signature = signer.sign_dynamic_typed_data_sync(&typed_data)?;
    let sig: Signature = signature.into();

    Ok(ActionRequest {
        signature: sig,
        action,
        nonce,
        vault_address: None,
        expires_after: None,
    })
}

fn sign_rmp<S: SignerSync>(
    signer: &S,
    action: Action,
    nonce: u64,
    maybe_vault_address: Option<Address>,
    maybe_expires_after: Option<DateTime<Utc>>,
) -> Result<ActionRequest> {
    let expires_after = maybe_expires_after.map(|after| after.timestamp_millis() as u64);
    let connection_id = action.hash(nonce, maybe_vault_address, expires_after)?;

    let sig = signer.sign_typed_data_sync(
        &solidity::Agent {
            source: "a".to_string(),
            connectionId: connection_id,
        },
        &CORE_MAINNET_EIP712_DOMAIN,
    )?;

    Ok(ActionRequest {
        signature: sig.into(),
        action,
        nonce,
        vault_address: maybe_vault_address,
        expires_after,
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "type")]
enum InfoRequest {
    FrontendOpenOrders {
        user: Address,
    },
    HistoricalOrders {
        user: Address,
    },
    UserFills {
        user: Address,
    },
    OrderStatus {
        user: Address,
        #[serde(with = "either::serde_untagged")]
        oid: OidOrCloid,
    },
    SpotClearinghouseState {
        user: Address,
    },
    AllMids,
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::*;
    use crate::hypercore::{
        ARBITRUM_SIGNATURE_CHAIN_ID,
        types::{self, HyperliquidChain},
    };

    // #[tokio::test]
    // async fn test_order_state() {
    //     let client = Client::new(MAINNET_URL.clone());
    // }

    fn get_signer() -> super::super::PrivateKeySigner {
        let priv_key = "e908f86dbb4d55ac876378565aafeabc187f6690f046459397b17d9b9a19688e";
        priv_key.parse::<super::super::PrivateKeySigner>().unwrap()
    }

    #[test]
    fn test_sign_usd_transfer_action() {
        let signer = get_signer();

        let usd_send = types::UsdSend {
            signature_chain_id: ARBITRUM_SIGNATURE_CHAIN_ID,
            hyperliquid_chain: HyperliquidChain::Mainnet,
            destination: "0x0D1d9635D0640821d15e323ac8AdADfA9c111414"
                .parse()
                .unwrap(),
            amount: Decimal::ONE,
            time: 1690393044548,
        };
        let typed_data = usd_send.typed_data(&usd_send);
        let signature = signer.sign_dynamic_typed_data_sync(&typed_data).unwrap();

        let expected_sig = "0xeca6267bcaadc4c0ae1aed73f5a2c45fcdbb7271f2e9356992404e5d4bad75a3572e08fe93f17755abadb7f84be7d1e9c4ce48bb5633e339bc430c672d5a20ed1b";
        assert_eq!(signature.to_string(), expected_sig);
    }
}
