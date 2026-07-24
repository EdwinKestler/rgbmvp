# Web UI/UX refresh — implemented and validated

**Status:** Implemented and locally validated  
**Date:** 2026-07-24  
**Baseline:** `9d1b104`  
**Rollback:** [UI_ROLLBACK_PLAN.md](./UI_ROLLBACK_PLAN.md)  
**Scope:** presentation and thin-client browser behavior only

## Outcome

The four existing browser surfaces now share a proof-first visual and
information architecture:

| Route | Human purpose |
|-------|---------------|
| `/` | Explain the lab and its testnet boundary, lead with inspectable S3 evidence, then expose the local operator console |
| `/status` | Read-only evidence explorer for the phase ladder, public-testnet S3 proof, and reproducible regtest covenants |
| `/demo` | Read-only observatory for wallets, swaps, transfer plans, and proofs |
| `/audit` | Guided C3 BFA history audit with seal, anchor, and backing results |

The design uses a distinct `rgbmvp` identity: near-black research-lab surfaces,
warm white text, a restrained electric-green verification accent, turquoise
links, and amber network/read-only boundaries. It is inspired by modern
Bitcoin/Liquid research presentation without copying third-party logos or
brand assets.

## Scenario mapping

This is UX polish over already implemented behavior:

| Scenario | Refresh coverage |
|----------|------------------|
| `U0` | Shared shell, consistent navigation, board, and phase presentation |
| `U1` | Issue/transfer forms retain the existing `/v1` fields and actions |
| `U2` | Value-HTLC guided flow retains the existing state machine |
| `S3` | RGB-wrap verification is presented as fund → claim → re-anchor → verify |
| `R4`–`R6` | Verification status, failures, and degraded states are human-readable |
| `C3` | BFA audit input and results have a guided presentation |
| `U4` | Public read-only and local operator surfaces remain visibly distinct |

P1, P2, P3, S3, and U4 evidence remains closed under its original definitions.
The refresh adds no protocol capability and does not widen any claim.

## Preserved boundaries

- No change to RGB consensus, validation, transaction construction, seals,
  commitments, consignments, witness extraction, HTLC scripts, or Simplicity.
- No change to `/v1` routes, request fields, response fields, action values, or
  status meanings.
- No change to U4 authorization, CORS, body limits, rate limits, path rules, or
  public swap redaction.
- No browser seeds, wallet keys, private consignments, or swap preimages.
- No change to `artifacts/public/` evidence.
- No deployment or cloud-resource changes.
- Liquid Testnet, Bitcoin testnet, and regtest only; Mainnet remains out of
  scope.

## Accessibility and browser hardening

- Consistent primary navigation and skip links.
- Semantic tab roles, `aria-selected`, controlled panels, and arrow/Home/End
  keyboard navigation.
- Explicit form-label associations and live status/error regions.
- Visible high-contrast focus indicators and minimum 44 px form controls.
- Responsive layouts at desktop and mobile widths; narrow tables scroll rather
  than overflow the viewport.
- `prefers-reduced-motion` handling.
- API/manifest-derived values escaped before HTML insertion on refreshed result
  surfaces.
- External URLs protocol-checked where rendered from data; new-tab links use
  `noopener noreferrer`.

The UI remains framework-free static HTML/CSS/JavaScript. Static assets were not
split into a shared route because both Axum and the legacy server currently
serve the four HTML files explicitly; changing server routing was outside the
approved UI-only boundary.

## Validation evidence

Validated locally on 2026-07-24:

| Check | Result |
|-------|--------|
| JavaScript parse for all four HTML files | Pass |
| `git diff --check` | Pass |
| `/`, `/demo`, `/status`, `/audit` | HTTP 200 |
| `/v1`, `/v1/health`, `/v1/security`, `/v1/phases` | HTTP 200 |
| Rendered health state replaces initial loading state | Pass |
| Evidence manifest renders the S3 session | Pass |
| Inactive tab panels hidden with correct ARIA state | Pass |
| Desktop 1440 × 1100 visual inspection | Pass |
| Mobile 390 × 844 visual inspection | Pass |
| `cargo test --workspace` | 108 passed, 2 ignored; doc tests passed |
| `pytest -q` | 12 passed |
| Protocol/API/evidence/deploy diff | Empty |

The Rust totals above include the vendored RGB consensus workspace tests. The
two ignored tests are the existing performance-only tree tests.

## Rollback readiness

The refresh is independently reversible under
[UI_ROLLBACK_PLAN.md](./UI_ROLLBACK_PLAN.md). Security, protocol-boundary,
evidence-integrity, preimage exposure, broken primary flows, or inaccurate
network claims are immediate rollback triggers.

Publication and Vercel/Google Cloud resource assignment remain pending
administrator approval. This validation does not claim that a public deployment
has occurred.
