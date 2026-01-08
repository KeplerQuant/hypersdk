# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`hypersdk` is a Rust SDK for the Hyperliquid decentralized exchange protocol, providing type-safe interfaces for trading, market data, and DeFi integrations.

The codebase is split into two main modules:

- **`hypercore`**: Native L1 chain with perpetual/spot markets, HTTP API, WebSocket streams
- **`hyperevm`**: Ethereum-compatible layer with integrations for Morpho (lending) and Uniswap V3

## Commands

### Build and Test

```bash
# Build the project
cargo build

# Run all tests (unit tests + doctests)
cargo test

# Run only unit tests
cargo test --lib

# Run only doctests
cargo test --doc

# Run a specific test
cargo test test_name

# Check code without building
cargo check
```

### Running Examples

Examples require a private key set via environment variable:

```bash
export PRIVATE_KEY="your_private_key_here"

# HyperCore examples
cargo run --example list_markets
cargo run --example list_tokens
cargo run --example send_order
cargo run --example websocket
cargo run --example transfer_to_evm

# Morpho examples
cargo run --example morpho_highest_apy
cargo run --example morpho_supply_apy

# Uniswap examples
cargo run --example uniswap_pools_created
```

## Architecture

### Signing Architecture

Hyperliquid uses **two distinct signing methods** depending on the action type:

1. **RMP (MessagePack) Signing** - Used for trading actions (orders, cancels, modifications)
   - Action is serialized to MessagePack format
   - Nonce, vault address, and expiry are appended to bytes
   - Keccak256 hash is computed
   - Hash is wrapped in an Agent struct and signed via EIP-712
   - Implementation: `signing::sign_rmp()` in `src/hypercore/signing.rs`

2. **EIP-712 Typed Data Signing** - Used for asset transfers (UsdSend, SpotSend, SendAsset)
   - Action is converted to structured EIP-712 TypedData
   - Signed directly using EIP-712 (more wallet-friendly)
   - Implementation: `signing::sign_eip712()` in `src/hypercore/signing.rs`

The `Signable` trait (`src/hypercore/signing.rs`) provides a unified interface - each action type implements it by calling the appropriate signing method.

### Module Structure

#### `src/hypercore/` - HyperCore L1 Module

- `mod.rs`: Chain configuration, market types (PerpMarket, SpotMarket, SpotToken), price tick calculation
- `http.rs`: HTTP client for API operations (place orders, query balances, transfers)
- `ws.rs`: WebSocket connection for real-time market data and user events
- `types.rs`: All request/response types, order types, WebSocket message types
- `signing.rs`: Signable trait and signing utilities (RMP and EIP-712)
- `utils.rs`: Utility functions for serialization helpers, typed data creation, hashing

#### `src/hyperevm/` - HyperEVM Module

- `mod.rs`: Base EVM functionality, wei conversion utilities
- `morpho/`: Morpho lending protocol integration (APY queries, vault data)
- `uniswap/`: Uniswap V3 integration (pool queries, position tracking)

### Price Tick Rounding

Hyperliquid enforces strict tick size requirements. The SDK provides O(1) tick size calculation:

- **Perpetual markets**: 5 significant figures, max 6 decimal places (6 - sz_decimals)
- **Spot markets**: Max 8 decimal places (8 - sz_decimals)

Algorithm: `decimals = clamp(5 - floor(log10(price)) - 1, 0, max_decimals)`

Implementation in `src/hypercore/mod.rs`:

- `PriceTick` struct with `tick_for()`, `round()`, `round_by_side()` methods
- Used by `PerpMarket` and `SpotMarket` to validate order prices

### Type Organization

**Public types** (exposed in public API) must appear BEFORE the `// PRIVATE TYPES` marker in `src/hypercore/types.rs` (around line 1198). This keeps the module's public interface clearly separated from internal implementation details.

**Private types** (`pub(super)`) like `ActionRequest`, `ApiResponse`, `Signature` appear after the marker - these are internal to the hypercore module.

### Cross-Chain Transfers

Assets can be transferred between three contexts:

- Perpetual trading balance (HyperCore perps)
- Spot trading balance (HyperCore spot)
- EVM balance (HyperEVM)

Transfer addresses are generated algorithmically: `0x20000000000000000000000000000000000000XX` where XX is the token index. See `generate_evm_transfer_address()` in `src/hypercore/mod.rs`.

The HTTP client provides convenience methods:

- `transfer_to_evm()` / `transfer_from_evm()`
- `transfer_to_perps()` / `transfer_to_spot()`

### WebSocket Message Flow

WebSocket connection (`src/hypercore/ws.rs`) uses yawc for the underlying connection. The flow:

1. Create connection with `WebSocket::new(url)`
2. Subscribe to channels via `subscribe(Subscription)`
3. Receive messages as `Incoming` enum via Stream implementation
4. Messages are automatically deserialized from JSON

Available subscription types are in `types::Subscription` enum - each maps to a specific Hyperliquid WebSocket channel.

## Development Guidelines

### Adding New Action Types

When adding a new signable action:

1. Define the action struct in `src/hypercore/types.rs` (before the PRIVATE TYPES marker if public)
2. Add it to the `Action` enum
3. Implement `Signable` trait in `src/hypercore/signing.rs`:
   - Use `sign_rmp()` for trading actions
   - Use `sign_eip712()` for transfers (requires TypedData implementation)
4. Add the HTTP client method in `src/hypercore/http.rs`
5. Add corresponding solidity struct in `types::solidity` module if using EIP-712

### Documentation

- All public types and methods must have rustdoc comments
- Use `#[must_use]` for methods returning values that should not be ignored
- Examples in doc comments should be working code or removed entirely (no `no_run` blocks)
- Run `cargo test --doc` to verify documentation examples compile

### Testing

- Unit tests go in the same file with `#[cfg(test)] mod tests`
- Integration tests requiring network access use `#[tokio::test]`
- Price tick rounding has comprehensive tests in `src/hypercore/mod.rs` (see `tick_tests` module)

## Important Constants

- `ARBITRUM_MAINNET_CHAIN_ID = "0xa4b1"` - Arbitrum One chain ID for EIP-712 signatures
- `ARBITRUM_TESTNET_CHAIN_ID = "0x66eee"` - Hyperliquid testnet chain ID
- `USDC_CONTRACT_IN_EVM` - USDC contract address on HyperEVM (differs from docs)

Chain-specific constants are accessed via `Chain::arbitrum_id()` method.

## Common Patterns

### Creating an HTTP Client

```rust
let client = hypercore::mainnet();  // or testnet()
let signer: PrivateKeySigner = private_key.parse()?;
```

### Placing Orders

All orders use the `BatchOrder` type even for single orders. Orders require:

- Rounded prices (use `market.round_price()`)
- Valid cloid (client order ID, defaults to zero)
- Nonce (typically current timestamp in milliseconds)

### Handling API Responses

API responses are wrapped in `ApiResponse` enum with `Ok(OkResponse)` or `Err(String)`. The HTTP client unwraps this automatically.

### Multi-signature Operations

Multisig uses a special flow in `src/hypercore/http.rs`:

1. Create `MultiSig` context via `client.multi_sig(lead, multisig_address, nonce)`
2. Add signers with `.signer()` or `.signers()`
3. Optionally append pre-existing signatures with `.signatures()`
4. Call action method (e.g., `.place()`, `.send_usdc()`, `.send_asset()`)
5. Collects signatures from all signers and wraps in `MultiSigAction`

The `signatures()` method allows appending pre-collected signatures, useful for:

- Offline signature collection workflows
- Aggregating signatures from multiple sources
- Adding signatures to partial multisig transactions
