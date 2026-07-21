# P2 plan — Simplicity seal covenants & backed assets

**Status:** **P2 CLOSED** (C0+C3; C1 delivered as stretch)  
**Date:** 2026-07-21  
**Depends on:** P0 (RGB on Liquid) · P1 closed ([P1_CLOSED.md](./P1_CLOSED.md))  
**Closure:** [P2_CLOSED.md](./P2_CLOSED.md) · [C0_CLOSED.md](./C0_CLOSED.md) · [C1_CLOSED.md](./C1_CLOSED.md) · [C3_CLOSED.md](./C3_CLOSED.md)  
**Scenarios:** [SCENARIOS.md](./SCENARIOS.md) C0–C5 · pins/ADR [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) · stack [STACK.md](./STACK.md)

---

## One-paragraph intent

P0 proved RGB can live on Liquid; P1 proved twin HTLC swaps without a custodian.
**P2** makes the *seal UTXO itself* programmable: Liquid **consensus** enforces spend
policy (Simplicity covenants)—for example “reveal preimage **and** leave an RGB-shaped
anchor,” or “mint only if exact backing is locked.” That is the lab’s Liquid-native
differentiator beyond plain Script HTLCs.

---

## Story: padlock vs vault door

| Layer | Analogy |
|-------|---------|
| RGB consignment | Title deed in a sealed envelope |
| Seal UTXO | Whoever can spend this coin holds the current title |
| Tapret / opret commitment | Wax stamp on the next envelope |
| Bitcoin/Liquid **Script** padlock | Signature / hash / timeout — **cannot** inspect sibling outputs |
| **Simplicity** vault door | Covenants + **introspection jets** — can require “output 0 is opret-shaped,” vault asset+amount, gate recursion |

Without covenants, a keyholder can move the seal coin without re-anchoring RGB; clients
reject the history, but the chain still moves value. With a seal covenant, **invalid for
RGB** and **invalid for Liquid** start to align for critical moves.

Reference pattern (KaleidoSwap spike):  
`preimage(H) ∧ valid_rgb_anchor` — a spend stripped of its anchor is rejected by the
**node**, not only by tooling.

---

## Why P2 matters

1. **Honesty gap** — Soft client rules become hard consensus rules for seal spends.
2. **Liquid differentiation** — Programmable seals are a first-class Liquid story; Bitcoin Script is weak at covenants.
3. **Product shapes** — Permissionless backed mint, oracle-free audit, time-locked stake.
4. **Ladder narrative** — Completes “RGB on Liquid → interop → chain-enforced policy” before P3 UX polish.

---

## Scenario map (from SCENARIOS)

| ID | Theme | Pass criteria (summary) | Priority |
|----|--------|-------------------------|----------|
| **C0** | Simplicity `preimage ∧ opret-shaped anchor` | Compliant spend OK; strip-anchor rejected by consensus | **MVP** |
| **C1** | Mint-gate (lock vault) | Permissionless mint only with backing lock + gate recursion | High |
| **C2** | Mint-gate burn variant | OP_RETURN burn as backing + anchor | Medium |
| **C3** | BFA schema + audit | Over-mint fails audit; honest history passes; no oracle | **Public demo** |
| **C4** | Time-locked staking covenant | Early unstake rejected; principal returns to staker | Stretch |
| **C5** | LiquiDEX / native vs RGB swap | Docs comparison only | Docs |

**P2 closed for the lab (definition of done):** C0 + C3 green on **CI regtest**, writeup in
`docs/P2_CLOSED.md`, optional public testnet note. Others may remain regtest-only.

---

## Applications (product stories)

| Story | Scenario | Outcome |
|-------|----------|---------|
| Swap that cannot ghost the asset | C0 | Claim must leave RGB-shaped commitment or chain rejects |
| Community mint vault | C1 | Anyone mints if they bring reserves; cheaters fail at the node |
| Auditor with no oracle | C3 | Browser walks mints: seal, anchor, vault ≥ rate |
| Stake and forget | C4 | After maturity anyone may unstake; principal forced home |

Do **not** confuse **native Liquid `issue_asset`** (backing / fees) with **RGB contracts**.

---

## Repo readiness assessment

### Already in place

- Patched `WitnessTx` + live P0 verify on Liquid Testnet  
- LWK-based PSET / broadcast (`lab-chain`)  
- HTLC + swap sessions (P1) — preimage/hash patterns for C0  
- Shared `/v1` + web verifier + `/demo` board  
- Scenario IDs C0–C5 and ARCHITECTURE sketch for `POST /v1/covenant/*`  
- STACK note: evaluate `lwk_simplicity` vs spike-style programs  

### Gaps

