# C2 closed — Simplicity mint-gate **burn** variant

**Status: CLOSED** on Elements regtest (Simplicity active)  
**Date:** 2026-07-22  
**Pins:** [C1_CLOSED.md](./C1_CLOSED.md) · ADR-C2 in [ROADMAP_NEXT.md](./ROADMAP_NEXT.md)

## What was proven

Same **mint-gate** program as C1 (`mint_gate_covenant.simf`, leaf **0xbe**, no key), but the baked `VAULT_SPK_HASH` is **`SHA256(empty script)`**. Anyone may spend the gate only if the mint container burns backing instead of locking it to a vault:

| Output | Rule |
|--------|------|
| **vout[0]** | RGB **opret** shape: `OP_RETURN OP_PUSHBYTES_32 <payload>` |
| **vout[1]** | Exactly `TRANCHE` of `BACKING_ASSET` to **empty SPK** (provably unspendable burn) |
| **vout[3]** | Same covenant script again (**recursion** for the next minter) |

vout[2] (recipient seal) remains unconstrained by the chain.

| Case | Result |
|------|--------|
| Round 1 burn-mint | Accepted; tranche destroyed on empty SPK; vout3 = gate |
| Round 2 burn-mint | Spends **recreated** gate; accepted again |
| **drop-anchor** | Consensus reject |
| **wrong-amount** | Consensus reject (short burn) |
| **no-recreate** | Consensus reject (vout3 not gate) |
| **not-burn** | Consensus reject (vout1 paid to spendable SPK) |

## Why reuse C1 program

ADR-C2: separate dual-role OP_RETURN only if jets can enforce burn+anchor together.  
Here we keep **separate outputs** (anchor + burn) and change only the **hash param** so C1 tooling (`lab-simp mint-spend --burn`) applies unchanged.

| Mode | `VAULT_SPK_HASH` | Economics |
|------|------------------|-----------|
| **C1 lock** | SHA256(vault SPK) | Backing accumulates in vault |
| **C2 burn** | SHA256(∅) = `e3b0c442…7852b855` | Backing destroyed each mint |

Gate **CMR/address differ** between lock and burn for the same asset/tranche.

## Live evidence (this lab, 2026-07-22)

| Item | Value |
|------|--------|
| Node | Elements **23.3.0** · `:7042` · `simplicity.active=true` |
| Example mint #1 | `380df01e411edeabee60d5a46c90a67598c5f7998e633fd106863491fc98f433` |
| Example mint #2 | `aa7c7dc28497b668dcc879910666f1b1a72b7b09d274771b9861d7cffd706f49` |
| Burn per mint | `0.00250000` units (= 250_000 asset-sats) to empty SPK |

Re-run:

```bash
./scripts/regtest_simplicity.sh up
./scripts/demo_c2_mint_gate_burn.sh
# or: cargo run -p lab-cli -- covenant demo-c2
```

## How to use

```bash
# Burn-gate address (empty SPK hash baked into CMR)
./target/debug/lab-simp address \
  --program crates/lab-simplicity/programs/mint_gate_covenant.simf \
  --burn --backing-asset <display_hex> --tranche 250000

# Build burn mint spend
./target/debug/lab-simp mint-spend --burn \
  --backing-asset … --tranche 250000 \
  --tamper none|drop-anchor|wrong-amount|no-recreate|not-burn
```

## BFA coupling

Genesis terms may commit burn mode so audits cannot silently treat a burn history as vault lock:

```text
elements-backing:v1;vault=;asset=<id>;rate=<n>/<d>;mode=burn
```

- `mode=burn` **requires** empty `vault` hex.  
- Default / omitted `mode` remains **lock** (backward compatible with C3 histories).  
- Client audit still uses `verify_backing` against the committed SPK (empty ⇒ sum burns).

## Scope notes

- **In C2:** chain container — opret shape, exact burn amount/asset, gate recursion, not-burn reject.  
- **Out of C2:** full RGB IFA mint contents; public Liquid Testnet; dual-role single OP_RETURN.  
- Random 32-byte “MPC roots” stand in for real RGB anchors in the demo.

## Next

- C4 staking research freeze  
- U4 before any public Internet bind  
- Optional: link real BFA mint plans to C2 burns in one demo script  
