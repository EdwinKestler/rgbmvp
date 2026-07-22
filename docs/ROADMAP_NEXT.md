# Roadmap next — protocol completeness first (localhost / testnet)

**Strategy (locked 2026-07-22):**  
**Deepen protocol on testnet + localhost first.** Public Internet demo only after **U4** security gate.

Historical phase closures (P1–P3) remain valid evidence of what was proven. This roadmap is **extension work**, not a rewrite of those claims.

| Priority | Track | When |
|----------|--------|------|
| 0 | Doc honesty + ADRs | Now |
| 1 | **S3** RGB-wrapped claim (CLI, testnet/localhost) | Main protocol track |
| 2 | **C2** burn mint-gate (regtest) | After short C2 ADR; can trail S3 |
| 3 | **C4** staking research freeze → implement | After ADR-C4; last protocol track |
| 4 | **U4** public-hosting security foundation | Parallel design OK; **must finish before any public bind** |
| 5 | Independent review + public **read-only** demo | After U4 acceptance |

```text
Localhost / public testnet (operator)
   │
   ├─► S3 RGB-wrapped claim     ◄── protocol completeness (NOW)
   ├─► C2 burn · C4 stake       ◄── regtest protocol depth
   │
   └─► U4 security (must complete before)
              │
              ▼
         Internet demo (later; default read-only)
```

---

## Phase 0 — Extension contract (docs)

### Claim reconciliation

| ID | Correct status |
|----|----------------|
| **S2** | Script HTLC fund/claim/refund **live** (value path) |
| **S3** | **Pending** — RGB-wrapped claim (preimage + close HTLC seal + re-anchor + verify). Value claim alone is **not** S3. |
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

#### ADR-C2 — Burn mint-gate

| Topic | Working default |
|-------|-----------------|
| Burn | Explicit asset + exact tranche + **provably unspendable** output |
| Anchor | Prefer dual-role OP_RETURN only if jets can enforce both shape and burn; else separate outputs documented in ADR |
| Gate | Recreate gate (same as C1) unless ADR decides one-shot gate |
| BFA | `backing_mode=burn` committed in genesis terms (cannot silently switch to lock) |

#### ADR-C4 — Time-locked staking

| Topic | Working default (MVP) |
|-------|------------------------|
| Time | Absolute block height |
| Principal | Forced to committed staker script hash |
| Trigger | Anyone after maturity |
| Fees | Separate input (do not erode principal) |
| Rewards / partial / rollover | **Out of MVP** |
| RGB | Optional later; first cut may be seal-value only if ADR says so |

#### ADR-U4 — Public vs operator surface

| Topic | Working default |
|-------|-----------------|
| Public | GET static + health/phases/proofs/public swap status only |
| Mutations | Bearer `LABD_API_TOKEN`, constant-time compare, off by default off-loopback |
| CORS | Allowlist from config |
| labd bind | `127.0.0.1` behind TLS reverse proxy |
| Docker RPC | Host bind **127.0.0.1** only |
| Mainnet | Forbidden in public deploy config |

Full U4 work packages and acceptance: peer review adopted in project discussion (2026-07-22); implement when approaching public demo.

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

Research freeze 3–5 days → implement only with peer-minimal semantics.

**Estimate:** 2–3 weeks after freeze.

---

## Phase U4 — Security gate (before Internet)

Not required for S3/C2/C4 **localhost** work. **Required** before binding labd or demos to a public interface.

MVP before “full Axum rewrite” if needed: loopback RPC ports, mutation flag, Bearer on POST, id regex, CORS allowlist, body limit.

**Estimate:** 1–2 weeks + review/soak before public IP.

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
3. Implement **S3** CLI (funding seal = HTLC, claim multi-output + plan, extract-preimage).  
4. Keep U4 checklist ready; do U4 engineering before any public demo.  
5. C2 after ADR; C4 after research freeze.  
