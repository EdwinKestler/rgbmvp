# P3 plan — Browser lab console (UX only)

**Status:** **CLOSED** — see [P3_CLOSED.md](./P3_CLOSED.md)  
**Depends on:** P0–P2 closed ([P1_CLOSED.md](./P1_CLOSED.md) · [P2_CLOSED.md](./P2_CLOSED.md))  
**Scenarios:** [SCENARIOS.md](./SCENARIOS.md) U0–U3  
**Repo strategy:** **Monorepo** (`rgbmvp`) — no P2 split (see § Repo)

---

## Intent

P3 is **not** a new consensus phase. It is a **browser lab console** over the existing
CLI + `/v1` API so a third party can see and drive the ladder without memorizing flags.

```text
P0  RGB works on Liquid
P1  Twin HTLC swap without custodian
P2  Programmable seals + oracle-free BFA audit
P3  Visitor gallery + guided wizards (this doc)
```

Analogy: P0–P2 built the physics; P3 is the **guided exhibit**, not a second particle collider.

---

## Accepted ADRs (2026-07-21)

### ADR-P3-001 — Security: lab console, not hot wallet

| Mode | Default | Behavior |
|------|---------|----------|
| **A. Operator lab** | **On** | UI → local `labd`; signing uses **server-side** `.rgbmvp` fixtures only |
| **B. Watch-only** | **On** for U0 | Show balances; no spend from browser storage |
| **C. Browser signer** | **Off** | Optional later; not required for P3 close |

**Never:** mnemonics or WIF in `localStorage` / form fields for public demos.  
**Never:** render swap preimages (already redacted on `GET /v1/swap/{id}`).

### ADR-P3-002 — API-first

Missing `/v1` routes ship **before** rich wizards. Browser does not re-implement RGB verify, BFA audit, or Simplicity.

### ADR-P3-003 — Stack

| Choice | Decision |
|--------|----------|
| Frontend | Static HTML/JS (same origin as `rgbmvp serve`) |
| Framework | None required for P3 close (optional Vite later) |
| LWK WASM | Optional U0 L-BTC watch only |
| RGB ops | Always via labd |
| P2 in UI | Story + **BFA audit upload**; live C0/C1 remain CLI/regtest |

### ADR-P3-004 — P3 closed definition (lab)

1. **U0** — Lab console shell + board (wallets, activity, phase chips)  
2. **U1** — Issue + transfer wizards calling `/v1/rgb/*` (server-side keys)  
3. **U2** — Swap status + guided steps (mutations localhost / explicit allow)  
4. **P2 gallery** — BFA audit page (upload history → pass/fail)  
5. README “Browser tour” on Liquid Testnet  

**U3** (Marina/Jade) deferred.

---

## Current baseline

| Surface | State |
|---------|--------|
| `web/index.html` | Verify + swap status |
| `web/demo.html` | Read-only board (Demo v0) |
| `rgbmvp serve` | Static + partial `/v1` (health, demo, swap GET, rgb verify) |
| CLI | Full control plane for keys, P1, P2 demos |

---

## Phases

### P3.0 — API foundation

| Deliverable | Notes |
|-------------|--------|
| `GET /v1` | Route catalog (phase, endpoints) |
| `POST /v1/audit/bfa` | History JSON → `BfaAuditResult` |
| Expand RGB POST as needed for U1 | issue / transfer when wired to lab wallets |
| CORS | `Access-Control-Allow-Origin` (already `*`); OPTIONS if needed |
| Error envelope | Prefer `{ "error": "…", "status": "error" }` consistency |

### P3.1 — U0 lab console shell

| Deliverable | Notes |
|-------------|--------|
| Shared nav | Board · Verify · Swap · Audit · Docs |
| Upgrade `/demo` | Phase chips P0/P1/P2 closed; refresh |
| Home `/` | Console entry + health |

### P3.2 — U1 issue / transfer wizards

Forms → `/v1/rgb/issue` / transfer; download plan JSON; deep-link verify.

### P3.3 — U2 swap theatre

Timeline UI (exists) + guided fund/claim **or** copy-CLI; no preimage.

### P3.4 — P2 gallery + BFA audit UI

Upload `bfa_history.json` → table of seal/anchor/backing checks.  
C0/C1: docs + “run `demo_c0` / `demo_c1`” (no browser regtest).

### P3.5 — Polish

README browser tour; `docs/P3_CLOSED.md`; update banners.

**Rough calendar:** 3–5 weeks wall-clock for P3.0–P3.5.

---

## UX map

```text
┌──────────────────────────────────────────────────────────┐
│  rgbmvp lab console (testnet) — no seeds in browser      │
├─────────┬─────────┬──────────┬─────────┬─────────────────┤
│ Board   │ Verify  │ Issue/   │ Swap    │ Audit (BFA)     │
│ (U0)    │         │ Transfer │ (U2)    │ (P2 gallery)    │
│         │         │ (U1)     │         │                 │
└─────────┴─────────┴──────────┴─────────┴─────────────────┘
              │
              ▼
         labd /v1  ──►  lab-rgb · lab-chain · .rgbmvp
```

---

## Repo strategy (locked)

| Decision | Detail |
|----------|--------|
| **Keep monorepo** | `EdwinKestler/rgbmvp` + `ffwd-org/rgbmvp` |
| **No P2-only repo now** | Headless protocol kit lives *inside* this repo |
| **Headless entry** | [HEADLESS.md](./HEADLESS.md) — crates + demos without UI |
| **Extract later** | Only if external crate consumers / paper need a frozen narrow surface |

P2 standalone value is real (Simplicity seals + BFA audit) but splitting before API freeze costs more than it gains.

---

## Non-goals (P3)

- Full non-custodial multi-chain wallet  
- Browser Simplicity regtest  
- Seeds / WIF in the UI  
- Dual JS validation of RGB anchors  
- Marina/Jade required for close  
- Mainnet  

---

## Success metrics

| Metric | Target |
|--------|--------|
| Third party runs `rgbmvp serve` + browser tour | ≤ 15 minutes (with pre-built binary) |
| Verify + swap status + BFA audit | Work without CLI for **read** paths |
| Issue/transfer | Work via UI against **lab fixtures** only |
| Security review | No secret material in HTML/JS |

---

## Next actions (implementation order)

1. ~~Write this plan; accept ADRs; monorepo.~~  
2. ~~**P3.0** — `GET /v1`, `POST /v1/audit/bfa`, `GET /v1/phases`.~~  
3. ~~**P3.1** — Console nav + board/phase + `/audit` page.~~  
4. ~~**P3.2 / U1** — Issue + transfer wizards + `/v1/rgb/issue|transfer|contracts`.~~  
5. ~~**P3.3 / U2** — Swap guided actions (`/v1/swap/init`, `/action`).~~  
6. ~~**P3.5** polish + `P3_CLOSED.md` / browser tour / done banner / twin contract fields.~~  

## Decisions log

| Date | Decision |
|------|----------|
| 2026-07-21 | ADRs P3-001…004 accepted as written |
| 2026-07-21 | Monorepo retained; no P2 extract |
| 2026-07-21 | Proceed P3 starting at P3.0 + U0 shell |
| 2026-07-21 | P3.0 + P3.1 landed (catalog, BFA audit API/UI, console nav) |
| 2026-07-21 | U1 issue/transfer wizards + API (server-side lab wallets) |
| 2026-07-21 | U2 swap guided actions; preimage never on GET or UI |
| 2026-07-21 | **P3 closed** — celebrate banner, twin contract fields, P3_CLOSED.md |
