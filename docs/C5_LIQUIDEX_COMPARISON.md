# C5 — LiquiDEX / native Liquid swap vs RGB twin HTLC

**Status:** Complete — documentation positioning only (2026-07-27)
**Scenario:** [SCENARIOS.md](./SCENARIOS.md) `C5`  
**Mainnet:** out of scope in this repository  
**Roadmap:** [ROADMAP_NEXT.md](./ROADMAP_NEXT.md) (low engineering urgency)

## Purpose

Position **rgbmvp** relative to Liquid-native collaborative exchange tooling
without conflating asset models or claiming a LiquiDEX implementation.

Two related mechanisms must stay distinct:

- A conventional Liquid asset swap can be collaboratively constructed as a
  **PSET** and settle both sides in one Liquid transaction.
- **LiquiDEX** is a specific two-message maker/taker proposal protocol. Its
  original publication uses `SIGHASH_SINGLE | SIGHASH_ANYONECANPAY` so the
  taker can complete and broadcast one Liquid transaction; it also labels the
  implementation as not production-ready.

## Hard rules (wording)

| Do not say | Say instead |
|------------|-------------|
| One RGB contract “moves” between BTC and Liquid | Two **chain-bound** twin contracts (`rgb:` ids) swap via linked HTLCs |
| Native Liquid issued asset **is** RGB | Native Elements asset id ≠ RGB contract id |
| RGB commitment = Elements consensus alone | RGB **client-side** validation + chain anchors |

## Comparison dimensions

| Dimension | Liquid native swap (PSET / LiquiDEX-style) | RGB twin HTLC swap (rgbmvp S2–S3) |
|-----------|----------------------------------------------|-----------------------------------|
| Asset identity | Elements asset ID | Separate chain-bound `rgb:` IDs |
| Validation | Elements consensus, signatures, and balance rules | RGB client validation plus Bitcoin/Liquid anchors |
| Construction | Collaborative PSET, or a LiquiDEX proposal completed by a taker | Two funded HTLC legs plus RGB consignments |
| Settlement | One transaction on Liquid | One claim per chain, linked by a shared preimage |
| Atomicity boundary | All exchanged Liquid inputs/outputs settle together or the transaction is not valid | Cross-chain safety depends on hashlock symmetry, staggered CSV refunds, and correct execution |
| Privacy | Confidential Transactions hide supported asset/amount details from public observers | Liquid CT plus selectively disclosed RGB history; Bitcoin-leg metadata remains public |
| Abort / failure path | An incomplete or invalid collaboration is not broadcast; a stale proposal can become unspendable if its input is spent | Each funded leg has an explicit post-timeout refund path |
| Wallet tooling | Elements/LWK PSET tooling; LiquiDEX needs proposal-specific validation | `lab-rgb`, HTLC, consignment, and `/v1` lab tooling |
| Custody | Can be non-custodial when each party verifies and signs its required exchange | Non-custodial under the documented HTLC assumptions |
| Cross-chain claim | No; assets remain on Liquid | Yes, via separate **twins** — never one contract moving chains |
| Mainnet | Out of scope here | Out of scope here |

This is a comparison of trust and settlement shapes, not a ranking. A
single-chain Liquid swap is simpler when both assets are native Liquid assets.
The rgbmvp path exists to demonstrate client-validated RGB twins across Bitcoin
testnet and Liquid Testnet, which requires a different failure and refund model.

## What rgbmvp demonstrates

| Claim | Where |
|-------|--------|
| Value HTLC twin swap (P1) | [P1_CLOSED.md](./P1_CLOSED.md) |
| RGB-wrapped claim (S3) | [S3_RGB_WRAP.md](./S3_RGB_WRAP.md) |
| Round-trip (S5) | Deferred to a post-freeze extension milestone — [ROADMAP_NEXT.md](./ROADMAP_NEXT.md) |

## Ecosystem (not lab-run)

LiquiDEX integrations, exchange UX, market makers, and production trading are
**ecosystem descriptions**. This repository does not run a LiquiDEX service,
does not reproduce the proposal protocol, and does not claim production
readiness from the comparison.

## Primary references

- [Liquid documentation: swaps and smart contracts](https://docs.liquid.net/docs/swaps-and-smart-contracts)
  — collaborative PSET swap and single-transaction settlement.
- [Liquid documentation: how Liquid works](https://docs.liquid.net/docs/how-liquid-works)
  — PSET lifecycle and issued-asset model.
- [Blockstream: LiquiDEX two-step atomic swaps](https://blog.blockstream.com/liquidex-2-step-atomic-swaps-on-the-liquid-network/)
  — proposal format, signature mode, maker/taker flow, trade-offs, and the
  original non-production-ready warning.
- [Blockstream: LWK](https://blog.blockstream.com/lwk-liquid-wallett-kit/)
  — wallet toolkit context and stated LiquiDEX support direction.

## Acceptance for C5 doc completion

- [x] No native Liquid asset described as RGB  
- [x] No “one contract id moves chains” language  
- [x] PSET atomicity vs dual-HTLC twin atomicity distinguished  
- [x] Generic PSET swaps distinguished from the specific LiquiDEX protocol
- [x] Runnable lab evidence linked for P1 and S3
- [x] S5 explicitly deferred and non-blocking

A value-only reproduction is already covered by P1 evidence; no additional
LiquiDEX command or protocol implementation is required to close this docs-only
scenario.

## Related

- [STACK.md](./STACK.md) — LWK vs RGB vs CLN  
- [WALLETS.md](./WALLETS.md)  
- [SCENARIOS.md](./SCENARIOS.md) C5 row  
