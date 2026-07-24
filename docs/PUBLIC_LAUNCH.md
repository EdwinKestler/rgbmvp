# Public launch checklist (Phase 4 content/CI + Phase 5 hardening)

**Status:** In-repo ready · **Date:** 2026-07-23  
**Depends on:** [U4_PUBLIC_HOSTING.md](./U4_PUBLIC_HOSTING.md)

This closes the “content & CI / hardening before announce” ladder without putting
secrets or hot wallets on the Internet.

---

## Phase 4 — Content & CI ✅ (in-repo)

| Item | Location |
|------|----------|
| Public proofs (no secrets) | [`artifacts/public/`](../artifacts/public/) |
| S3 live summary (preimage redacted) | [`artifacts/public/s3-rgbmvp-live.json`](../artifacts/public/s3-rgbmvp-live.json) |
| Phase chips + explorer links | [`artifacts/public/manifest.json`](../artifacts/public/manifest.json) · [`web/status.html`](../web/status.html) |
| Proof-first UI + rollback boundary | [`UI_REFRESH.md`](./UI_REFRESH.md) · [`UI_ROLLBACK_PLAN.md`](./UI_ROLLBACK_PLAN.md) |
| CI: cargo test + build | [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) |
| CI: gitleaks (+ path-scoped BFA fixture allowlist) | [`.gitleaks.toml`](../.gitleaks.toml) · `ci.yml` |
| CI: public image + Trivy (GHCR lowercase owner) | [`.github/workflows/docker-public.yml`](../.github/workflows/docker-public.yml) |
| Axum security headers (CSP, nosniff, frame deny, cache) | `crates/lab-cli/src/labd_axum.rs` |
| Public UI presentation (`public_read_only`) | `web/index.html` · `web/audit.html` |
| Deploy Cloud Run (OIDC) | [`.github/workflows/deploy-cloudrun.yml`](../.github/workflows/deploy-cloudrun.yml) — needs secrets |
| Deploy Vercel | [`.github/workflows/deploy-vercel.yml`](../.github/workflows/deploy-vercel.yml) — needs secrets |
| README badges | root [README.md](../README.md) |

### Enable deploy workflows (operator)

Full name table + `gh` commands: **[deploy/README.md](../deploy/README.md#github-actions--variables--secrets)**.

**Cloud Run (OIDC) — primary public origin**

| Kind | Names |
|------|--------|
| Variables | `GCP_PROJECT_ID`, `GCP_REGION`, `GCP_AR_REPO`, optional `LABD_CORS_ORIGINS` |
| Secrets | `GCP_WORKLOAD_IDENTITY_PROVIDER`, `GCP_SERVICE_ACCOUNT` |
| Environment | `public-demo` |

1. GCP project + enable APIs + Artifact Registry repo `rgbmvp`.  
2. Workload Identity Federation for GitHub → fill the two secrets.  
3. Set vars (example project: `silicon-pointer-490721-r0`, region `us-central1`, AR `rgbmvp`).  
4. **Actions → deploy-cloudrun → Run** (or push paths that trigger it).

**Vercel (optional / secondary)**

| Kind | Names |
|------|--------|
| Secrets | `VERCEL_TOKEN`, `VERCEL_ORG_ID`, `VERCEL_PROJECT_ID` |

Until `GCP_PROJECT_ID` is set, Cloud Run deploy **no-ops**. Until `VERCEL_TOKEN` is set, Vercel deploy soft-skips.

---

## Phase 5 — Hardening before announce

The browser refresh is locally validated, but it does not alter the remaining
operator gates: resource assignment, first deployment, GET-only soak, and
administrator approval are still required before announcement.

| Item | Status |
|------|--------|
| Gitleaks on CI | ✅ `ci.yml` |
| Image scan (Trivy HIGH/CRITICAL) | ✅ `docker-public.yml` |
| Rate limit `POST /v1/rgb/verify` | ✅ `LABD_VERIFY_RATE_LIMIT` (default 30/min/IP) |
| 24–48h soak (GET only) | ⏳ **operator** after first public URL |
| Announce | ⏳ after soak |

### Soak procedure (operator)

1. Deploy public image / Vercel with `LABD_PUBLIC_READ_ONLY=1` (image default).  
2. Confirm:
   - `GET /v1/security` → `public_read_only: true`
   - `POST /v1/*` without token → **403**
   - `GET /status` loads phase chips from manifest  
3. Leave live **24–48h**; watch Cloud Run/Vercel metrics and budget alerts.  
4. Only GET traffic; no wallet mounts.  
5. Then announce (README already states read-only demo).

### Announce blurb (copy)

> **rgbmvp** is a public testnet lab for RGB-on-Liquid (and Bitcoin twins), HTLC swaps, and Simplicity seal demos.  
> **Public surface is read-only** (status board + explorers). Full ladder: run locally — see README.

---

## Explicit non-goals

- Hosting WIFs / mnemonics / preimages  
- Public Elements regtest RPC  
- Mainnet  
- Unauthenticated mutations  
