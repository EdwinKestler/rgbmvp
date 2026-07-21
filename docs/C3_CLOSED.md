# C3 closed — BFA schema + full-history audit (no oracle)

**Status: CLOSED** on Elements regtest  
**Date:** 2026-07-21  
**Depends on:** [C0_CLOSED.md](./C0_CLOSED.md) · [C1_CLOSED.md](./C1_CLOSED.md) (optional chain)  
**P2 lab closed:** with C0 — see [P2_CLOSED.md](./P2_CLOSED.md)

## Thesis

The chain cannot see RGB mint amounts. An under-backed mint can still **confirm**.  
BFA puts **backing terms in genesis** (vault SPK, asset, rate) so they are part of the  
**contract id**. Holders rebuild history and check every mint:

1. **Seal** — witness spends the claimed gate  
2. **Anchor** — witness pays the tapret commitment of the rebuilt transition  
3. **Backing** — vault locked ≥ `minted × rate` of the committed asset  

No oracle. Lying about mint size fails the anchor. Under-backing fails the vault rule.

## Schema

`BackedFungibleAsset` = IFA + mandatory genesis global `backingTerms`  
(`elements-backing:v1;vault=<spk>;asset=<id>;rate=<n>/<d>`).

## Proof points (demo)

| Case | Result |
|------|--------|
| Two honest chained mints | **audit OK** (seal + anchor + backing) |
| Over-mint chain accepts (40k mint / 10k lock) | **audit FAILS** (backing) |
| History lies (claims 10k mint on 40k anchor) | **audit FAILS** (anchor) |

## How to run

```bash
./scripts/regtest_simplicity.sh up
./scripts/demo_c3_bfa_audit.sh
# or: cargo run -p lab-cli -- bfa demo
```

CLI:

```bash
rgbmvp bfa issue --gate-seal txid:vout --backing 'elements-backing:v1;…'
rgbmvp bfa mint-plan --genesis-gate … --mint 30000 …
rgbmvp bfa audit --history path/to/history.json
```

## Code

| Module | Role |
|--------|------|
| `crates/lab-rgb/src/bfa.rs` | Schema, issue, mint, `audit_history` |
| `crates/lab-rgb/src/mint.rs` | `build_mint_with`, `verify_backing` |
| `scripts/demo_c3_bfa_audit.sh` | End-to-end regtest proof |

## Explicitly out of C3

- Public Liquid Testnet BFA (regtest is source of truth)  
- Full Simplicity mint-gate on the same seal (C1 is separate container demo)  
- Browser audit UI polish (CLI + JSON sufficient for ladder close)  
