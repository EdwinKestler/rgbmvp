# Roadmap next — protocol completeness first (localhost / testnet)

**Strategy (locked 2026-07-22):**  
**Deepen protocol on testnet + localhost first.** Public Internet demo only after **U4** security gate.

Historical phase closures (P1–P3) remain valid evidence of what was proven. This roadmap is **extension work**, not a rewrite of those claims.

| Priority | Track | When |
|----------|--------|------|
| 0 | Doc honesty + ADRs | Now |
| 1 | **S3** RGB-wrapped claim (CLI + live proof) | Done — [S3_RGB_WRAP.md](./S3_RGB_WRAP.md) |
| 2 | **C2** burn mint-gate (regtest) | **Done** — [C2_CLOSED.md](./C2_CLOSED.md) |
| 3 | **C4** staking (regtest) | **Done** — [C4_CLOSED.md](./C4_CLOSED.md) |
| 4 | **U4** public-hosting security foundation | **Implemented** — [U4_PUBLIC_HOSTING.md](./U4_PUBLIC_HOSTING.md); deploy still operator-run |
| 5 | Independent review + public **read-only** demo | After U4 soak + deploy |

```text
Localhost / public testnet (operator)
   │
   ├─► S3 RGB-wrapped claim     ◄── done (CLI + live)
   ├─► C2 burn mint-gate        ◄── done (regtest)
   ├─► C4 stake                 ◄── done (regtest)
   │
   └─► U4 security (code done; deploy optional)
              │
              ▼
         Internet demo (read-only; Vercel + Cloud Run sketches)
```

---

## Phase 0 — Extension contract (docs)

### Claim reconciliation

| ID | Correct status |
|----|----------------|
| **S2** | Script HTLC fund/claim/refund **live** (value path) |
| **S3** | **CLI implemented** — see [S3_RGB_WRAP.md](./S3_RGB_WRAP.md). Live testnet happy-path still operator-run. Value claim alone is **not** S3. |
| **P1 closed** | Still correct for **value** HTLC lab path; RGB-wrap was always deferred in [P1_CLOSED.md](./P1_CLOSED.md) |
| **U4** | **New** — public hosting security gate (not a silent expansion of closed P3) |

### ADR stubs (fill before/while implementing)

#### ADR-S3 — RGB-wrapped claim

| Topic | Working default |
|-------|-----------------|
| Scope | CLI-first; both BTC and Liquid legs |
| Seal | Funding transfer assigns RGB allocation to **HTLC outpoint** |
| Claim tx | Preimage reveal + spend HTLC + **successor seal** + **commitment** (tapret preferred on both; opret only if required by a specific covenant demo) |
| Leg 2 preimage | Prefer extract from **confirmed claim witness**, not only local session file |
| Session | Versioned per-leg RGB fields (contract, plan ids, seals, consignment ref, verify status) |
| Done | Value claims **and** both RGB `anchor_verify` valid |
| Regression | Keep `rgb_wrap=false` value-only path |

#### ADR-C2 — Burn mint-gate (**accepted · implemented**)

| Topic | Decision |
|-------|----------|
| Burn | Explicit asset + exact tranche to **empty SPK** (SHA256∅ baked as `VAULT_SPK_HASH`) |
| Anchor | **Separate** opret vout0 (not dual-role OP_RETURN) |
| Gate | Recreate gate (same C1 program + recursion) |
| BFA | `mode=burn` + empty `vault=` in `elements-backing:v1` terms |
| Evidence | [C2_CLOSED.md](./C2_CLOSED.md) · `./scripts/demo_c2_mint_gate_burn.sh` |

#### ADR-C4 — Time-locked staking (**accepted · implemented**)

| Topic | Decision |
|-------|----------|
| Time | Absolute block height via `jet::check_lock_height` + `nLockTime` |
| Principal | Full stake input → `STAKER_SPK_HASH` (explicit asset+amount) |
| Trigger | Keyless; anyone after maturity |
| Fees | Separate P2WPKH input |
| Rewards / partial / rollover | **Out of MVP** |
| RGB | Deferred; MVP is seal-value only |
| Evidence | [C4_CLOSED.md](./C4_CLOSED.md) · `./scripts/demo_c4_stake.sh` |

#### ADR-U4 — Public vs operator surface (**accepted · implemented**)

| Topic | Decision |
|-------|----------|
| Public | GET static + `/v1` read surface; see `GET /v1/security` |
| Mutations | Bearer `LABD_API_TOKEN` (constant-time); required if `LABD_PUBLIC_READ_ONLY` or non-loopback |
| CORS | Allowlist `LABD_CORS_ORIGINS` (no `*` in public mode) |
| labd bind | Operator `127.0.0.1`; Cloud Run `0.0.0.0:$PORT` + read-only |
| Docker RPC | Host bind **127.0.0.1** only |
| Mainnet | Forbidden at config load |
| Evidence | [U4_PUBLIC_HOSTING.md](./U4_PUBLIC_HOSTING.md) · `Dockerfile.public` · `deploy/` |

---

## Phase S3 — RGB-wrapped claim (primary)

**Goal:** one transaction per leg reveals preimage, closes HTLC-bound RGB seal, creates receiver seal, re-anchors, passes verify.

**Surfaces:** CLI first; `/v1` status only after invariants stable.

**Exit:** documented runbook + negative tests (missing/wrong commitment, wrong seal, bad consignment, failed verify after broadcast, preimage extract failures).

**Estimate:** 2–4 weeks.

---

## Phase C2 — Burn mint-gate

Reuse C1 tooling. Program + demo + BFA burn mode + negatives.

**Estimate:** 1–2 weeks after ADR-C2.

---

## Phase C4 — Staking

**Closed** — [C4_CLOSED.md](./C4_CLOSED.md). Absolute height + principal-home; no rewards/partial.

---

## Phase U4 — Security gate (before Internet)

**Implemented in-tree** — [U4_PUBLIC_HOSTING.md](./U4_PUBLIC_HOSTING.md).  
Still **required** before marketing a public URL: deploy soak + no secrets in image.

MVP delivered: loopback RPC ports, `LABD_PUBLIC_READ_ONLY`, Bearer on POST, id regex, CORS allowlist, body limit, public Dockerfile + Vercel/Cloud Run sketches.

---

## Explicit non-goals (near term)

- Public Internet labd without U4  
- Browser-side RGB transition construction  
- Reopening P1/P2/P3 as “failed” — they closed the scopes they claimed  
- Mainnet  

---

## Next concrete actions

1. ~~Lock strategy: protocol first, localhost.~~  
2. ~~Reconcile S3/U4 in SCENARIOS + this roadmap.~~  
3. ~~Implement **S3** CLI + live proof.~~ → [S3_RGB_WRAP.md](./S3_RGB_WRAP.md)  
4. ~~**C2** burn mint-gate on regtest.~~ → [C2_CLOSED.md](./C2_CLOSED.md)  
5. ~~**C4** staking.~~ → [C4_CLOSED.md](./C4_CLOSED.md)  
6. ~~**U4** security engineering.~~ → [U4_PUBLIC_HOSTING.md](./U4_PUBLIC_HOSTING.md)  
7. ~~Public content + CI + harden.~~ → [PUBLIC_LAUNCH.md](./PUBLIC_LAUNCH.md) · `artifacts/public/` · `.github/workflows/*`  
8. Operator: enable deploy secrets → 24–48h soak → announce.  
