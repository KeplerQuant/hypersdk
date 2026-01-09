//! Signing utilities for HyperCore actions.
//!
//! This module provides functions for signing various types of actions on Hyperliquid,
//! including regular actions, multisig actions, and EIP-712 typed data.

use alloy::{
    dyn_abi::TypedData,
    primitives::{Address, B256},
    signers::{Signer, SignerSync},
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::hypercore::{
    ARBITRUM_TESTNET_CHAIN_ID, Chain,
    raw::{
        Action, ActionRequest, ApproveAgent, MultiSigAction, MultiSigPayload, SendAssetAction,
        SpotSendAction, UsdSendAction,
    },
    types::{
        BatchCancel, BatchCancelCloid, BatchModify, BatchOrder, CORE_MAINNET_EIP712_DOMAIN,
        ScheduleCancel, Signature, solidity,
    },
    utils::{get_typed_data, rmp_hash},
};

/// Trait for signing actions that modify state on Hyperliquid.
///
/// This trait provides a unified interface for signing different types of actions on Hyperliquid.
/// Each action type (orders, transfers, cancellations, etc.) implements this trait with the
/// appropriate signing method based on whether it uses RMP (MessagePack) hashing or EIP-712
/// typed data.
///
/// # Signing Methods
///
/// Hyperliquid uses two signing approaches:
///
/// ## RMP (MessagePack) Signing
///
/// Used for trading actions (orders, modifications, cancellations):
/// 1. Serialize the action to MessagePack format
/// 2. Append nonce, vault address, and expiry to the bytes
/// 3. Hash the bytes with Keccak256
/// 4. Sign the hash using EIP-712 with an Agent wrapper struct
///
/// ## EIP-712 Typed Data Signing
///
/// Used for asset transfers (UsdSend, SpotSend, SendAsset):
/// 1. Create structured EIP-712 typed data from the action
/// 2. Sign the typed data directly using EIP-712
/// 3. More human-readable in wallet UIs
///
/// # Implementation Pattern
///
/// Actions implement this trait by calling either `sign_rmp()` or `sign_eip712()`:
///
/// ```rust,ignore
/// // RMP-based action (orders, cancels, etc.)
/// impl Signable for BatchOrder {
///     fn sign<S: SignerSync>(
///         self,
///         signer: &S,
///         nonce: u64,
///         maybe_vault_address: Option<Address>,
///         maybe_expires_after: Option<DateTime<Utc>>,
///         chain: Chain,
///     ) -> Result<ActionRequest> {
///         sign_rmp(signer, Action::Order(self), nonce, maybe_vault_address, maybe_expires_after, chain)
///     }
/// }
///
/// // EIP-712 typed data action (transfers)
/// impl Signable for UsdSend {
///     fn sign<S: SignerSync>(
///         self,
///         signer: &S,
///         nonce: u64,
///         _maybe_vault_address: Option<Address>,
///         _maybe_expires_after: Option<DateTime<Utc>>,
///         _chain: Chain,
///     ) -> Result<ActionRequest> {
///         let typed_data = self.typed_data().unwrap();
///         sign_eip712(signer, Action::UsdSend(self), typed_data, nonce)
///     }
/// }
/// ```
///
/// # Type Parameters
///
/// - `S: SignerSync`: The signer type that can sign messages synchronously
///
/// # Required Traits
///
/// - `Serialize`: Actions must be serializable (for RMP hashing or typed data creation)
pub(super) trait Signable: Serialize + Clone {
    /// Sign this action synchronously and create a signed action request.
    ///
    /// This method consumes the action, signs it using the appropriate method (RMP or EIP-712),
    /// and returns a complete `ActionRequest` that can be submitted to the exchange.
    ///
    /// # Parameters
    ///
    /// - `self`: The action to sign (consumed to avoid cloning)
    /// - `signer`: The signer that will sign the action synchronously
    /// - `nonce`: Unique transaction nonce (typically millisecond timestamp)
    /// - `maybe_vault_address`: Optional vault address if trading on behalf of a vault
    /// - `maybe_expires_after`: Optional expiration time for the request
    /// - `chain`: The chain (mainnet or testnet)
    ///
    /// # Returns
    ///
    /// Returns an `ActionRequest` containing:
    /// - The signed action
    /// - The signature (r, s, v)
    /// - The nonce
    /// - Optional vault address
    /// - Optional expiration timestamp
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Serialization fails (for RMP-based actions)
    /// - Signing fails (invalid signer or signature generation error)
    /// - Typed data creation fails (for EIP-712 actions)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use hypersdk::hypercore::types::BatchOrder;
    /// use hypersdk::hypercore::signing::Signable;
    /// use alloy::signers::local::PrivateKeySigner;
    ///
    /// let signer: PrivateKeySigner = "0x...".parse()?;
    /// let order = BatchOrder { /* ... */ };
    /// let nonce = chrono::Utc::now().timestamp_millis() as u64;
    ///
    /// // Sign the order synchronously
    /// let action_request = order.sign_sync(
    ///     &signer,
    ///     nonce,
    ///     None,  // No vault
    ///     None,  // No expiry
    ///     Chain::Mainnet,
    /// )?;
    ///
    /// // action_request can now be submitted to the exchange
    /// ```
    ///
    /// # Notes
    ///
    /// - Nonce must be unique for each transaction (typically use current timestamp in ms)
    /// - Vault address is only needed when trading on behalf of a vault
    /// - Expiration is optional but recommended for time-sensitive operations
    /// - The action is consumed (moved) to avoid unnecessary cloning
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> anyhow::Result<ActionRequest>;

    /// Sign this action asynchronously and create a signed action request.
    ///
    /// This method consumes the action, signs it using the appropriate method (RMP or EIP-712),
    /// and returns a complete `ActionRequest` that can be submitted to the exchange.
    ///
    /// # Parameters
    ///
    /// - `self`: The action to sign (consumed to avoid cloning)
    /// - `signer`: The async signer that will sign the action
    /// - `nonce`: Unique transaction nonce (typically millisecond timestamp)
    /// - `maybe_vault_address`: Optional vault address if trading on behalf of a vault
    /// - `maybe_expires_after`: Optional expiration time for the request
    /// - `chain`: The chain (mainnet or testnet)
    ///
    /// # Returns
    ///
    /// Returns an `ActionRequest` containing:
    /// - The signed action
    /// - The signature (r, s, v)
    /// - The nonce
    /// - Optional vault address
    /// - Optional expiration timestamp
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Serialization fails (for RMP-based actions)
    /// - Signing fails (invalid signer or signature generation error)
    /// - Typed data creation fails (for EIP-712 actions)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use hypersdk::hypercore::types::BatchOrder;
    /// use hypersdk::hypercore::signing::Signable;
    /// use alloy::signers::local::PrivateKeySigner;
    ///
    /// let signer: PrivateKeySigner = "0x...".parse()?;
    /// let order = BatchOrder { /* ... */ };
    /// let nonce = chrono::Utc::now().timestamp_millis() as u64;
    ///
    /// // Sign the order asynchronously
    /// let action_request = order.sign(
    ///     &signer,
    ///     nonce,
    ///     None,  // No vault
    ///     None,  // No expiry
    ///     Chain::Mainnet,
    /// ).await?;
    ///
    /// // action_request can now be submitted to the exchange
    /// ```
    ///
    /// # Notes
    ///
    /// - Nonce must be unique for each transaction (typically use current timestamp in ms)
    /// - Vault address is only needed when trading on behalf of a vault
    /// - Expiration is optional but recommended for time-sensitive operations
    /// - The action is consumed (moved) to avoid unnecessary cloning
    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> anyhow::Result<ActionRequest>;
}

// RMP-based actions (orders, cancels, modifications)
// These use MessagePack serialization + keccak256 hash + EIP-712 Agent wrapper
impl Signable for BatchOrder {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp_sync(
            signer,
            Action::Order(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp(
            signer,
            Action::Order(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
        .await
    }
}

impl Signable for BatchModify {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp_sync(
            signer,
            Action::BatchModify(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp(
            signer,
            Action::BatchModify(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
        .await
    }
}

impl Signable for BatchCancel {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp_sync(
            signer,
            Action::Cancel(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp(
            signer,
            Action::Cancel(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
        .await
    }
}

impl Signable for BatchCancelCloid {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp_sync(
            signer,
            Action::CancelByCloid(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp(
            signer,
            Action::CancelByCloid(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
        .await
    }
}

impl Signable for ScheduleCancel {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp_sync(
            signer,
            Action::ScheduleCancel(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        sign_rmp(
            signer,
            Action::ScheduleCancel(self),
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
        .await
    }
}

// EIP-712 typed data actions (transfers and asset movements)
// These use direct EIP-712 typed data signing for better wallet UX
impl Signable for UsdSendAction {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::UsdSend>(&self, chain, None);
        sign_eip712_sync(signer, Action::UsdSend(self), typed_data, nonce)
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::UsdSend>(&self, chain, None);
        sign_eip712(signer, Action::UsdSend(self), typed_data, nonce).await
    }
}

impl Signable for SendAssetAction {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::SendAsset>(&self, chain, None);
        sign_eip712_sync(signer, Action::SendAsset(self), typed_data, nonce)
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::SendAsset>(&self, chain, None);
        sign_eip712(signer, Action::SendAsset(self), typed_data, nonce).await
    }
}

impl Signable for SpotSendAction {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::SpotSend>(&self, chain, None);
        sign_eip712_sync(signer, Action::SpotSend(self), typed_data, nonce)
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::SpotSend>(&self, chain, None);
        sign_eip712(signer, Action::SpotSend(self), typed_data, nonce).await
    }
}

impl Signable for ApproveAgent {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::ApproveAgent>(&self, chain, None);
        sign_eip712_sync(signer, Action::ApproveAgent(self), typed_data, nonce)
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        _maybe_vault_address: Option<Address>,
        _maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        let typed_data = get_typed_data::<solidity::ApproveAgent>(&self, chain, None);
        sign_eip712(signer, Action::ApproveAgent(self), typed_data, nonce).await
    }
}

impl Signable for MultiSigAction {
    fn sign_sync<S: SignerSync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        multisig_lead_msg_sync(
            signer,
            self,
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
    }

    async fn sign<S: Signer + Send + Sync>(
        self,
        signer: &S,
        nonce: u64,
        maybe_vault_address: Option<Address>,
        maybe_expires_after: Option<DateTime<Utc>>,
        chain: Chain,
    ) -> Result<ActionRequest> {
        multisig_lead_msg(
            signer,
            self,
            nonce,
            maybe_vault_address,
            maybe_expires_after,
            chain,
        )
        .await
    }
}

/// Send a signed action hashing with typed data (synchronous).
pub(super) fn sign_eip712_sync<S: SignerSync>(
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

/// Send a signed action hashing with typed data (asynchronous).
pub(super) async fn sign_eip712<S: Signer + Send + Sync>(
    signer: &S,
    action: Action,
    typed_data: TypedData,
    nonce: u64,
) -> Result<ActionRequest> {
    let signature = signer.sign_dynamic_typed_data(&typed_data).await?;
    let sig: Signature = signature.into();

    Ok(ActionRequest {
        signature: sig,
        action,
        nonce,
        vault_address: None,
        expires_after: None,
    })
}

/// Signs an action using RMP (messagepack) hashing (synchronous).
pub(super) fn sign_rmp_sync<S: SignerSync>(
    signer: &S,
    action: Action,
    nonce: u64,
    maybe_vault_address: Option<Address>,
    maybe_expires_after: Option<DateTime<Utc>>,
    chain: Chain,
) -> Result<ActionRequest> {
    let expires_after = maybe_expires_after.map(|after| after.timestamp_millis() as u64);
    let connection_id = action.hash(nonce, maybe_vault_address, expires_after)?;

    let signature = sign_l1_action_sync(signer, chain, connection_id)?;

    Ok(ActionRequest {
        signature,
        action,
        nonce,
        vault_address: maybe_vault_address,
        expires_after,
    })
}

/// Signs an action using RMP (messagepack) hashing (asynchronous).
pub(super) async fn sign_rmp<S: Signer + Send + Sync>(
    signer: &S,
    action: Action,
    nonce: u64,
    maybe_vault_address: Option<Address>,
    maybe_expires_after: Option<DateTime<Utc>>,
    chain: Chain,
) -> Result<ActionRequest> {
    let expires_after = maybe_expires_after.map(|after| after.timestamp_millis() as u64);
    let connection_id = action.hash(nonce, maybe_vault_address, expires_after)?;

    let signature = sign_l1_action(signer, chain, connection_id).await?;

    Ok(ActionRequest {
        signature,
        action,
        nonce,
        vault_address: maybe_vault_address,
        expires_after,
    })
}

/// Signs an L1 action with EIP-712.
#[inline(always)]
pub(super) fn sign_l1_action_sync<S: SignerSync>(
    signer: &S,
    chain: Chain,
    connection_id: B256,
) -> anyhow::Result<Signature> {
    let sig = signer.sign_typed_data_sync(
        &solidity::Agent {
            source: if chain.is_mainnet() { "a" } else { "b" }.to_string(),
            connectionId: connection_id,
        },
        &CORE_MAINNET_EIP712_DOMAIN,
    )?;
    Ok(sig.into())
}

/// Signs an L1 action with EIP-712.
#[doc(hidden)]
#[inline(always)]
pub async fn sign_l1_action<S: Signer + Send + Sync>(
    signer: &S,
    chain: Chain,
    connection_id: B256,
) -> anyhow::Result<Signature> {
    let sig = signer
        .sign_typed_data(
            &solidity::Agent {
                source: if chain.is_mainnet() { "a" } else { "b" }.to_string(),
                connectionId: connection_id,
            },
            &CORE_MAINNET_EIP712_DOMAIN,
        )
        .await?;
    Ok(sig.into())
}

/// Signs a multisig action for submission to the exchange (synchronous).
///
/// This function creates the final signature that wraps all the collected multisig signatures.
/// The lead signer signs an envelope containing:
/// - The chain identifier (mainnet/testnet)
/// - The hash of the entire multisig action (including all signer signatures)
/// - The nonce
///
/// # EIP-712 Domain
///
/// Always uses the mainnet multisig domain (chainId 0x66eee) for both mainnet and testnet.
/// The `hyperliquidChain` field in the message distinguishes between mainnet and testnet.
///
/// # Parameters
///
/// - `signer`: The lead signer who submits the transaction
/// - `chain`: The chain (mainnet/testnet) - determines the hyperliquidChain field
/// - `action`: The complete multisig action with all collected signatures
/// - `nonce`: Unique transaction nonce
/// - `maybe_vault_address`: Optional vault address if trading on behalf of a vault
/// - `maybe_expires_after`: Optional expiration time for the request
#[doc(hidden)]
pub fn multisig_lead_msg_sync<S: SignerSync>(
    signer: &S,
    action: MultiSigAction,
    nonce: u64,
    maybe_vault_address: Option<Address>,
    maybe_expires_after: Option<DateTime<Utc>>,
    chain: Chain,
) -> Result<ActionRequest> {
    let expires_after = maybe_expires_after.map(|after| after.timestamp_millis() as u64);
    let multsig_hash = rmp_hash(&action, nonce, maybe_vault_address, expires_after)?;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        hyperliquid_chain: String,
        multi_sig_action_hash: String,
        nonce: u64,
    }

    let envelope = Envelope {
        hyperliquid_chain: chain.to_string(),
        multi_sig_action_hash: multsig_hash.to_string(),
        nonce,
    };

    let typed_data = get_typed_data::<solidity::SendMultiSig>(&envelope, chain, None);

    let sig = signer.sign_dynamic_typed_data_sync(&typed_data)?.into();

    Ok(ActionRequest {
        signature: sig,
        action: Action::MultiSig(action),
        nonce,
        vault_address: maybe_vault_address,
        expires_after,
    })
}

/// Signs a multisig action for submission to the exchange (asynchronous).
///
/// This function creates the final signature that wraps all the collected multisig signatures.
/// The lead signer signs an envelope containing:
/// - The chain identifier (mainnet/testnet)
/// - The hash of the entire multisig action (including all signer signatures)
/// - The nonce
///
/// # EIP-712 Domain
///
/// Always uses the mainnet multisig domain (chainId 0x66eee) for both mainnet and testnet.
/// The `hyperliquidChain` field in the message distinguishes between mainnet and testnet.
///
/// # Parameters
///
/// - `signer`: The lead signer who submits the transaction
/// - `chain`: The chain (mainnet/testnet) - determines the hyperliquidChain field
/// - `action`: The complete multisig action with all collected signatures
/// - `nonce`: Unique transaction nonce
/// - `maybe_vault_address`: Optional vault address if trading on behalf of a vault
/// - `maybe_expires_after`: Optional expiration time for the request
#[doc(hidden)]
pub async fn multisig_lead_msg<S: Signer + Send + Sync>(
    signer: &S,
    action: MultiSigAction,
    nonce: u64,
    maybe_vault_address: Option<Address>,
    maybe_expires_after: Option<DateTime<Utc>>,
    chain: Chain,
) -> Result<ActionRequest> {
    let expires_after = maybe_expires_after.map(|after| after.timestamp_millis() as u64);
    let multsig_hash = rmp_hash(&action, nonce, maybe_vault_address, expires_after)?;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Envelope {
        hyperliquid_chain: String,
        multi_sig_action_hash: String,
        nonce: u64,
    }

    let envelope = Envelope {
        hyperliquid_chain: chain.to_string(),
        multi_sig_action_hash: multsig_hash.to_string(),
        nonce,
    };

    let typed_data = get_typed_data::<solidity::SendMultiSig>(&envelope, chain, None);
    let sig = signer.sign_dynamic_typed_data(&typed_data).await?.into();

    Ok(ActionRequest {
        signature: sig,
        action: Action::MultiSig(action),
        nonce,
        vault_address: maybe_vault_address,
        expires_after,
    })
}

/// Collects signatures from all signers for a multisig action, with support for appending pre-existing signatures.
///
/// This function implements the Hyperliquid multisig signature collection protocol.
/// It handles both EIP-712 typed data actions (transfers) and RMP-based actions (orders, cancels).
/// Additionally, it allows you to append pre-existing signatures that were collected separately.
///
/// # Process
///
/// For EIP-712 typed data actions (UsdSend, SpotSend, SendAsset):
/// 1. Gets multisig typed data via `action.typed_data_multisig()`
/// 2. Each signer signs the typed data directly
/// 3. Appends pre-existing signatures to the collection
///
/// For RMP-based actions (orders, cancels, modifications):
/// 1. Creates an RMP hash from: `[multisig_user, lead_signer, action]`
/// 2. Each signer signs the hash using EIP-712 with the L1 Agent wrapper
/// 3. Appends pre-existing signatures to the collection
///
/// # Address Normalization
///
/// Both the multisig user address and lead signer address are normalized to lowercase
/// before hashing. This ensures consistency across different address representations.
///
/// # Parameters
///
/// - `lead`: The lead signer who will submit the transaction
/// - `multi_sig_user`: The multisig account address
/// - `signers`: Iterator of signers who will sign the action (generates new signatures)
/// - `signed`: Iterator of pre-existing signatures to append (collected separately)
/// - `inner_action`: The action to be signed (Order, Cancel, etc.)
/// - `nonce`: Unique transaction nonce
/// - `chain`: The chain (mainnet/testnet)
///
/// # Returns
///
/// A `MultiSigAction` containing all collected signatures (new + existing) and the action payload.
///
/// # Use Cases
///
/// - **Offline signature collection**: Collect signatures asynchronously and combine them
/// - **Partial multisigs**: Add additional signatures to an existing partial multisig
/// - **Multi-source aggregation**: Combine signatures from different systems or parties
///
/// # Example
///
/// ```rust,ignore
/// use hypersdk::hypercore::signing::multisig_collect_signatures;
///
/// // Collect new signatures from some signers and append pre-existing ones
/// let multisig_action = multisig_collect_signatures(
///     lead_address,
///     multisig_wallet_address,
///     vec![&signer1, &signer2].into_iter(),
///     existing_signatures.into_iter(),  // Pre-collected signatures
///     Action::Order(batch_order),
///     nonce,
///     Chain::Mainnet,
/// )?;
/// ```
///
/// # Reference
///
/// Based on: https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/be7523d58297a93d0e938063460c14ae45e9034f/hyperliquid/utils/signing.py#L293
pub(super) async fn multisig_collect_signatures<'a, S: Signer + Send + Sync + 'a>(
    lead: Address,
    multi_sig_user: Address,
    signers: impl Iterator<Item = &'a S>,
    signed: impl Iterator<Item = Signature>,
    inner_action: Action,
    nonce: u64,
    chain: Chain,
) -> Result<MultiSigAction> {
    // Normalize addresses (required for consistent hashing)
    let multi_sig_user_str = multi_sig_user.to_string().to_lowercase();
    let lead_str = lead.to_string().to_lowercase();

    // Dispatch to specialized function based on action type
    let mut signatures =
        if let Some(typed_data) = inner_action.typed_data_multisig(multi_sig_user, lead, chain) {
            // EIP-712 typed data actions (UsdSend, SpotSend, SendAsset)
            multisig_collect_eip712_signatures(signers, typed_data).await?
        } else {
            // RMP-based actions (orders, cancels, modifications)
            multisig_collect_rmp_signatures(
                signers,
                &multi_sig_user_str,
                &lead_str,
                &inner_action,
                nonce,
                chain,
            )
            .await?
        };
    signatures.extend(signed);

    Ok(MultiSigAction {
        signature_chain_id: ARBITRUM_TESTNET_CHAIN_ID.to_owned(),
        signatures,
        payload: MultiSigPayload {
            multi_sig_user: multi_sig_user_str,
            outer_signer: lead_str,
            action: Box::new(inner_action),
        },
    })
}

/// Collects signatures for EIP-712 typed data actions (transfers).
///
/// Creates the typed data object once, then has each signer sign it.
/// This is used for UsdSend, SpotSend, and SendAsset actions.
///
/// # Process
///
/// 1. Set the multisig EIP-712 domain on the typed data
/// 2. Each signer signs the same typed data
/// 3. Return all signatures
async fn multisig_collect_eip712_signatures<'a, S: Signer + Send + Sync + 'a>(
    signers: impl Iterator<Item = &'a S>,
    typed_data: TypedData,
) -> Result<Vec<Signature>> {
    // Prepare typed data once with the multisig domain
    let mut typed_data = typed_data;
    typed_data.domain = super::types::ARBITRUM_TESTNET_EIP712_DOMAIN;

    let mut signatures = vec![];
    for signer in signers {
        let signature = signer.sign_dynamic_typed_data(&typed_data).await?;
        signatures.push(signature.into());
    }

    Ok(signatures)
}

/// Collects signatures for RMP-based actions (orders, cancels, modifications).
///
/// Creates the RMP hash once, then has each signer sign it using EIP-712 Agent wrapper.
/// This is used for BatchOrder, BatchModify, BatchCancel, and similar actions.
///
/// # Process
///
/// 1. Create RMP hash from (multisig_user, lead, action, nonce)
/// 2. Each signer signs the hash using EIP-712 Agent wrapper
/// 3. Return all signatures
async fn multisig_collect_rmp_signatures<'a, S: Signer + Send + Sync + 'a>(
    signers: impl Iterator<Item = &'a S>,
    multi_sig_user: &str,
    lead: &str,
    action: &Action,
    nonce: u64,
    chain: Chain,
) -> Result<Vec<Signature>> {
    // Create the RMP hash once
    let connection_id = rmp_hash(&(multi_sig_user, lead, action), nonce, None, None)?;

    let mut signatures = vec![];
    for signer in signers {
        let signature = sign_l1_action(signer, chain, connection_id).await?;
        signatures.push(signature);
    }

    Ok(signatures)
}

#[cfg(test)]
mod tests {

    use alloy::signers::local::PrivateKeySigner;

    use super::*;
    use crate::hypercore::{
        ARBITRUM_MAINNET_CHAIN_ID,
        types::{self},
    };

    fn get_signer() -> PrivateKeySigner {
        let priv_key = "e908f86dbb4d55ac876378565aafeabc187f6690f046459397b17d9b9a19688e";
        priv_key.parse::<PrivateKeySigner>().unwrap()
    }

    #[test]
    fn test_sign_usd_transfer_action() {
        let signer = get_signer();

        let usd_send = types::raw::UsdSendAction {
            signature_chain_id: ARBITRUM_MAINNET_CHAIN_ID.to_owned(),
            hyperliquid_chain: Chain::Mainnet,
            destination: "0x0D1d9635D0640821d15e323ac8AdADfA9c111414"
                .parse()
                .unwrap(),
            amount: rust_decimal::Decimal::ONE,
            time: 1690393044548,
        };
        // Get typed data directly for UsdSend
        let typed_data = get_typed_data::<solidity::UsdSend>(&usd_send, Chain::Mainnet, None);
        let signature = signer.sign_dynamic_typed_data_sync(&typed_data).unwrap();

        let expected_sig = "0xeca6267bcaadc4c0ae1aed73f5a2c45fcdbb7271f2e9356992404e5d4bad75a3572e08fe93f17755abadb7f84be7d1e9c4ce48bb5633e339bc430c672d5a20ed1b";
        assert_eq!(signature.to_string(), expected_sig);
    }
}
