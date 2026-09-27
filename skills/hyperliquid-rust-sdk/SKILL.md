---
name: hyperliquid-rust-sdk
description: Write Rust code that uses the hypersdk crate to trade on Hyperliquid programmatically — placing/canceling orders, reading positions and account state, subscribing to market data via WebSocket, and signing/submitting transactions. Use this skill whenever writing or editing Rust code in a project that depends on hypersdk, for any order, position, balance, or market-data logic.
---

# hypersdk (Rust)

Crate `hypersdk` 0.2.16 — the Rust library, called directly from bot code. Not the `hypecli`
binary; never shell out.

Every signature and code block in this skill was checked against `src/` at 0.2.16 and
compile-checked in a consumer crate. The crate's own `///` examples are **not** a reliable
source: several reference items that do not exist (`Client::mainnet()`,
`ARBITRUM_SIGNATURE_CHAIN_ID`, `client.spot_meta()`, `Subscription::WebData2`). If something is
not in this skill, read `src/hypercore/http.rs` (all client methods) and
`src/hypercore/types/mod.rs` (all types) before writing it.

Code blocks that end in `fn main() {}` are function-level snippets; the empty `main` is only
there so each block compiles on its own. Blocks without `fn main` are signatures or type
shapes for reference, not code to paste.

## Where to look

| Task | File |
| --- | --- |
| Market metadata, asset index, coin names, price ticks, size decimals, mids, book, candles | [references/markets.md](references/markets.md) |
| Place / market / cancel / modify orders, leverage, scheduled cancel, TWAP, order statuses | [references/orders.md](references/orders.md) |
| Positions, margin, balances, open orders, order status, fills, fees, rate limit | [references/account.md](references/account.md) |
| WebSocket subscriptions, lifecycle, reconnects, posting over the socket | [references/websocket.md](references/websocket.md) |
| USDC / spot transfers, withdrawals, agents, subaccounts, vaults, multisig | [references/transfers.md](references/transfers.md) |
| Complete programs taken from `examples/` | [references/recipes.md](references/recipes.md) |

## Setup

```toml
[dependencies]
hypersdk = "0.2.16"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
futures = "0.3"                                              # StreamExt, for the WebSocket
anyhow = "1"                                                 # every client method returns anyhow::Result
rust_decimal = { version = "1.39", features = ["macros"] }  # Decimal + dec!()
chrono = "0.4"            # only for expires_after / schedule_cancel (DateTime<Utc>)
# reqwest = "0.13"        # only to downcast errors to reqwest::Error or pass with_http_client
# serde_json = "1"        # only to downcast serde_json::Error or build PostRequest::Info
```

- The crate defines **no Cargo features**. Nothing to enable.
- It is edition 2024 with `rust-version = "1.94.1"`. Older toolchains are refused by cargo.
- Keystore decryption (`PrivateKeySigner::decrypt_keystore`) is not enabled by hypersdk's own
  alloy features; it needs `alloy = { version = "2", features = ["signer-keystore"] }` in your
  crate. Raw hex keys need nothing extra.

Minimal working program:

```rust
use std::str::FromStr;

use hypersdk::hypercore::{self, PrivateKeySigner};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Hex private key, with or without 0x.
    let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let client = hypercore::testnet();

    let perps = client.perps().await?;
    let btc = perps
        .iter()
        .find(|m| m.name == "BTC")
        .ok_or_else(|| anyhow::anyhow!("BTC not listed"))?;
    println!("BTC asset={} sz_decimals={}", btc.index, btc.sz_decimals);

    let state = client.clearinghouse_state(signer.address(), None).await?;
    println!("account value {}", state.margin_summary.account_value);
    Ok(())
}
```

## Client / connection

```rust
pub fn hypercore::mainnet() -> HttpClient          // https://api.hyperliquid.xyz
pub fn hypercore::testnet() -> HttpClient          // https://api.hyperliquid-testnet.xyz
pub fn HttpClient::new(chain: Chain) -> Self       // Chain::Mainnet | Chain::Testnet
pub fn HttpClient::with_url(self, base_url: Url) -> Self               // keeps the chain
pub fn HttpClient::with_http_client(self, http_client: reqwest::Client) -> Self
pub const fn HttpClient::chain(&self) -> Chain
pub fn HttpClient::websocket(&self) -> WebSocket   // wss://<same host>/ws
pub fn hypercore::mainnet_ws() -> WebSocket
pub fn hypercore::testnet_ws() -> WebSocket
```

