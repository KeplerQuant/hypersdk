//! Morpho Blue lending protocol integration.
//!
//! This module provides clients for interacting with Morpho Blue, a decentralized
//! lending protocol, and MetaMorpho vaults on HyperEVM.
//!
//! # Overview
//!
//! Morpho Blue is an efficient lending protocol that allows users to:
//! - Supply assets to earn interest
//! - Borrow assets with collateral
//! - Create isolated lending markets
//!
//! MetaMorpho vaults aggregate multiple Morpho markets to optimize yields.
//!
//! # Clients
//!
//! - [`Client`]: For interacting with individual Morpho Blue markets
//! - [`MetaClient`]: For interacting with MetaMorpho vaults
//!
//! # Examples
//!
//! ## Query Market APY
//!
//! ```no_run
//! use hypersdk::hyperevm::morpho;
//! use hypersdk::Address;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let client = morpho::Client::mainnet().await?;
//!
//! let morpho_addr: Address = "0x...".parse()?;
//! let market_id = [0u8; 32].into();
//!
//! let apy = client.apy(morpho_addr, market_id).await?;
//! println!("Borrow APY: {:.2}%", apy.borrow * 100.0);
//! println!("Supply APY: {:.2}%", apy.supply * 100.0);
//! # Ok(())
//! # }
//! ```
//!
//! ## Query MetaMorpho Vault APY
//!
//! ```no_run
//! use hypersdk::hyperevm::morpho;
//! use hypersdk::Address;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let client = morpho::MetaClient::mainnet().await?;
//!
//! let vault_addr: Address = "0x...".parse()?;
//! let vault_apy = client.apy(vault_addr).await?;
//!
//! println!("Vault APY: {:.2}%", vault_apy.apy() * 100.0);
//! println!("Fee: {:.2}%", vault_apy.fee * 100.0);
//! # Ok(())
//! # }
//! ```

use alloy::{
    primitives::{Address, FixedBytes, U256},
    providers::Provider,
    transports::TransportError,
};

use crate::hyperevm::{
    DynProvider, ERC20,
    morpho::contracts::{
        IIrm,
        IMetaMorpho::{self, IMetaMorphoInstance},
        IMorpho::{self, IMorphoInstance},
        Market, MarketParams,
    },
};

pub mod contracts;

/// Morpho market identifier.
///
/// A 32-byte unique identifier for a Morpho Blue market.
pub type MarketId = FixedBytes<32>;

/// Annual Percentage Yield (APY) for a Morpho market.
///
/// Contains both borrow and supply APY rates for a lending market.
///
/// # Example
///
/// Query APY for a market: `client.apy(morpho_addr, market_id).await?`
/// Access borrow and supply APY via `apy.borrow` and `apy.supply` fields.
#[derive(Debug, Clone)]
pub struct PoolApy {
    /// Market parameters (loan token, collateral, oracle, IRM, LLTV)
    pub params: MarketParams,
    /// Current market state (supply, borrow, fees)
    pub market: Market,
    /// Borrow APY as a decimal (0.05 = 5%)
    pub borrow: f64,
    /// Supply APY as a decimal (0.03 = 3%)
    pub supply: f64,
}

/// MetaMorpho vault APY information.
///
/// A MetaMorpho vault aggregates multiple Morpho markets to optimize yields.
/// This struct contains all the information needed to calculate the vault's APY.
///
/// # Example
///
/// Query vault APY: `client.apy(vault_addr).await?`
/// Calculate effective APY after fees using `vault_apy.apy()` method.
/// Individual market data available in `vault_apy.components`.
#[derive(Debug, Clone)]
pub struct VaultApy {
    /// Individual markets that compose this vault
    pub components: Vec<VaultSupply>,
    /// Vault management fee as a decimal (0.10 = 10%)
    pub fee: f64,
    /// Total assets deposited into the vault
    pub total_deposits: f64,
}

#[derive(Debug, Clone)]
pub struct VaultSupply {
    pub supplied_shares: U256,
    pub pool: PoolApy,
}

