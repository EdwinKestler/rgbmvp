# C4 closed — Simplicity time-locked staking

**Status: CLOSED** on Elements regtest (Simplicity active)  
**Date:** 2026-07-22  
**Pins:** [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) · ADR-C4 in [ROADMAP_NEXT.md](./ROADMAP_NEXT.md)

## What was proven

A **stake UTXO** locked under SimplicityHL (tapleaf **0xbe**, NUMS / no key path) can be spent by **anyone** after an absolute height, but consensus only accepts unstakes that:

| Rule | Mechanism |
|------|-----------|
| **Maturity** | `jet::check_lock_height(MATURE_HEIGHT)` — tx `nLockTime` height ≥ param |
| **Principal home** | `vout[0]` SPK hash = `STAKER_SPK_HASH` (committed at address derivation) |
| **Full principal** | Explicit asset + amount on `vout[0]` equals **current stake input** |
| **Fees separate** | Second P2WPKH input pays fee; principal is not reduced |

| Case | Result |
|------|--------|
| Early unstake (`height < mature`) | **Rejected** (locktime / non-final) |
| Mature unstake to staker | **Accepted**; principal returned |
| **wrong-dest** | Consensus reject |
| **wrong-amount** | Consensus reject |
| **early-lock** (nLockTime below mature after satisfy) | Consensus reject |

## Program

`crates/lab-simplicity/programs/stake_covenant.simf`  
(also `programs/simplicity/stake_covenant.simf`)

| Param | Role |
|-------|------|
| `MATURE_HEIGHT` | Absolute block height (type `Height`) |
| `STAKER_SPK_HASH` | SHA256 of staker `scriptPubKey` |
| `PRINCIPAL_ASSET` | Explicit asset id (LE / jet order) — demo uses L-BTC |

No witness values (keyless trigger). Fee key is only for the separate fee input.

## Live evidence (this lab, 2026-07-22)

| Item | Value |
|------|--------|
| Node | Elements **23.3.0** · `:7042` · `simplicity.active=true` |
| Mature height | 201 (demo run; height-dependent) |
| Unstake tx | `80e27a3a5a78602b474d6d056685a85b8da3673d9f74f12c2e2979d6adea6b58` |
| Principal | `0.00050000` L-BTC → staker SPK |

Re-run:

```bash
./scripts/regtest_simplicity.sh up
./scripts/demo_c4_stake.sh
# or: cargo run -p lab-cli -- covenant demo-c4
```

## CLI

```bash
# Stake address
./target/debug/lab-simp stake-address \
  --mature-height <H> --staker-spk <hex> --principal-asset <lbtc>

# Unstake raw hex
./target/debug/lab-simp stake-spend \
  --mature-height <H> --staker-spk <hex> --principal-asset <lbtc> \
  --stake-txid … --stake-vout … --stake-value-sat … \
  --fee-txid … --fee-vout … --fee-input-sat … \
  --lbtc-asset … --genesis-hash … \
  --tamper none|early-lock|wrong-dest|wrong-amount
```

## ADR-C4 (accepted)

| Topic | Decision |
|-------|----------|
| Time | Absolute block height (`check_lock_height`) |
| Principal | Forced to committed staker SPK hash; full input amount |
| Trigger | Anyone after maturity (no signature on stake input) |
| Fees | Separate input |
| Rewards / partial / rollover | **Out of MVP** |
| RGB | Optional later; this close is **seal-value only** |

## Explicitly out of C4

- Staking rewards / interest  
- Partial unstake or restake  
- Relative CSV-only maturity (absolute height is MVP)  
- RGB allocation on the stake seal  
- Public Liquid Testnet  

## Next

- **U4** public-hosting security before Internet bind  
- Optional: wire stake address into `/v1` or demo board chip  
