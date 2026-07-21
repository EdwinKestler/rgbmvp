# C0 closed — Simplicity `preimage ∧ opret` anchor covenant

**Status: CLOSED** on Elements regtest (Simplicity active)  
**Date:** 2026-07-21  
**Pins:** [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) · plan [P2_PLAN.md](./P2_PLAN.md)

## What was proven

A seal UTXO locked under a SimplicityHL program (tapleaf **0xbe**) can be spent **only** if:

1. The spender reveals `preimage` with `SHA256(preimage) == EXPECTED_HASH` (baked into CMR/address), **and**
2. Output 0 is exactly RGB **opret-shaped**: `OP_RETURN OP_PUSHBYTES_32 <32B payload>`.

| Case | Result |
|------|--------|
| **A** Wrong preimage | Program **fails to satisfy** (tooling / off-chain) |
| **B** Strip opret after satisfaction | **Consensus rejects** (`non-mandatory-script-verify-flag (Assertion failed inside jet)`) |
| **C** Compliant spend | **Accepted**; vout0 = `6a20\|\|<MPC root>` |

## Live evidence (this lab, 2026-07-21)

| Item | Value |
|------|--------|
| Node | `rgbmvp-elementsd-simplicity` Elements **23.3.0** · RPC `:7042` |
| Deployment | `getdeploymentinfo.deployments.simplicity.active = true` |
| Genesis | `209577bda6bf4b5804bd46f8621580dd6d4e8bfa2d190e1c50e932492baca07d` |
| L-BTC asset | `b2e15d0d7a0c94e4e2ce0fe6e8691b9e451377f6e46e8045a86f7c4b5d4f0f23` |
| Example CMR | (run-dependent; hashlock changes address) |
| Example spend txid | `8d40259bf89a034e5e1278f7da085f5d6d2a109972aabf74d21cac6044cdbf37` |
| Strip-anchor reject | error **-26** · *Assertion failed inside jet* |

Re-run:

```bash
./scripts/regtest_simplicity.sh up
./scripts/demo_c0_simplicity.sh
# or: cargo run -p lab-cli -- covenant demo
```

## How to use

```bash
# Address for a given hashlock
cargo run -p lab-simplicity --bin lab-simp -- address --hash <sha256_hex>
# same via lab CLI:
cargo run -p lab-cli -- covenant address --hash <sha256_hex>

# Full regtest proof (A/B/C)
./scripts/demo_c0_simplicity.sh
```

Program source: `programs/simplicity/rgb_anchor_covenant.simf`  
(crate copy: `crates/lab-simplicity/programs/rgb_anchor_covenant.simf`)

## Explicitly out of C0

- Tapret-shaped covenant (opret only for MVP)  
- Full HTLC (claimer sig + CSV) inside Simplicity  
- Mint-gate / BFA / staking (C1–C4)  
- Public Liquid Testnet (regtest is source of truth)  
- `/v1/covenant/*` HTTP (next polish)

## Next

- C1 mint-gate (lock vault)  
- Optional: expose covenant status on demo board  
- C3 BFA audit when IFA schemas land  