impl VaultApy {
    /// Calculates the effective vault APY after fees.
    ///
    /// This is a weighted average of all underlying market APYs, adjusted for
    /// the vault's management fee.
    ///
    /// # Returns
    ///
    /// The APY as a decimal (e.g., 0.05 = 5%).
    ///
    /// # Example
    ///
    /// Calculate vault APY: `vault_apy.apy()` returns the effective APY as a decimal.
    #[must_use]
    pub fn apy(&self) -> f64 {
        if self.total_deposits == 0.0 {
            return 0.0;
        }

        self.components
            .iter()
            .map(|component| {
                // Calculate supplied shares (see Morpho SharesMathLib.sol)
                let supplied_shares =
                    (component.supplied_shares / U256::from(1_000_000u64)).to::<u64>() as f64;

                // Convert shares to assets using price per share
                let price_per_share = if component.pool.market.totalSupplyShares == 0 {
                    0.0
                } else {
                    component.pool.market.totalSupplyAssets as f64
                        / component.pool.market.totalSupplyShares as f64
                };

                let supplied_assets = price_per_share * supplied_shares;

                // Weight by proportion of total deposits
                let weight = supplied_assets / self.total_deposits;
                weight * component.pool.supply
            })
            .sum::<f64>()
            * (1.0 - self.fee)
    }

    /// Returns the gross APY before fees.
    ///
    /// This is the weighted average APY without subtracting the management fee.
    #[must_use]
    pub fn gross_apy(&self) -> f64 {
        if self.total_deposits == 0.0 {
            return 0.0;
        }

        self.components
            .iter()
            .map(|component| {
                let supplied_shares =
                    (component.supplied_shares / U256::from(1_000_000u64)).to::<u64>() as f64;

                let price_per_share = if component.pool.market.totalSupplyShares == 0 {
                    0.0
                } else {
                    component.pool.market.totalSupplyAssets as f64
                        / component.pool.market.totalSupplyShares as f64
                };

                let supplied_assets = price_per_share * supplied_shares;
                let weight = supplied_assets / self.total_deposits;
                weight * component.pool.supply
            })
            .sum()
    }

    /// Returns the number of markets in the vault.
    #[must_use]
    pub fn market_count(&self) -> usize {
        self.components.len()
    }
}

/// Client for Morpho Blue lending markets.
///
/// Provides methods for querying market information and calculating APYs.
///
/// # Example
///
/// ```no_run
/// use hypersdk::hyperevm::morpho;
/// use hypersdk::Address;
///
/// # async fn example() -> anyhow::Result<()> {
/// // Create a mainnet client
/// let client = morpho::Client::mainnet().await?;
///
/// // Query a market's APY
/// let morpho_addr: Address = "0x...".parse()?;
/// let market_id = [0u8; 32].into();
/// let apy = client.apy(morpho_addr, market_id).await?;
///
/// println!("Supply APY: {:.2}%", apy.supply * 100.0);
/// # Ok(())
/// # }
/// ```
pub struct Client<P>
where
    P: Provider,
{
    provider: P,
}

impl Client<DynProvider> {
    /// Creates a client for HyperEVM mainnet.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hypersdk::hyperevm::morpho;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let client = morpho::Client::mainnet().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn mainnet() -> Result<Self, TransportError> {
        let provider = DynProvider::new(super::mainnet().await?);
        Ok(Self::new(provider))
    }

    /// Creates a client with a custom RPC URL.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hypersdk::hyperevm::morpho;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let client = morpho::Client::mainnet_with_url("https://custom-rpc.example.com").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn mainnet_with_url(url: &str) -> Result<Self, TransportError> {
        let provider = DynProvider::new(super::mainnet_with_url(url).await?);
        Ok(Self::new(provider))
    }
}

