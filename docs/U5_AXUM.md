# U5 — labd HTTP platform (Axum / Hyper)

**Status:** Implemented (default labd backend) — 2026-07-24  
**Scenario:** extension of P3/U4 surfaces — **does not reopen** closed P3 or U4 scopes  
**Roadmap:** [ROADMAP_NEXT.md](./ROADMAP_NEXT.md)

## Run

```bash
rgbmvp serve --bind 127.0.0.1:8080
# logs: labd (axum/U5) listening on …

# optional: previous handwritten TCP server
LABD_HTTP=legacy rgbmvp serve
```

## Why U5

Historically, `rgbmvp serve` used a handwritten TCP HTTP/1.1 loop (still available as
`LABD_HTTP=legacy`). That became risky as S3/S5 added mutations.

**U5 (default):** transport/platform is **Axum over Hyper**, while keeping:

- Existing `/v1` request and response shapes
- U4 security posture (read-only public, Bearer mutations, CORS allowlist, …)
- Shared application services used by CLI and HTTP (no JS-side RGB rules)

## Non-goals

- Reopening P3 as “incomplete” — U5 is platform parity, not a new browser phase
- Weakening U4 — all U4 gates must pass on the Axum router
- Mainnet
- Browser-side preimage or consignment construction

## Prerequisites

1. Service extraction: swap/RGB orchestration callable without Clap  
   (`lab_api::SwapService`, domain gates in `lab_rgb::swap`)
2. S3 offline negative matrix green in CI
3. Parity tests for catalog, health, security, public swap redaction

## Target shape

```text
lab-cli          → Clap only; calls services
lab-api          → /v1 types, public views, SwapService
lab-rgb / …      → domain
labd (Axum)      → routes + middleware → same services
```

Middleware parity: Bearer, public read-only, constant-time token, CORS allowlist,
body size, timeouts, concurrency, verify rate limit, path-ID validation,
request IDs, secret redaction, generic external errors, preimage redaction on
every public swap GET.

Blocking LWK / filesystem / `reqwest::blocking` must not occupy Tokio workers
(dedicated blocking pool).

## Acceptance

- [x] `/v1` shapes compatible with current console  
- [x] U4 mutation gate + CORS on Axum middleware  
- [x] GET `/v1/swap/*` uses `public_swap_view` (preimage redacted)  
- [x] Dual path: `LABD_HTTP=legacy` keeps handwritten TCP server for one release  
- [x] Blocking LWK/verify work via `spawn_blocking`  
- [ ] Full parity automated suite vs legacy (expand tests as needed)  
- [ ] Remove legacy after soak
