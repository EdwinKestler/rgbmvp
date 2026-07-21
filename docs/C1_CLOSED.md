# C1 closed — Simplicity mint-gate (vault lock + gate recursion)

**Status: CLOSED** on Elements regtest (Simplicity active)  
**Date:** 2026-07-21  
**Pins:** [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) · prior [C0_CLOSED.md](./C0_CLOSED.md)

## What was proven

The **gate seal** UTXO is locked under a SimplicityHL program (tapleaf **0xbe**) with **no signature key**. Anyone may spend it, but consensus only accepts a transaction shaped like a backed mint:

| Output | Rule |
|--------|------|
| **vout[0]** | RGB **opret** shape: `OP_RETURN OP_PUSHBYTES_32 <payload>` |
| **vout[1]** | Exactly `TRANCHE` of `BACKING_ASSET` locked to vault SPK (explicit asset+amount) |
| **vout[3]** | Same covenant script again (**recursion** for the next minter) |

vout[2] (recipient seal) is unconstrained by the chain (RGB contents are client-side).

| Case | Result |
|------|--------|
| Round 1 mint | Accepted; vault holds tranche; vout3 = gate |
| Round 2 mint | Spends **recreated** gate; accepted again |
| **drop-anchor** | Consensus reject |
| **wrong-amount** | Consensus reject (short vault tranche) |
| **no-recreate** | Consensus reject (vout3 not gate) |

## Live evidence (this lab, 2026-07-21)

| Item | Value |
|------|--------|
| Node | Elements **23.3.0** · `:7042` · `simplicity.active=true` |
| Example mint #1 | `ee72a1df42fbee64ce5abe5cd8e00050f1eeb91f02f444455d9fe3159abf835d` |
| Example mint #2 | `e88a03306245c48fb45e81701648070b6abd5edff972a1827571e0818adad710` |
| Vault lock | `0.00250000` units (= 250_000 asset-sats) per mint |

Re-run:

```bash
./scripts/regtest_simplicity.sh up
./scripts/demo_c1_mint_gate.sh
# or: cargo run -p lab-cli -- covenant demo-c1
```

## How to use

```bash
# Gate address (params baked into CMR)
./target/debug/lab-simp address \
  --program crates/lab-simplicity/programs/mint_gate_covenant.simf \
  --vault-spk <hex> --backing-asset <display_hex> --tranche 250000

# Build mint spend (raw hex)
./target/debug/lab-simp mint-spend ... --tamper none|drop-anchor|wrong-amount|no-recreate
```

**Note:** `BACKING_ASSET` param uses **byte-reversed** display hex (jet / consensus order). `lab-simp` does this when you pass `--backing-asset`.

## Scope notes

- **In C1:** chain container — anchor shape, vault asset/amount, gate recursion.  
- **Out of C1:** full RGB IFA issue/mint client-side validation (pair later with BFA/C3).  
- Random 32-byte “MPC roots” stand in for real RGB anchors in the demo.

## Next

- C2 burn mint-gate variant (optional)  
- C3 BFA schema + audit  
- Optional `/v1/covenant/*`  
