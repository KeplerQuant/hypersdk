# Sync instructions for hyperliquid-rust-sdk skill

This skill documents the hypersdk crate's public Rust API for a trading bot
that imports it as a dependency. It is separate from skills/hyperliquid-trading
and skills/hyperliquid-payments, which document the hypecli CLI binary — ignore
those as a model, they're not relevant here.

## Scope of this sync

Only update what the diff actually touches. Do not rewrite sections whose
underlying code didn't change — this skill was hand-verified once (32 code
blocks compile-checked against the crate), and unnecessary rewrites throw
that verification away silently.

## Steps

1. Read CLAUDE.md in the repo root if it changed, for any new conventions.
2. Read Cargo.toml. If the version bumped, update the Cargo.toml snippet
   in SKILL.md's setup section to match.
3. Walk the diffed files in src/. For each changed `pub fn`, `pub struct`,
   `pub enum`, `pub trait`, `pub use`:
   - find where it's documented in SKILL.md or references/*.md
   - if the signature, parameters, return type, or error variants changed,
     update that section only
   - if it's a new public item, decide whether it belongs in scope for a
     trading bot (orders, positions, markets, websocket, transfers) or in
     the excluded list at the bottom of SKILL.md (EVM DeFi, staking, vault
     admin, asset deployment, outcome markets, low-level signing, niche
     account settings, read-only queries with little bot use) — follow the
     same categorization already used there
4. Check examples/ and tests/ for changes. If a code block currently in the
   skill was sourced from a file that changed, re-verify it still compiles
   as written. If an example was added that covers something currently
   listed as "no example combines X" (see recipes.md), consider adding that
   recipe now that it exists.
5. Doc comments (///) in src/ are not trustworthy by default — this skill
   already flags several that don't match reality (Client::mainnet(),
   ARBITRUM_SIGNATURE_CHAIN_ID, client.spot_meta(), the WebData2 subscription
   row). If the diff touches one of these specific known-wrong doc comments
   and the underlying code now matches the doc, remove that caveat from the
   skill. If the diff introduces a new mismatch between a doc comment and
   examples/tests, trust examples/tests and flag the new mismatch the same
   way the existing ones are flagged.
6. Check open caveats against the diff:
   - twap_order / twap_cancel: flagged as possibly returning a JSON-parse
     Err even on exchange acceptance, needs testnet verification. If the
     diff touches the response type or these functions, re-check whether
     this is resolved.
   - vault_address = master for subaccount signers in
     examples/hypercore/send_order.rs contradicts the subaccounts() doc.
     If either side of that contradiction changed, re-resolve it.
   - Posting a signed order over the WebSocket is marked compile-checked
     only, no example/test uses it. If an example now covers this, verify
     it against that example and remove the caveat.

## Verification

Extract every ```rust code block in SKILL.md and references/*.md that has
a `fn main`. Check they compile against the current crate version:

    cargo check --ignore-rust-version

(the crate requires a newer Rust edition than may be installed on the
runner; --ignore-rust-version matches how this was verified originally —
note in the PR description if this flag was needed and why)

## Rules

- Never invent function names, parameters, or error variants. Verify every
  code block against src/, don't paraphrase from memory or from doc comments.
- If something is genuinely unclear from the diff, mark it "needs
  verification" rather than guessing.
- Keep the SKILL.md + references/ split as-is. If a single reference file
  would exceed roughly 500 lines after your edit, split further rather than
  letting it grow unbounded.
- If nothing in the skill's actual scope changed (e.g. the diff only touched
  excluded modules like hyperevm or staking), say so in the PR description
  and don't touch the skill files.

## PR description requirements

List:
- which files were changed and why
- any caveat that was added, removed, or resolved
- any new public function that was categorized as excluded, and which
  category it was put in
- confirmation that cargo check passed on all extracted code blocks, or
  which ones failed and why