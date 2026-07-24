# Public deploy sketches (U4)

**Do not** expose wallets, WIFs, or Elements RPC to the Internet.  
Public posture: **read-only** labd + static UI. Full protocol demos stay on the operator machine.

See [docs/U4_PUBLIC_HOSTING.md](../docs/U4_PUBLIC_HOSTING.md).

## Recommended split

| Surface | Host | Cost |
|---------|------|------|
| Static console / board | **Vercel** Hobby | $0 |
| Optional live `GET /v1/*` | **GCP Cloud Run** | ~$0 scale-to-zero |

Publication freeze for this lab: **single Cloud Run origin first** (no Vercel as primary).
Vercel secrets below are optional/secondary.

---

## GitHub Actions — variables & secrets

Workflows: [`.github/workflows/deploy-cloudrun.yml`](../.github/workflows/deploy-cloudrun.yml),
[`.github/workflows/deploy-vercel.yml`](../.github/workflows/deploy-vercel.yml).  
GitHub **Environment:** `public-demo` (jobs bind to it; vars/secrets may be **repository** or **environment** scoped).

### Repository / environment **variables** (`vars.*`)

| Name | Required | Example | Used by |
|------|----------|---------|---------|
| `GCP_PROJECT_ID` | **yes** (Cloud Run) | `silicon-pointer-490721-r0` | `deploy-cloudrun` job gate + image/project |
| `GCP_REGION` | **yes** (Cloud Run) | `us-central1` | Artifact Registry host + `gcloud run deploy` |
| `GCP_AR_REPO` | **yes** (Cloud Run) | `rgbmvp` | Image path `…/$GCP_AR_REPO/rgbmvp-public:sha` |
| `LABD_CORS_ORIGINS` | optional | `https://your-origin.example` | Cloud Run env; default in workflow if empty: `https://example.vercel.app` |

```bash
# Repository-scoped (recommended minimum)
gh variable set GCP_PROJECT_ID --body "silicon-pointer-490721-r0"
gh variable set GCP_REGION --body "us-central1"
gh variable set GCP_AR_REPO --body "rgbmvp"
# optional:
# gh variable set LABD_CORS_ORIGINS --body "https://YOUR_PUBLIC_ORIGIN"

# Or environment-scoped (same names under public-demo):
# gh variable set GCP_PROJECT_ID --env public-demo --body "silicon-pointer-490721-r0"
```

### Repository / environment **secrets** (`secrets.*`)

| Name | Required | Example shape | Used by |
|------|----------|---------------|---------|
| `GCP_WORKLOAD_IDENTITY_PROVIDER` | **yes** (Cloud Run) | `projects/PROJECT_NUMBER/locations/global/workloadIdentityPools/POOL/providers/PROVIDER` | `google-github-actions/auth` |
| `GCP_SERVICE_ACCOUNT` | **yes** (Cloud Run) | `rgbmvp-deploy@PROJECT_ID.iam.gserviceaccount.com` | OIDC deploy identity |
| `VERCEL_TOKEN` | yes (Vercel only) | Vercel personal/token | `deploy-vercel` gate + action |
| `VERCEL_ORG_ID` | yes (Vercel only) | `team_…` / org id | `amondnet/vercel-action` |
| `VERCEL_PROJECT_ID` | yes (Vercel only) | `prj_…` | `amondnet/vercel-action` |

Built-in (no config): `GITHUB_TOKEN` — CI gitleaks, GHCR push in `docker-public.yml`.

```bash
# Cloud Run OIDC (values from GCP WIF setup — never commit)
gh secret set GCP_WORKLOAD_IDENTITY_PROVIDER --body "projects/…/locations/global/workloadIdentityPools/…/providers/…"
gh secret set GCP_SERVICE_ACCOUNT --body "rgbmvp-deploy@silicon-pointer-490721-r0.iam.gserviceaccount.com"

# Optional Vercel (skip while Cloud Run is the only public origin)
# gh secret set VERCEL_TOKEN --body "…"
# gh secret set VERCEL_ORG_ID --body "…"
# gh secret set VERCEL_PROJECT_ID --body "…"

# Environment-scoped alternative:
# gh secret set GCP_WORKLOAD_IDENTITY_PROVIDER --env public-demo
# gh secret set GCP_SERVICE_ACCOUNT --env public-demo
```

### Gate behavior

| Workflow | Runs when | Skips when |
|----------|-----------|------------|
| `deploy-cloudrun` | `vars.GCP_PROJECT_ID` is non-empty | `GCP_PROJECT_ID` unset/empty |
| `deploy-vercel` | `secrets.VERCEL_TOKEN` set | `VERCEL_TOKEN` empty (soft skip in step) |

With `GCP_PROJECT_ID` set but **without** the two GCP secrets, Cloud Run deploy **starts and fails** at OIDC auth — finish WIF + SA secrets before the next path-triggered push, or run deploy only via **workflow_dispatch** after secrets exist.

### Verify on the repo

```bash
gh variable list
gh secret list
gh variable list --env public-demo
gh secret list --env public-demo
```

---

## 1. Vercel (static)

```bash
# From repo root (requires vercel CLI: npm i -g vercel)
cp deploy/vercel.json ./vercel.json   # or link
vercel                  # preview
vercel --prod           # production
```

Point the browser at static pages only, or set a future `window.LABD_API` to Cloud Run.

`LABD_CORS_ORIGINS` on Cloud Run must include your Vercel origin.

## 2. Cloud Run (public labd)

```bash
export PROJECT=your-gcp-project
export REGION=us-central1

gcloud config set project "$PROJECT"
gcloud services enable run.googleapis.com cloudbuild.googleapis.com artifactregistry.googleapis.com

# One-time Artifact Registry repo
gcloud artifacts repositories create rgbmvp \
  --repository-format=docker --location="$REGION" || true

IMAGE="${REGION}-docker.pkg.dev/${PROJECT}/rgbmvp/rgbmvp-public:latest"
gcloud builds submit --tag "$IMAGE" -f Dockerfile.public .

gcloud run deploy rgbmvp-public \
  --image="$IMAGE" \
  --region="$REGION" \
  --allow-unauthenticated \
  --min-instances=0 \
  --max-instances=2 \
  --memory=512Mi \
  --cpu=1 \
  --set-env-vars="LABD_PUBLIC_READ_ONLY=1,RGBMVP_NETWORK=liquid-testnet,LABD_CORS_ORIGINS=https://YOUR_VERCEL_DOMAIN"
```

Budget alert: set a $1–5/month budget in GCP Billing.

## 3. Local smoke (public mode)

```bash
export LABD_PUBLIC_READ_ONLY=1
export LABD_CORS_ORIGINS=http://127.0.0.1:8080
export LABD_BIND=127.0.0.1:8080
cargo run -p lab-cli -- serve

curl -s http://127.0.0.1:8080/v1/security | jq .
# POST without token → 403
curl -s -X POST http://127.0.0.1:8080/v1/swap/init -d '{}' | jq .
```

## 4. Modal.com

Not used for the public site. Optional later for ephemeral regtest jobs only.
