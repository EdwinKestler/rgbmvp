# C5 — LiquiDEX / native Liquid swap vs RGB twin HTLC

**Status:** Partial — comparison skeleton (not a finished ecosystem writeup)  
**Scenario:** [SCENARIOS.md](./SCENARIOS.md) `C5`  
**Mainnet:** out of scope in this repository  
**Roadmap:** [ROADMAP_NEXT.md](./ROADMAP_NEXT.md) (low engineering urgency)

## Purpose

Position **rgbmvp** relative to Liquid-native collaborative exchange tooling
(e.g. PSET / LiquiDEX-style flows) without conflating asset models.

## Hard rules (wording)

| Do not say | Say instead |
|------------|-------------|
| One RGB contract “moves” between BTC and Liquid | Two **chain-bound** twin contracts (`rgb:` ids) swap via linked HTLCs |
| Native Liquid issued asset **is** RGB | Native Elements asset id ≠ RGB contract id |
| RGB commitment = Elements consensus alone | RGB **client-side** validation + chain anchors |

## Comparison dimensions

| Dimension | Liquid native / PSET swap | RGB twin HTLC swap (rgbmvp S2–S3) |
|-----------|---------------------------|-----------------------------------|
| Asset identity | Elements asset ID | Separate chain-bound `rgb:` IDs |
| Validation | Elements consensus | RGB client validation + anchors |
| Construction | Collaborative PSET | Dual HTLC + consignments |
| Atomicity | One Liquid tx / PSET | Two chains linked by preimage / CSV |
| Privacy | Confidential Transactions | CT + private RGB history |
| Failure path | Incomplete PSET not broadcast | CSV refunds per leg |
| Wallet tooling | LWK / LiquiDEX-compatible | lab-rgb, HTLC, consignment tooling |
| Custody | Non-custodial if parties sign | Non-custodial if both HTLC legs correct |
| Cross-chain claim | No | Yes, via **twins** — not teleportation |
| Mainnet | Out of scope here | Out of scope here |

## What rgbmvp demonstrates

| Claim | Where |
|-------|--------|
| Value HTLC twin swap (P1) | [P1_CLOSED.md](./P1_CLOSED.md) |
| RGB-wrapped claim (S3) | [S3_RGB_WRAP.md](./S3_RGB_WRAP.md) |
| Round-trip (S5) | Deferred — [ROADMAP_NEXT.md](./ROADMAP_NEXT.md) |

## Ecosystem (not lab-run)

LiquiDEX / Liquid DEX UX, production market makers, and mainnet PSET flows are
**ecosystem descriptions**. Cite primary protocol docs when expanding this file;
do not invent lab commands that imply the monorepo runs a LiquiDEX node.

## Acceptance for C5 doc completion

- [x] No native Liquid asset described as RGB  
- [x] No “one contract id moves chains” language  
- [x] PSET atomicity vs dual-HTLC twin atomicity distinguished  
- [ ] Links to runnable lab scenarios kept current as S5 lands  
- [ ] Optional: short reproduction of a **value-only** twin swap for contrast  

## Related

- [STACK.md](./STACK.md) — LWK vs RGB vs CLN  
- [WALLETS.md](./WALLETS.md)  
- [SCENARIOS.md](./SCENARIOS.md) C5 row  
