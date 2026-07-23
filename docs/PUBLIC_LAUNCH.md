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
| CI: cargo test + build | [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) |
| CI: gitleaks | same |
| CI: public image + Trivy | [`.github/workflows/docker-public.yml`](../.github/workflows/docker-public.yml) |
| Deploy Cloud Run (OIDC) | [`.github/workflows/deploy-cloudrun.yml`](../.github/workflows/deploy-cloudrun.yml) — needs secrets |
| Deploy Vercel | [`.github/workflows/deploy-vercel.yml`](../.github/workflows/deploy-vercel.yml) — needs secrets |
| README badges | root [README.md](../README.md) |

### Enable deploy workflows (operator)

**Vercel**

1. Create project from monorepo (or empty + GH).  
2. Repo secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`, `VERCEL_PROJECT_ID`.  
3. Push `main` touching `web/` or `artifacts/public/`, or **Actions → deploy-vercel → Run**.

**Cloud Run (OIDC)**

1. GCP project + Artifact Registry repo `rgbmvp`.  
2. Workload Identity Federation for GitHub.  
3. Repo vars: `GCP_PROJECT_ID`, `GCP_REGION`, `GCP_AR_REPO`, optional `LABD_CORS_ORIGINS`.  
4. Repo secrets: `GCP_WORKLOAD_IDENTITY_PROVIDER`, `GCP_SERVICE_ACCOUNT`.  
5. Environment `public-demo` for the job.  
6. **Actions → deploy-cloudrun → Run** (or push paths that trigger it).

Until vars/secrets are set, deploy jobs **no-op** (`if: vars/secrets empty`).

---

## Phase 5 — Hardening before announce

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
