# Scenario ladder (full capacity, phased)

Each phase is **publicly demonstrable** when complete. Later phases reuse the
same CLI + `/v1` API + web verifier shell.

Legend: **CLI** · **Web** · **Regtest CI** · **Liquid Testnet** · **BTC Testnet**

---

## Phase 0 — Foundations (before public claims)

**Goal:** runnable skeleton, network connectivity, docs, no false “RGB works” claims.

| ID | Scenario | Surfaces | Pass criteria |
|----|----------|----------|---------------|
| F0 | Health + network config | CLI, Web, API | `/v1/health` reports liquid-testnet reachability (Esplora/Electrum or node) |
| F1 | LWK single-sig flow | CLI | Create signer/wollet, show address, sync, show L-BTC balance (faucet funded) |
| F2 | Regtest docker smoke | CI | elementsd (+ bitcoind) up; can list UTXOs |
| F3 | Vendored / documented `WitnessTx` patch strategy | docs + build | Build path for patched `rgb-consensus` documented; unit tests for trait adapter |

**Exit:** developer can fund a Liquid Testnet address via faucet and see balance in CLI.

**Status (2026-07-21):** F0/F1/F3 met on this machine — `lab0` funded (100k L-BTC sats + faucet side asset); F2 regtest docker still deferred.

### P0 live proof (2026-07-21)

| Scenario | Result |
|----------|--------|
| R0 issue | `rgb:JBZ2QrMz-…` NIA `tRGB` on Liquid Testnet seal |
| R2 transfer+broadcast | tx `2b1b2f045ab9797ff34dd919293fb5b67e4e123d5557d99fc90fd06cb36c2635` |
| R4 verify | seal_closure + tapret_dbc + **anchor_verify** all **ok** (`status: valid`) |
| R5 proof | stored under `.rgbmvp/rgb/proofs/` + `rgbmvp serve` web verifier |
| R3 consign | `rgb consign put|get` blob store |
| R7 confidential | seal UTXO was CT-funded; commitment output explicit P2TR (TapretFirst) |

---

## Phase P0 — RGB on Liquid (core claim)

**Goal:** issue, transfer, verify a real RGB20 on Liquid Testnet; public proof page.

| ID | Scenario | Surfaces | Pass criteria |
|----|----------|----------|---------------|
| R0 | Issue RGB20 (NIA) on Liquid Testnet | CLI, API | Genesis `chain_net = liquid-testnet`; contract id returned |
| R1 | Build invoice / receive seal intent | CLI | Invoice JSON includes network + seal policy |
| R2 | Transfer with anchor tx | CLI, API | Liquid tx broadcasts; commitment present (tapret/opret); explorer URL |
| R3 | Consignment exchange | CLI, API | Upload/download consignment by id; TTL enforced |
| R4 | Client-side verify | CLI, **Web verifier** | Seal closure + `Anchor::verify` on Liquid witness; status valid |
| R5 | Public proof page | Web | Shareable `/proofs/{id}` with checks + tx link |
| R6 | Negative: stale/missing witness | CLI, Web | Clear invalid/pending statuses; no silent success |
| R7 | Confidential seal outputs | CLI | Transfer with blinded Liquid outputs; verify still passes (spike claim) |

**Exit (public):** two wallets on Liquid Testnet complete issue→send→verify via CLI; a third party can open the web verifier and confirm validity without keys.

### CLI sketch (P0)

```bash
rgbmvp net status
rgbmvp wallet create --network liquid-testnet
rgbmvp wallet address
rgbmvp rgb issue --ticker tRGB --name "Test RGB" --supply 1000000
rgbmvp rgb invoice --contract <id>
rgbmvp rgb transfer --to <invoice> --amount 1000
rgbmvp rgb consign export --opid <id> -o payment.rgbc
rgbmvp rgb verify --consignment payment.rgbc
rgbmvp serve --bind 127.0.0.1:8080   # labd + static verifier
```

### Web verifier (P0) — minimal pages

1. **Home** — what this lab is; testnet disclaimer; link to docs.
2. **Verify** — paste consignment or proof id → structured checks.
3. **Proof** — read-only result page (browser-shareable).

