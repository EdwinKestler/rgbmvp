# Public deploy sketches (U4)

**Do not** expose wallets, WIFs, or Elements RPC to the Internet.  
Public posture: **read-only** labd + static UI. Full protocol demos stay on the operator machine.

See [docs/U4_PUBLIC_HOSTING.md](../docs/U4_PUBLIC_HOSTING.md).

## Recommended split

| Surface | Host | Cost |
|---------|------|------|
| Static console / board | **Vercel** Hobby | $0 |
| Optional live `GET /v1/*` | **GCP Cloud Run** | ~$0 scale-to-zero |

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
