# Public launch checklist (Phase 4 content/CI + Phase 5 hardening)

**Status:** In-repo ready · **Date:** 2026-07-23  
**Depends on:** [U4_PUBLIC_HOSTING.md](./U4_PUBLIC_HOSTING.md)

Operator walkthrough: [PUBLISH_TUTORIAL.md](./PUBLISH_TUTORIAL.md).

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

**Cloud Run (OIDC) — primary public origin (freeze)**

| Kind | Names |
|------|--------|
| Variables | `GCP_PROJECT_ID`, `GCP_REGION`, `GCP_AR_REPO`; optional `GCP_RUNTIME_SERVICE_ACCOUNT`, `LABD_CORS_ORIGINS` |
| Secrets | `GCP_WORKLOAD_IDENTITY_PROVIDER`, `GCP_SERVICE_ACCOUNT` (**deploy** SA only) |
| Environment | `public-demo` |
| Runtime SA | `rgbmvp-public-run@…` — dedicated, **no** project roles |

**First revision profile:** service `rgbmvp-public` · public auth · ingress all · min 0 / max **1** · 1 CPU · 512 MiB · `LABD_PUBLIC_READ_ONLY=1` · `RGBMVP_NETWORK=liquid-testnet` · **no** `LABD_API_TOKEN` · **no** wallets/volumes. See [`deploy/cloudrun.yaml`](../deploy/cloudrun.yaml).

1. GCP project + enable APIs + Artifact Registry repo `rgbmvp` + create runtime SA.  
2. Workload Identity Federation for GitHub → fill the two **deploy** secrets.  
3. Set vars (example project: `silicon-pointer-490721-r0`, region `us-central1`, AR `rgbmvp`).  
4. **Actions → deploy-cloudrun → Run** (workflow deploys freeze profile + post-deploy smoke).

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
| `LABD_HTTP=legacy` rollback backend | ✅ retained through first soak; removal gated afterward |
| 24–48h soak (GET only) | ⏳ **operator** after first public URL |
| Announce | ⏳ after soak |

> **T1 note.** `deploy/cloudrun.yaml` (read-only, no wallets) remains the
> publication profile and the target of this soak. The separate
> `deploy/cloudrun-demo.yaml` profile enables bounded public swaps and holds
> spendable testnet keys — it is **not** part of this launch and must not be
> deployed until its own gates pass. See
> [TESTNET_PUBLIC_SWAPS.md](./TESTNET_PUBLIC_SWAPS.md).

### Soak procedure (operator)

1. Deploy Cloud Run freeze revision (`rgbmvp-public`, max 1 instance, runtime SA no privileges).  
2. Confirm (also automated in `deploy-cloudrun` smoke step):
   - `GET /v1/security` → `public_read_only: true`
   - `POST /v1/*` without token → **403**
   - Security headers present (`nosniff`, `DENY`, CSP)
   - `GET /status` and `/v1/health` OK  
3. Leave live **24–48h**; watch Cloud Run metrics and budget alerts.  
4. Only GET traffic; no wallet mounts; no `LABD_API_TOKEN`.  
5. Keep `LABD_HTTP=legacy` available as rollback insurance throughout the soak;
   do not add features to it or switch without recording the incident.
6. After a successful soak, record parity/rollback sign-off. Legacy removal is a
   separate approved cleanup; it is not a publication prerequisite.
7. Then announce (README already states read-only demo).

### Announce blurb (copy)

> **rgbmvp** is a public testnet lab for RGB-on-Liquid (and Bitcoin twins), HTLC swaps, and Simplicity seal demos.  
> **Public surface is read-only** (status board + explorers). Full ladder: run locally — see README.

---

## Explicit non-goals

- Hosting WIFs / mnemonics / preimages  
- Public Elements regtest RPC  
- Mainnet  
- Unauthenticated mutations  