- `HttpClient` (= `hypercore::http::Client`) is **not `Clone`**. Share it as `Arc<HttpClient>`.
- The default inner `reqwest::Client` has a 10 s timeout.
- The chain decides the signing domain. A testnet client signs for testnet; you cannot reuse a
  request across networks.
- `PrivateKeySigner` is re-exported as `hypersdk::hypercore::PrivateKeySigner` (alloy's local
  signer). `signer.address()` gives the account address.
- Creating a `WebSocket` calls `tokio::spawn`, so it must happen inside a Tokio runtime.

A shape that fits a bot, following the patterns in `examples/hypercore/`:

```rust
use std::{str::FromStr, sync::Arc};

use hypersdk::{
    Address,
    hypercore::{self, HttpClient, NonceHandler, PrivateKeySigner},
};

pub struct Exchange {
    pub client: Arc<HttpClient>,
    pub signer: PrivateKeySigner,
    /// One per signing key. Thread-safe; `next()` is unique and increasing.
    pub nonces: Arc<NonceHandler>,
    /// The account whose state you query (see "Agents" below).
    pub account: Address,
}

impl Exchange {
    pub fn from_env(testnet: bool) -> anyhow::Result<Self> {
        let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
        let client = if testnet { hypercore::testnet() } else { hypercore::mainnet() };
        Ok(Self {
            account: signer.address(),
            client: Arc::new(client),
            signer,
            nonces: Arc::new(NonceHandler::default()),
        })
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ex = Exchange::from_env(true)?;
    let mids = ex.client.all_mids(None).await?;
    println!("BTC mid {:?}, nonce {}", mids.get("BTC"), ex.nonces.next());
    Ok(())
}
```

## Rules that keep generated code correct

1. **Prices and sizes are `rust_decimal::Decimal`**, never `f64`. Use `dec!(0.01)`, `.parse()`,
   or `Decimal::try_from(f64)` at the boundary (as `examples/hypercore/market_order.rs` does).
2. **Order and cancel `asset` is the market's `index`** from `perps()` / `spot()` /
   `outcomes()`. Do not hardcode it; spot and HIP-3 indices are offset (see markets.md). Types
   differ: `OrderRequest.asset`, `Cancel.asset`, `update_leverage` take `usize`;
   `CancelByCloid.asset` and `top_up_isolated_only_margin` take `u32`.
3. **Info calls and WebSocket subscriptions take a coin string**, not the index: `"BTC"`,
   `"xyz:XYZ100"` (HIP-3), the spot market's `name` (`"PURR/USDC"`, `"@107"`), or
   `OutcomeMarket::coin()` (`"#42"`).
4. **Round every price** with `market.round_price(px)` or `market.round_by_side(side, px,
   conservative)`; both return `Option<Decimal>` (`None` for zero/negative prices). There is
   **no size-rounding helper**: truncate sizes to `sz_decimals` yourself (markets.md).
5. **Every signed action needs a fresh nonce.** Use one `NonceHandler` per signing key and call
   `next()` per request. For `UsdSend.time`, `SpotSend.time` and `SendAsset.nonce`, pass the
   **same** value you pass as `nonce`.
6. **A batch "succeeding" does not mean the orders did.** `place` returns
   `Ok(Vec<OrderResponseStatus>)`, and a rejected order is `OrderResponseStatus::Error(String)`
   inside that `Ok`. Check every element.
7. **`vault_address`** is `None` for your own account. The `subaccounts()` doc says to act on a
   subaccount, the master signs with `vault_address = Some(subaccount_address)`; the same
   parameter is used when trading for a vault.
8. **Agents (API wallets):** `approve_agent` is signed by the main account. When a bot signs
   with an agent key, query positions/orders for the **owning account**, not
   `signer.address()`; `client.user_role(agent)` returns `UserRole::Agent { user }` with that
   account.
