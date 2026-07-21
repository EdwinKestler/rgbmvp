# SimplicityHL programs (P2)

| File | Scenario | Role |
|------|----------|------|
| `rgb_anchor_covenant.simf` | **C0** | `preimage(H) ∧ opret-shaped output 0` |
| `mint_gate_covenant.simf` | **C1** | opret + vault tranche + gate recursion |

Further programs (burn, staking) land with C2+; do not invent semantics here.

**Toolchain pins and ADR:** [`docs/P2_SIMPLICITY.md`](../../docs/P2_SIMPLICITY.md)

**Provenance:** C0 program adapted from [kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike) (`spike-simplicity`).