impl<P> Client<P>
where
    P: Provider + Clone,
{
    /// Creates a new Morpho client with a custom provider.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hypersdk::hyperevm::{self, morpho};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let provider = hyperevm::mainnet().await?;
    /// let client = morpho::Client::new(provider);
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Returns a reference to the underlying provider.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Creates a Morpho contract instance at the given address.
    ///
    /// Use this to call Morpho contract methods directly.
    pub fn instance(&self, address: Address) -> IMorphoInstance<P> {
        IMorpho::new(address, self.provider.clone())
    }

    /// Calculates the APY for a specific Morpho market.
    ///
    /// # Parameters
    ///
    /// - `address`: The Morpho Blue contract address
    /// - `market_id`: The unique market identifier
    ///
    /// # Returns
    ///
    /// A `PoolApy` containing borrow and supply APY rates.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use hypersdk::hyperevm::morpho;
    /// use hypersdk::Address;
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// let client = morpho::Client::mainnet().await?;
    /// let morpho_addr: Address = "0x...".parse()?;
    /// let market_id = [0u8; 32].into();
    ///
    /// let apy = client.apy(morpho_addr, market_id).await?;
    /// println!("Borrow: {:.2}%, Supply: {:.2}%",
    ///     apy.borrow * 100.0, apy.supply * 100.0);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn apy(&self, address: Address, market_id: MarketId) -> anyhow::Result<PoolApy> {
        let morpho = IMorpho::new(address, self.provider.clone());
        let (params, market) = self
            .provider
            .multicall()
            .add(morpho.idToMarketParams(market_id))
            .add(morpho.market(market_id))
            .aggregate()
            .await?;
        self.apy_with(params, market).await
    }

    /// Returns the APY of the market.
    pub async fn apy_with(
        &self,
        params: impl Into<MarketParams>,
        market: impl Into<Market>,
    ) -> anyhow::Result<PoolApy> {
        let params = params.into();
        let market = market.into();
        let irm = IIrm::new(params.irm, self.provider.clone());
        let rate = irm
            .borrowRateView(params.into(), market.into())
            .call()
            .await?;

        let fee = market.fee as f64 / 1e18;
        let utilization = market.totalBorrowAssets as f64 / market.totalSupplyAssets as f64;
        let rate = rate.to::<u64>() as f64 / 1e18;
        let borrow_apy = (rate * 31_536_000f64).exp() - 1.0;
        let supply_apy = borrow_apy * utilization * (1.0 - fee);
        Ok(PoolApy {
            params,
            market,
            borrow: borrow_apy,
            supply: supply_apy,
        })
    }
}

/// MetaMorpho client
pub struct MetaClient<P>
where
    P: Provider,
{
    provider: P,
}

impl MetaClient<DynProvider> {
    /// Creates a client for mainnet.
    pub async fn mainnet() -> Result<Self, TransportError> {
        let provider = DynProvider::new(super::mainnet().await?);
        Ok(Self::new(provider))
    }

    /// Creates a client for mainnet.
    pub async fn mainnet_with_url(url: &str) -> Result<Self, TransportError> {
        let provider = DynProvider::new(super::mainnet_with_url(url).await?);
        Ok(Self::new(provider))
    }
}

impl<P> MetaClient<P>
where
    P: Provider + Clone,
{
    /// Create a uniswap client.
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Returns the root provider.
    pub fn provider(&self) -> &P {
        &self.provider
    }

    /// Creates a MetaMorphoInstance.
    pub fn instance(&self, address: Address) -> IMetaMorphoInstance<P> {
        IMetaMorpho::new(address, self.provider.clone())
    }

    /// Returns the pool's APY.
    ///
    /// <https://github.com/morpho-org/metamorpho-v1.1/blob/main/src/MetaMorphoV1_1.sol#L796>
    pub async fn apy(&self, address: Address) -> anyhow::Result<VaultApy> {
        let meta_morpho = IMetaMorpho::new(address, self.provider.clone());
        // the vault is at the same time a token and holds balances
        let vault_erc20 = ERC20::new(address, self.provider.clone());
        let (fee, supply_queue_len, total_supply, morpho_addr) = self
            .provider
            .multicall()
            .add(meta_morpho.fee())
            .add(meta_morpho.supplyQueueLength())
            .add(vault_erc20.totalSupply())
            .add(meta_morpho.MORPHO())
            .aggregate()
            .await?;
        // vault fee
        let fee = fee.to::<u64>() as f64 / 1e18;
        // total deposits in the vault
        let total_deposits = (total_supply / U256::from(1e18)).to::<u64>() as f64;
        let supply_queue_len = supply_queue_len.to::<usize>();

        let morpho = IMorpho::new(morpho_addr, self.provider.clone());

        let mut apy = VaultApy {
            components: vec![],
            fee,
            total_deposits,
        };
        for i in 0..supply_queue_len {
            // TODO: is there a way to aggregate this?
            let market_id = meta_morpho.supplyQueue(U256::from(i)).call().await?;

            let (config, params, market) = self
                .provider
                .multicall()
                .add(meta_morpho.config(market_id))
                .add(morpho.idToMarketParams(market_id))
                .add(morpho.market(market_id))
                .aggregate()
                .await?;

            if !config.enabled
                || params.irm.is_zero()
                || params.collateralToken.is_zero()
                || params.loanToken.is_zero()
            {
                // println!("{} has no IRM?", market_id);
                continue;
            }

            let position = morpho
                .position(market_id, *meta_morpho.address())
                .call()
                .await?;

            let pool = Client::new(self.provider.clone())
                .apy_with(params, market)
                .await?;

            apy.components.push(VaultSupply {
                supplied_shares: position.supplyShares,
                pool,
            });
        }

        Ok(apy)
    }
}
