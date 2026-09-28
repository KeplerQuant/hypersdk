# Transfers, agents, subaccounts, vaults, multisig

All return `anyhow::Result<()>`; an exchange rejection is an `ApiError`.

Signing scheme, per `Action::signing_typed_data` in `src/hypercore/types/api.rs`:
`send_usdc`, `spot_send`, `send_asset`, `transfer_to_perps`/`transfer_to_spot` (they build a
`SendAsset`), `transfer_to_evm` (a `SpotSend`), `usd_class_transfer`, `withdraw`,
`approve_agent` and `approve_builder_fee` are **EIP-712 user-signed**: sign them with the
account's own key. `vault_transfer`, `agent_send_asset` and `create_sub_account` are L1
(msgpack) actions like orders. `agent_send_asset` exists specifically so an agent can move funds
between the account's own balances.

**Nonce rule:** for `UsdSend.time`, `SpotSend.time` and `SendAsset.nonce` pass the same value as
the `nonce` argument (`UsdSend.time` "doubles as the action nonce").

## Signatures

```rust
pub async fn send_usdc<S: SignerSync>(&self, signer: &S, send: UsdSend, nonce: u64) -> Result<()>
pub async fn transfer_to_perps<S: Signer + SignerSync>(&self, signer: &S, token: SpotToken, amount: Decimal, nonce: u64) -> Result<()>
pub async fn transfer_to_spot<S: Signer + SignerSync>(&self, signer: &S, token: SpotToken, amount: Decimal, nonce: u64) -> Result<()>
pub async fn usd_class_transfer<S: SignerSync>(&self, signer: &S, amount: Decimal, to_perp: bool, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>) -> Result<()>
pub fn spot_send<S: SignerSync>(&self, signer: &S, send: SpotSend, nonce: u64) -> impl Future<Output = Result<()>> + Send + 'static
pub fn send_asset<S: SignerSync>(&self, signer: &S, send: SendAsset, nonce: u64) -> impl Future<Output = Result<()>> + Send + 'static
pub fn agent_send_asset<S: SignerSync>(&self, signer: &S, send: AgentSendAsset, nonce: u64) -> impl Future<Output = Result<()>> + Send + 'static
pub async fn transfer_to_evm<S: Send + SignerSync>(&self, signer: &S, token: SpotToken, amount: Decimal, nonce: u64) -> Result<()>
pub async fn withdraw<S: SignerSync>(&self, signer: &S, destination: Address, amount: Decimal, nonce: u64,
    vault_address: Option<Address>, expires_after: Option<DateTime<Utc>>) -> Result<()>
pub async fn vault_transfer<S: SignerSync>(&self, signer: &S, vault_address: Address, usd: Decimal, nonce: u64, is_deposit: bool) -> Result<()>
pub async fn approve_agent<S: Signer + Send + Sync>(&self, signer: &S, agent: Address, name: String, nonce: u64) -> Result<()>
pub async fn approve_builder_fee<S: Signer + Send + Sync>(&self, signer: &S, builder: Address, max_fee_rate: String, nonce: u64) -> Result<()>
```

| Method | Moves |
| --- | --- |
| `send_usdc` | USDC, your perp balance → another address's perp balance |
| `transfer_to_perps` / `transfer_to_spot` | USDC between your own spot and perp balances. `token` must be the USDC `SpotToken`, anything else errors |
| `usd_class_transfer` | same as above via the `usdClassTransfer` action; no example uses it |
| `spot_send` | any spot token to another address |
| `send_asset` | a token between perp / spot / HIP-3 DEX balances and subaccounts; `AssetTarget::{Perp, Spot, Dex(String)}` |
| `transfer_to_evm` | a spot token to your HyperEVM address; the token needs `cross_chain_address` |
| `withdraw` | USDC to Arbitrum (doc: $1 fee, ~5 min) |
| `vault_transfer` | deposit (`is_deposit: true`) or withdraw USDC to/from a vault; `usd` is plain dollars |

## USDC between accounts and balances

From `examples/hypercore/send_usd.rs` and `transfer_to_perps.rs`:

```rust
use std::str::FromStr;

use hypersdk::{
    Address,
    hypercore::{self, NonceHandler, PrivateKeySigner, types::UsdSend},
};
use rust_decimal::dec;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let to: Address = std::env::var("TO")?.parse()?;
    let client = hypercore::testnet();
    let nonces = NonceHandler::default();

    // Perp USDC to another address.
    let nonce = nonces.next();
    client
        .send_usdc(&signer, UsdSend { destination: to, amount: dec!(10), time: nonce }, nonce)
        .await?;

    // Own spot balance -> own perp balance. Needs the USDC SpotToken.
    let usdc = client
        .spot_tokens()
        .await?
        .into_iter()
        .find(|t| t.name == "USDC")
        .ok_or_else(|| anyhow::anyhow!("USDC not found"))?;
    client.transfer_to_perps(&signer, usdc, dec!(25), nonces.next()).await?;
    Ok(())
}
```