Prepare routes/components so a later “Wallet” page can call the same API
without rewriting validation.

---

## Phase P1 — Interop (atomic swap, no custodian)

**Goal:** demonstrate Bitcoin-layer ↔ Liquid RGB twin swap publicly on testnets.

| ID | Scenario | Surfaces | Pass criteria |
|----|----------|----------|---------------|
| S0 | Issue twin pair | CLI | RGB-A on BTC testnet/signet + RGB-B on Liquid testnet (separate contract ids) |
| S1 | Hashlock setup | CLI, API | Shared `H = SHA256(s)`; addresses on both chains |
| S2 | Full Script HTLC | CLI, CI | Claim, wrong preimage reject, early refund reject, post-timeout refund |
| S3 | RGB-wrapped claim | CLI, CI | One tx: reveal preimage + close seal + re-anchor asset |
| S4 | Coordinated swap (two parties or user↔maker bot) | CLI, API, Web status | Atomicity: either both complete or refund path; both anchors verify |
| S5 | Round-trip swap | CLI, CI | There and back; supplies conserved on each chain |
| S6 | (Optional) CLN adjacency | docs only | Document that LN is **not** required; optional future BTC LN UX via Boltz/CLN is separate |

**Exit (public):** documented runbook where Alice and Bob (or Alice and demo maker) swap RGB twins on public testnets; web shows swap status machine.

Lightning note: P1 default is **on-chain HTLC**, matching the spike. CLN is optional infrastructure for Bitcoin-side UX experiments, not for Liquid channels.

**Plan:** [`P1_SWAP_PLAN.md`](./P1_SWAP_PLAN.md).  
BTC fixture: [`fixtures/testnet_btc.json`](../fixtures/testnet_btc.json).

**Progress (2026-07-21):**

| ID | Status |
|----|--------|
| S0 (BTC twin) | **Live** — `bRGB` issued + verified on bitcoin-testnet |
| S0 (LQ twin) | Available via P0 Liquid path (`alice`/`bob`) |
| S1 HTLC addresses | **Done** — dual HTLC (BTC claimer=bob, LQ claimer=alice) |
| S2 fund both | **Live** — `swap fund-btc` + `swap fund-lq` |
| S2 claim (value HTLC) | **Live** — Alice `claim-lq` then Bob `claim-btc` → **phase=done** (Script HTLC only) |
| S3 RGB-wrapped claim | **CLI implemented** — [S3_RGB_WRAP.md](./S3_RGB_WRAP.md); fund-wrap + multi-out claim + extract-preimage; live testnet proof operator-run |
| S4 coordinator | CLI session under `.rgbmvp/swaps/` (+ P3 guided UI) |
| S2 refund | **CLI done** — `swap refund-btc` / `swap refund-lq` (CSV mature) |
| S5 round-trip | Deferred |
| P1 closure | **CLOSED** for **value** HTLC path — [`P1_CLOSED.md`](./P1_CLOSED.md); S3 was always deferred there |

**HTLC live path (`p1-live`, 2026-07-21):**

