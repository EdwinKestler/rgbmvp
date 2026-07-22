# P2 closed — programmable seals & backed assets (lab)

**Status: CLOSED** for the public-lab definition of done  
**Date:** 2026-07-21  

Per [SCENARIOS.md](./SCENARIOS.md) / [P2_PLAN.md](./P2_PLAN.md):  
**C0 + C3 green on CI/regtest** (optional public testnet deferred).

## In scope (done)

| ID | Capability | Evidence |
|----|------------|----------|
| **R0** | Elements 23.3 Simplicity regtest + pins | [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) |
| **C0** | `preimage ∧ opret` seal covenant; strip-anchor consensus reject | [C0_CLOSED.md](./C0_CLOSED.md) |
| **C1** | Mint-gate vault lock + gate recursion | [C1_CLOSED.md](./C1_CLOSED.md) (stretch, delivered) |
| **C3** | BFA schema + full-history audit (no oracle) | [C3_CLOSED.md](./C3_CLOSED.md) |

## Re-run ladder proofs

```bash
./scripts/regtest_simplicity.sh up
./scripts/demo_c0_simplicity.sh
./scripts/demo_c1_mint_gate.sh   # optional extra
./scripts/demo_c3_bfa_audit.sh
```

## Explicitly deferred

| Item | Note |
|------|------|
| C2 burn mint-gate | **CLOSED** — [C2_CLOSED.md](./C2_CLOSED.md) |
| C4 staking covenant | Stretch |
| C5 LiquiDEX comparison | Docs only |
| Public Liquid Testnet Simplicity/BFA | Feature probe later |
| `/v1/audit/*` browser page | Thin polish on existing JSON audit |
| Full RGB IFA wallet UX | P3 |

## Product narrative (complete)

1. **P0** — RGB lives on Liquid (issue / transfer / verify).  
2. **P1** — Twin HTLC swap without custodian.  
3. **P2** — Chain-enforced seal policy (Simplicity) **and** oracle-free backed-asset audit (BFA).  

Next track historically: **P3** (done). Protocol extension: **C4** staking · **U4** security ([ROADMAP_NEXT.md](./ROADMAP_NEXT.md)).