| Gap | Impact |
|-----|--------|
| No `lab-simplicity` crate / `.simf` programs | Greenfield covenant work |
| No Elements-with-Simplicity regtest in-repo (F2 deferred) | Need Docker compose for P2 |
| Public Liquid Testnet Simplicity availability | **Regtest-first**; testnet optional |
| P0 path leaned **tapret**; spike C0 used **opret** shape for jets | C0 ADR: opret for MVP |
| No IFA / BFA schemas yet | Needed for C1–C3 |
| Toolchain pin risk | SimplicityHL + Elements 23.3+ version lock |

### Risk scores

| Area | Level | Note |
|------|-------|------|
| Conceptual clarity | High | Ladder already written |
| RGB core reuse | High | `lab-rgb` + vendor patch |
| Covenant engineering | Medium–Low | New crate + regtest |
| Public testnet demo | Medium | May lag; CI is source of truth |
| Product demo value | High | Differentiates the lab |

---

## Guiding decisions (ADR defaults)

Record final choices in this file’s “Decisions log” after R0.

| Topic | Default recommendation |
|-------|------------------------|
| Network order | **Regtest first** (Elements Simplicity); public testnet only if features confirmed |
| C0 anchor shape | **opret-shaped** commitment at vout 0 (spike-faithful); tapret later |
| Program source | Start **spike-style** programs (known-good demos); evaluate wrap via `lwk_simplicity` once C0 green |
| API surface | Keep consensus logic behind `/v1`; CLI thin client |
| Scope discipline | Do not block on P3 UI, CLN, or full Simplicity HTLC (sig+CSV) |
| Secrets | Regtest keys under `.rgbmvp/` only; never commit |

### Dual path evaluation (week 1)

- **Path A:** Port/adapt KaleidoSwap `spike-simplicity` programs + driver into `lab-simplicity`.  
- **Path B:** Drive standard programs through LWK `lwk_simplicity` if API covers leaf + witness.  

**Default:** Path A for C0; Path B as packaging once demos pass.

---

## Implementation phases

### R0 — Research freeze (3–5 days)

| Task | Output |
|------|--------|
| Pin Elements (23.3+ Simplicity) + SimplicityHL toolchain | Version pins in this doc / STACK |
| Inventory spike `rgb_anchor_covenant.simf` jets | Program checklist |
| Probe `lwk_simplicity` ~0.18 API | Path A vs B decision |
| ADR: opret-only C0 vs dual anchor | Written decision |
| Docker compose: `elementsd` (Simplicity) ± bitcoind | Restore F2 for P2 |

**Exit:** toolchain + network + program source frozen.

### C0 — Anchor covenant (P2 MVP)

**Goal:** Compliant spend OK; strip-anchor spend rejected by consensus.

| Step | Work | Surfaces |
|------|------|----------|
| C0.1 | `crates/lab-simplicity/` — compile/load program, CMR, address | Rust |
| C0.2 | Fund seal UTXO under Simplicity tapleaf (0xbe) | CLI / regtest RPC |
| C0.3 | Spend with preimage + opret 32-byte payload @ vout 0 | CLI |
| C0.4 | Negative: strip opret → node reject | CI |
| C0.5 | Optional: MPC root from real `lab-rgb` transfer plan | Bridge |
| C0.6 | Status API + demo board chip | `/v1/covenant/*`, web |

**CLI sketch:**

```bash
rgbmvp covenant compile --program rgb_anchor
rgbmvp covenant fund --program rgb_anchor --hash <H> --wallet alice
rgbmvp covenant spend-ok --preimage <s> --mpc-root <32B>
rgbmvp covenant spend-bad-strip-anchor   # expect consensus reject
```

### C1–C2 — Mint-gate

| ID | Work |
|----|------|
| C1 | IFA-style gate seal + vault lock (asset/amount jets) + gate recursion |
| C2 | Burn variant: single OP_RETURN as anchor + destroy backing |

Backing may use **native** Liquid test assets (LWK `issue_asset`) — infrastructure only, not “RGB.”

### C3 — BFA schema + audit (public-facing win)

| Step | Work |
|------|------|
| Schema | BFA = IFA + vault/rate in genesis → terms in contract id |
| `bfa-audit` | Walk mints: seal closed, anchor match, vault ≥ minted × rate |
| Web | Read-only `/audit/{contract_id}` (same ethos as verify) |
| Negative | Over-mint fails audit; tampered history fails anchor |

### C4 — Staking (stretch)

Absolute time lock; principal forced to staker; keyless trigger after maturity.

### C5 — Docs

Native Liquid P2P / LiquiDEX vs RGB twin swaps — extend [WALLETS.md](./WALLETS.md) positioning.

---

## Suggested calendar

```text
Week 1     R0 research + Docker Elements Simplicity + ADR
Week 2–3   C0 green on regtest + CLI + one CI job
Week 4     C0 optional RGB-linked opret; /v1 + demo chip
Week 5–6   C1 mint-gate (lock) on regtest
Week 7     C3 BFA audit CLI + web page
Week 8+    C2 burn, C4 stake; public testnet probe if available
```

