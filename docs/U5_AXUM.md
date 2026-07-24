# U5 — labd HTTP platform (Axum / Hyper)

**Status:** Planned (not implemented)  
**Scenario:** extension of P3/U4 surfaces — **does not reopen** closed P3 or U4 scopes  
**Roadmap:** [ROADMAP_NEXT.md](./ROADMAP_NEXT.md)

## Why U5

`rgbmvp serve` currently implements TCP accept, HTTP/1.1 parsing, route matching,
auth, CORS, static files, rate limits, and JSON errors in a large handwritten
loop inside `lab-cli` (~1.4k lines of `serve_labd` + handlers). That is fine for
the lab, but becomes risky as S3/S5 add mutations.

U5 replaces the **transport/platform** layer with Axum over Hyper while keeping:

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

- `/v1` shapes compatible with current console
- U4 security tests pass against Axum
- GET `/v1/swap/*` never exposes preimage
- Feature-flag or one-release dual path, then remove handwritten server