## Spot tokens and cross-DEX moves

```rust
use hypersdk::{
    Address,
    hypercore::{
        self, NonceHandler, PrivateKeySigner, SpotToken,
        types::{AssetTarget, SendAsset, SendToken, SpotSend},
    },
};
use rust_decimal::Decimal;

async fn send_spot_token(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    token: SpotToken,
    to: Address,
    amount: Decimal,
) -> anyhow::Result<()> {
    let nonce = nonces.next();
    client
        .spot_send(signer, SpotSend { destination: to, token: SendToken(token), amount, time: nonce }, nonce)
        .await
}

/// Own USDC from the main perp balance to a HIP-3 DEX balance.
async fn fund_hip3_dex(
    client: &hypercore::HttpClient,
    signer: &PrivateKeySigner,
    nonces: &NonceHandler,
    usdc: SpotToken,
    dex: &str,
    amount: Decimal,
) -> anyhow::Result<()> {
    let nonce = nonces.next();
    client
        .send_asset(
            signer,
            SendAsset {
                destination: signer.address(),
                source_dex: AssetTarget::Perp,
                destination_dex: AssetTarget::Dex(dex.to_string()),
                token: SendToken(usdc),
                amount,
                from_sub_account: String::new(), // empty = main account
                nonce,
            },
            nonce,
        )
        .await
}

fn main() {}
```

`fund_hip3_dex` follows how `transfer_to_perps` builds its `SendAsset` internally; no example
sends to a HIP-3 DEX.

## Agents (API wallets)

From `examples/hypercore/approve_agent.rs`. Sign with the **main** account key. The doc limits:
1 unnamed agent, up to 3 named, 2 named per subaccount. An empty `name` means unnamed.

```rust
use std::str::FromStr;

use hypersdk::{
    Address,
    hypercore::{Chain, HttpClient, NonceHandler, PrivateKeySigner},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let owner = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let client = HttpClient::new(Chain::Testnet);

    // Generate the agent key locally; store it, the bot will sign orders with it.
    let agent = PrivateKeySigner::random();
    let agent_address: Address = agent.address();

    client
        .approve_agent(&owner, agent_address, "bot".to_string(), NonceHandler::default().next())
        .await?;

    for a in client.api_agents(owner.address()).await? {
        println!("agent {} {:?} valid_until={:?}", a.name, a.address, a.valid_until);
    }
    Ok(())
}
```

Then trade with `agent` as the signer and query state for `owner.address()`.

## Subaccounts

`subaccounts(master)` lists them (account.md). Per its doc, subaccounts have no keys: the master
signs, passing `vault_address = Some(sub_account_user)` to the trading method.
`create_sub_account(signer, name, nonce, expires_after) -> Result<Address>` creates one.

## Multisig

From `examples/hypercore/multisig_order.rs`. `multi_sig(lead, multisig_address, nonce)` returns a
builder; add signers, then call `.place(batch, vault_address, expires_after)`,
`.send_usdc(UsdSend)`, `.send_asset(SendAsset)`, `.approve_agent(..)`,
`.approve_builder_fee(..)` or `.convert_to_normal_user()`. `.place` returns
`Result<Vec<OrderResponseStatus>, ActionError<Cloid>>` like `HttpClient::place`.

```rust
use std::str::FromStr;

use hypersdk::{
    Address,
    hypercore::{
        self as hypercore, Chain, Cloid, NonceHandler, PrivateKeySigner,
        types::{BatchOrder, OrderGrouping, OrderRequest, OrderTypePlacement, TimeInForce},
    },
};
use rust_decimal::dec;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = hypercore::HttpClient::new(Chain::Testnet);
    let multisig_address: Address = std::env::var("MULTISIG")?.parse()?;

    // Every signer must be authorized on the multisig wallet.
    let signers: Vec<PrivateKeySigner> = std::env::var("KEYS")?
        .split(',')
        .map(PrivateKeySigner::from_str)
        .collect::<Result<_, _>>()?;

    let perps = client.perps().await?;
    let btc = perps.iter().find(|p| p.name == "BTC").ok_or_else(|| anyhow::anyhow!("btc"))?;

    let order = BatchOrder {
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
    };

    let resp = client
        .multi_sig(&signers[0], multisig_address, NonceHandler::default().next())
        .signers(&signers)
        .place(order, None, None)
        .await?;
    println!("Multisig order response: {resp:?}");
    Ok(())
}
```