---

## Target layout

```text
crates/
  lab-simplicity/     # programs, CMR, spend builders, negative tests
  lab-rgb/            # extend: IFA/BFA schemas, audit walk
  lab-chain/          # elements regtest RPC / fund simplicity outs
  lab-api/            # /v1/covenant/*, /v1/audit/*
  lab-cli/            # covenant + bfa-audit subcommands
docker/ or scripts/regtest/
  elements-simplicity.conf
  compose (P2 only)
docs/
  P2_PLAN.md          # this file
  P2_SIMPLICITY.md    # toolchain pins (after R0)
  P2_CLOSED.md        # when C0+C3 done
programs/ or vendor/  # *.simf with license attribution if vendored
```

### API sketch (stable `/v1`)

| Method | Path | Role |
|--------|------|------|
| POST | `/v1/covenant/compile` | Program id → CMR / address params |
| POST | `/v1/covenant/fund` | Record funded seal (lab session) |
| POST | `/v1/covenant/spend` | Build/broadcast compliant spend (local/regtest) |
| GET | `/v1/covenant/{id}` | Status (no secrets) |
| POST | `/v1/audit/bfa` | Full-history backing audit |
| GET | `/v1/audit/{contract_id}` | Shareable audit result |

Validation rules stay in Rust; web remains a thin client (no private keys).

---

## Ops & sudo

| Need | When | Notes |
|------|------|-------|
| Docker / docker group | Regtest Elements | May need `sudo usermod -aG docker $USER` once |
| Build tools | Already for P0/P1 | `build-essential`, `pkg-config`, `libssl-dev`, `clang`, `cmake` if missing |
| SimplicityHL toolchain | R0 pin | Install per ADR; no secrets |
| Public testnet | Optional later | No sudo; feature may be unavailable |

Phase core remains **no sudo** for day-to-day CLI once Docker is available to the user.

---

## Explicit non-goals (P2)

- Full browser send/swap theatre (P3)  
- Core Lightning / RGB-over-Lightning  
- Calling native Liquid assets “RGB”  
- Production mainnet covenants  
- Full Simplicity HTLC (claimer sig + CSV) in the first C0 cut  
- Blocking lab work on public testnet Simplicity availability  

---

## Provenance & references

- Scenario ladder: [SCENARIOS.md](./SCENARIOS.md)  
- Architecture (programmable seals, `/v1/covenant/*`): [ARCHITECTURE.md](./ARCHITECTURE.md)  
- LWK / `lwk_simplicity`: [STACK.md](./STACK.md)  
- Upstream inspiration: [kaleidoswap/rgb-on-liquid-spike](https://github.com/kaleidoswap/rgb-on-liquid-spike)  
  (`spike-simplicity`, `demo_simplicity.sh`, mint-gate / BFA / staking demos)  
- Blockstream LWK: [github.com/Blockstream/lwk](https://github.com/Blockstream/lwk)  

If vendoring `.simf` or driver code, preserve original license headers (MIT/Apache-2.0 dual on spike; follow file headers).

---

## Decisions log

| Date | Decision | Notes |
|------|----------|-------|
| 2026-07-21 | Plan written; regtest-first; C0 opret MVP; Path A default | Post–P1_CLOSED |
| 2026-07-21 | **R0 complete** — pins + Docker + ADR | See [P2_SIMPLICITY.md](./P2_SIMPLICITY.md) |
| 2026-07-21 | Elements **23.3.0** + `evbparams=simplicity:-1:::` | Image `ghcr.io/vulpemventures/elements:23.3.0`, RPC **:7042** |
| 2026-07-21 | **simplicityhl 0.6** · **simplicity-lang 0.8** | Path A; crates.io verified |
| 2026-07-21 | Path A for C0 (not `lwk_simplicity` 0.18 / hl 0.5) | ADR-002 |
| 2026-07-21 | C0 opret shape; tapret later | ADR-003 |
| 2026-07-21 | **C0 closed** on regtest | demo_c0_simplicity.sh; consensus −26 strip-anchor |
| 2026-07-21 | **C1 closed** on regtest | demo_c1_mint_gate.sh; 2 mints + 3 negatives |
| 2026-07-21 | **C3 closed** + **P2 closed** | demo_c3_bfa_audit.sh; honest/over-mint/lie |
| _TBD_ | Public testnet C0/C1/C3 yes/no | After feature probe |

---

## Next concrete actions

1. ~~R0–C3 / P2 closed.~~ See [P2_CLOSED.md](./P2_CLOSED.md).  
2. Optional C2 burn / C4 stake.  
3. Optional `/v1/audit/*` + demo board chip.  
4. **P3** browser UI on shared `/v1`.