9. `place`, `cancel`, `cancel_by_cloid`, `modify`, `send_asset`, `spot_send` and the outcome
   methods are plain `fn`s returning `impl Future<Output = …> + Send + 'static`. They **sign at
   call time**, don't borrow the client or signer, and can go straight into `tokio::spawn`.

## Error handling

Every fallible call returns `anyhow::Result`, except the four batch order methods.

| Calls | Return type | What an error means |
| --- | --- | --- |
| Info queries (`perps`, `clearinghouse_state`, `open_orders`, `all_mids`, …) | `anyhow::Result<T>` | `ApiError` = non-2xx HTTP (message has `[label] HTTP <status> body=…`); `serde_json::Error` (with body context) = response shape changed; `reqwest::Error` = network or timeout |
| `place`, `cancel_by_cloid` | `Result<Vec<OrderResponseStatus>, ActionError<Cloid>>` | The whole request failed: signing, network, HTTP, or the exchange answered `{"status":"err"}` |
| `cancel` | `Result<Vec<OrderResponseStatus>, ActionError<u64>>` | same, ids are oids |
| `modify` | `Result<Vec<OrderResponseStatus>, ActionError<OidOrCloid>>` | same |
| `market_open`, `modify_order` | `anyhow::Result<Vec<OrderResponseStatus>>` | same, converted into anyhow |
| Other signed actions returning `()` (`update_leverage`, `send_usdc`, `withdraw`, …) | `anyhow::Result<()>` | `ApiError(msg)` when the exchange answers `{"status":"err"}` or an unexpected response type |
| `twap_order`, `twap_cancel`, `gossip_priority_bid` | `anyhow::Result<Response>` | Transport/HTTP only; you match `Response::Ok` / `Response::Err` yourself |

`ActionError<T>` (`hypersdk::hypercore::ActionError`) has `.message() -> &str`,
`.ids() -> &[T]` and `.into_ids()`. It stores the cause **as a string**: you cannot downcast it
to `reqwest::Error`, so a timeout and a rejection look alike. Put a `Cloid` on orders you care
about so you can reconcile with `order_status(account, OidOrCloid::Right(cloid))` after an
ambiguous failure. `ActionError` implements `std::error::Error`, so `?` into `anyhow` works.

Downcasting an `anyhow::Error`, from `examples/hypercore/error_handling.rs` (needs `reqwest`
and `serde_json` in your Cargo.toml):

```rust
use std::str::FromStr;

use hypersdk::{
    Address,
    hypercore::{self, ApiError, NonceHandler, PrivateKeySigner, types::UsdSend},
};
use rust_decimal::dec;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let signer = PrivateKeySigner::from_str(&std::env::var("PRIVATE_KEY")?)?;
    let to: Address = std::env::var("TO")?.parse()?;
    let client = hypercore::testnet();
    let nonce = NonceHandler::default().next();

    let result = client
        .send_usdc(&signer, UsdSend { destination: to, amount: dec!(1), time: nonce }, nonce)
        .await;

    match result {
        Ok(()) => println!("Transfer succeeded"),
        Err(e) => {
            if let Some(err) = e.downcast_ref::<ApiError>() {
                println!("API rejected: {err}");
            } else if let Some(err) = e.downcast_ref::<reqwest::Error>() {
                if err.is_timeout() {
                    println!("Timed out, safe to retry");
                } else {
                    println!("Network error: {err}");
                }
            } else if let Some(err) = e.downcast_ref::<serde_json::Error>() {
                println!("Bad response JSON: {err}");
            } else {
                println!("Other error: {e}");
            }
        }
    }
    Ok(())
}
```

## Not in the crate — do not invent these

- No size/lot rounding helper (only price rounding).
- No `close_position`, bracket-order or TP/SL convenience method. Closing means a
  `reduce_only: true` order on the opposite side; triggers are built by hand with
  `OrderTypePlacement::Trigger` (orders.md). No example exercises triggers.
- No retries, rate limiting, or local order-book maintenance. The WebSocket reconnects, the HTTP
  client does not retry.
- No automatic nonces inside the client: every signed method takes `nonce: u64`.
- `schedule_cancel` always sends a time; the crate has no method to clear a scheduled cancel.
- No typed response for TWAP: `OkResponse` has no TWAP variant (see orders.md).