| Step | Tx |
|------|-----|
| BTC fund 10k | [`d7094192…a848`](https://blockstream.info/testnet/tx/d709419264b737df9558858ccc0f411d4586ff3a34852d691509777dd7cba848) |
| LQ fund 5k | [`c2b59fbe…ec27`](https://blockstream.info/liquidtestnet/tx/c2b59fbef40dea977a7f30bf78d73932e68c95775f3b0d234e3aa12b992cec27) |
| LQ claim (preimage) | [`c4b8e9d6…97a7`](https://blockstream.info/liquidtestnet/tx/c4b8e9d6a241665bc1de9f344392ce831177de06164cbddd42972436813197a7) |
| BTC claim | [`15cb860e…0e1f`](https://blockstream.info/testnet/tx/15cb860e686389575eb0123a51db73c1b635e2e4705146caf35b79d04af40e1f) |

BTC RGB anchor (earlier): [`2a573998…1806`](https://blockstream.info/testnet/tx/2a5739986a0a3d41a6a4a2a3a2062504af8320d92c7786a49fde627fea571806) (≥3 confs).

---

## Phase P2 — Programmable seals & backed assets

**Plan:** [`P2_PLAN.md`](./P2_PLAN.md) (regtest-first, C0 + C3 = lab closed).  
**R0 pins / Docker / ADR:** [`P2_SIMPLICITY.md`](./P2_SIMPLICITY.md) (`./scripts/regtest_simplicity.sh up`).  
**C0 closed:** [`C0_CLOSED.md`](./C0_CLOSED.md) · `./scripts/demo_c0_simplicity.sh`.  
**C1 closed:** [`C1_CLOSED.md`](./C1_CLOSED.md) · `./scripts/demo_c1_mint_gate.sh`.  
**C3 closed:** [`C3_CLOSED.md`](./C3_CLOSED.md) · `./scripts/demo_c3_bfa_audit.sh`.  
**P2 closed:** [`P2_CLOSED.md`](./P2_CLOSED.md) (C0+C3 definition of done).

**Goal:** exercise Simplicity + backed-mint patterns that differentiate Liquid.

| ID | Scenario | Surfaces | Pass criteria |
|----|----------|----------|---------------|
| C0 | Simplicity `preimage ∧ opret-shaped anchor` | CLI, regtest | **CLOSED** — compliant OK; strip-anchor consensus reject (−26 jet assertion) |
| C1 | Mint-gate (lock vault) | CLI, regtest | **CLOSED** — 2 chained mints; drop-anchor / wrong-amount / no-recreate reject |
| C2 | Mint-gate burn variant | CLI, CI | OP_RETURN burn as backing + anchor |
| C3 | BFA schema + audit | CLI, regtest | **CLOSED** — honest pass; over-mint fail; lie fails anchor |
| C4 | Time-locked staking covenant | CLI, CI | Early unstake rejected; principal returns to staker after maturity |
| C5 | (Stretch) LiquiDEX / native asset swap | docs | Optional comparison: Liquid native atomic swap vs RGB twin swap |

**Exit:** at least C0 + C3 demonstrated with public writeup; others in CI regtest.

---

## Phase P3 — Browser UI (after API is stable)

Not a new consensus phase—**UX only**.  
**Plan:** [`P3_PLAN.md`](./P3_PLAN.md) · **Closed:** [`P3_CLOSED.md`](./P3_CLOSED.md).  
**Headless kit:** [`HEADLESS.md`](./HEADLESS.md).

| ID | Scenario | Pass criteria |
|----|----------|---------------|
| U0 | Wallet page / lab board | **CLOSED** — board + phase chips + console shell |
| U1 | Issue / transfer wizards | **CLOSED** — `/v1/rgb/issue|transfer` + UI |
| U2 | Swap wizard | **CLOSED** — guided **value** HTLC fund/claim; demo-u2 phase done |
| U3 | Hardware / Marina path (optional) | Deferred |
| **U4** | Public hosting security gate | **Required before Internet exposure** — auth, CORS allowlist, limits, loopback RPC, read/write split ([ROADMAP_NEXT.md](./ROADMAP_NEXT.md)) |

P0 web verifier must not hard-code assumptions that block U0–U2 (shared API types, CORS, versioned errors).  
**P3 closed** remains valid for localhost operator console; U4 is a **new** ops/security scenario, not a silent reopening of P3.

---

## Failure scenarios (all phases)

| Failure | Expected behavior |
|---------|-------------------|
| Chain RPC / Esplora down | API `503`, CLI exit ≠ 0, explicit “chain unavailable” |
| Unknown schema / bad consignment | `invalid` with check names; never partial silent accept |
| Witness tx not yet confirmed | `pending_witness` + retry guidance |
| Redis project-memory down | Agents continue from files; lab product does not depend on it |
| Shared Redis | Lab never FLUSHDB/FLUSHALL |

---

## Definition of “ladder complete”

- P0 public on Liquid Testnet with web verifier.
- P1 at least one public **value** HTLC swap (user↔bot acceptable).
- P2 C0 + C3 green on CI; optional public testnet if tooling allows.
- P3 lab console closed on localhost operator model.
- **Next completeness (not required for historical P1–P3 close):** S3 RGB-wrapped claim; then public demo only after U4.
