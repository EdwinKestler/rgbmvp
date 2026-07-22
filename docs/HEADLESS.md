# Headless protocol kit (inside monorepo)

P2-class work does **not** require the browser UI. Use this entry when you only
need consensus demos and libraries.

## Crates

| Crate | Role |
|-------|------|
| `lab-rgb` | NIA issue/transfer/verify · **BFA** issue/mint/audit · HTLC swap helpers |
| `lab-simplicity` | C0–C2 + C4 stake (`lab-simp` binary) |
| `lab-chain` / `lab-btc` | Liquid LWK + Bitcoin testnet legs |
| `vendor/rgb-consensus-patched` | `WitnessTx` patch |

## Demos (regtest / testnet)

```bash
# P2 Simplicity + BFA (Elements regtest)
./scripts/regtest_simplicity.sh up
./scripts/demo_c0_simplicity.sh
./scripts/demo_c1_mint_gate.sh
./scripts/demo_c2_mint_gate_burn.sh
./scripts/demo_c4_stake.sh
./scripts/demo_c3_bfa_audit.sh

# P0/P1 still use Liquid/BTC public testnets via CLI — see README
```

## Docs

| Doc | Content |
|-----|---------|
| [P2_CLOSED.md](./P2_CLOSED.md) | Phase closure |
| [C0_CLOSED.md](./C0_CLOSED.md) · [C1_CLOSED.md](./C1_CLOSED.md) · [C3_CLOSED.md](./C3_CLOSED.md) | Proofs |
| [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) | Pins / ADR |

## Product lab vs kit

| Concern | Where |
|---------|--------|
| Protocol proofs, crates, demos | This file + crates/scripts |
| Browser console, wizards | [P3_PLAN.md](./P3_PLAN.md) · `web/` · `rgbmvp serve` |

A separate public kit repo is **deferred** until external consumers need a frozen extract.
