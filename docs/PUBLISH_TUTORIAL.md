# Publish the frozen testnet demo — Cloud Run and Vercel

**Scope:** operator tutorial for the current read-only proof of concept

**GCP project ID:** `silicon-pointer-490721-r0`

**Networks:** Liquid Testnet, Bitcoin testnet, and regtest only

**Mainnet:** out of scope
**Deployment authority:** administrator approval and cloud resource assignment
are required before running deployment steps

This tutorial does not host wallets, keys, seeds, private consignments,
preimages, or Elements RPC. It publishes only the frozen public image and
redacted public artifacts.

## Understand the two surfaces first

| Surface | Current capability | Recommended use |
|---------|--------------------|-----------------|
| **Cloud Run** | Static UI and live read-only `/v1` from one origin | Primary functional demo |
| **Vercel** | Static UI and canned public artifacts | Optional secondary presentation |

The browser currently calls relative paths such as `/v1/security`. The Vercel
configuration does not proxy those paths to Cloud Run. Therefore, publishing
the repository to Vercel as-is does **not** create a functional `/v1` frontend.
Use the Cloud Run URL for the functional demo. Adding a Vercel `/v1` proxy is a
separate configuration change that requires review, CORS/security validation,
and a new CI run.

## 1. Publication checklist

Before changing either provider, confirm:

1. An administrator approved the public-demo deployment and billing budget.
2. The intended Git revision has green `ci` and `docker-public` workflows.
3. The GitHub repository is `EdwinKestler/rgbmvp`.
4. Google Cloud shows project ID `silicon-pointer-490721-r0`. The display name
   may differ; permissions and commands use the project **ID**.
5. Billing is attached and a small budget alert is configured.
6. No `.env`, `.rgbmvp/`, wallet, WIF, seed, or private consignment is staged.
7. The GitHub environment `public-demo` exists and has required reviewers if
   the administrator wants a manual deployment approval gate.

Record the release revision:

```bash
git switch main
git pull --ff-only
git status --short
git rev-parse HEAD
gh run list --branch main --limit 10
```

Do not deploy a dirty working tree or an unreviewed revision.

## 2. Install and authenticate operator tools

Use [Google Cloud Shell](https://shell.cloud.google.com/) or a workstation with
the `gcloud` CLI, plus GitHub CLI and Vercel CLI where applicable.

```bash
gcloud auth login
gcloud config set project silicon-pointer-490721-r0
gcloud config get-value project
gh auth status
npm install --global vercel@latest
vercel --version
```

Do not create or download a Google service-account JSON key. The repository
workflow uses short-lived GitHub OIDC credentials through Workload Identity
Federation (WIF).

## 3. Prepare Google Cloud resources once

Set shell variables without reusing system variables:

```bash
export RGBMVP_GCP_PROJECT="silicon-pointer-490721-r0"
export RGBMVP_GCP_REGION="us-central1"
export RGBMVP_GITHUB_REPO="EdwinKestler/rgbmvp"
export RGBMVP_WIF_POOL="github-actions"
export RGBMVP_WIF_PROVIDER="rgbmvp"
```

Confirm the active identity and project before creating anything:

```bash
gcloud auth list
gcloud config set project "$RGBMVP_GCP_PROJECT"
gcloud projects describe "$RGBMVP_GCP_PROJECT" \
  --format='table(projectId,name,projectNumber)'
```

Enable the required APIs:

```bash
gcloud services enable \
  run.googleapis.com \
  artifactregistry.googleapis.com \
  iam.googleapis.com \
  iamcredentials.googleapis.com \
  sts.googleapis.com \
  --project="$RGBMVP_GCP_PROJECT"
```

Create the Docker repository and the two distinct service accounts:

```bash
gcloud artifacts repositories create rgbmvp \
  --repository-format=docker \
  --location="$RGBMVP_GCP_REGION" \
  --description="rgbmvp frozen public images" \
  --project="$RGBMVP_GCP_PROJECT"

gcloud iam service-accounts create rgbmvp-public-run \
  --display-name="rgbmvp public runtime - no project roles" \
  --project="$RGBMVP_GCP_PROJECT"

gcloud iam service-accounts create rgbmvp-deploy \
  --display-name="rgbmvp GitHub deployment identity" \
  --project="$RGBMVP_GCP_PROJECT"
```

If a resource already exists, inspect it instead of treating `ALREADY_EXISTS`
as a failure. The runtime account must have no project roles. The deploy
account receives only deployment permissions.

```bash
export RGBMVP_RUNTIME_SA="rgbmvp-public-run@${RGBMVP_GCP_PROJECT}.iam.gserviceaccount.com"
export RGBMVP_DEPLOY_SA="rgbmvp-deploy@${RGBMVP_GCP_PROJECT}.iam.gserviceaccount.com"

gcloud artifacts repositories add-iam-policy-binding rgbmvp \
  --location="$RGBMVP_GCP_REGION" \
  --member="serviceAccount:${RGBMVP_DEPLOY_SA}" \
  --role="roles/artifactregistry.writer" \
  --project="$RGBMVP_GCP_PROJECT"

gcloud projects add-iam-policy-binding "$RGBMVP_GCP_PROJECT" \
  --member="serviceAccount:${RGBMVP_DEPLOY_SA}" \
  --role="roles/run.admin"

gcloud iam service-accounts add-iam-policy-binding "$RGBMVP_RUNTIME_SA" \
  --member="serviceAccount:${RGBMVP_DEPLOY_SA}" \
  --role="roles/iam.serviceAccountUser" \
  --project="$RGBMVP_GCP_PROJECT"
```

`roles/run.admin` is used because the workflow creates a public service and
updates its invoker policy. Tightening this after the first deployment should
be handled as a separate IAM review.

## 4. Configure GitHub-to-Google WIF once

Create a pool and a provider restricted to the exact repository:

```bash
gcloud iam workload-identity-pools create "$RGBMVP_WIF_POOL" \
  --location=global \
  --display-name="GitHub Actions" \
  --project="$RGBMVP_GCP_PROJECT"

gcloud iam workload-identity-pools providers create-oidc "$RGBMVP_WIF_PROVIDER" \
  --location=global \
  --workload-identity-pool="$RGBMVP_WIF_POOL" \
  --display-name="rgbmvp GitHub provider" \
  --issuer-uri="https://token.actions.githubusercontent.com" \
  --attribute-mapping="google.subject=assertion.sub,attribute.repository=assertion.repository,attribute.ref=assertion.ref" \
  --attribute-condition="assertion.repository == '${RGBMVP_GITHUB_REPO}' && assertion.ref == 'refs/heads/main'" \
  --project="$RGBMVP_GCP_PROJECT"
```

Obtain the full pool and provider resource names:

```bash
export RGBMVP_WIF_POOL_NAME="$(gcloud iam workload-identity-pools describe "$RGBMVP_WIF_POOL" \
  --location=global --project="$RGBMVP_GCP_PROJECT" --format='value(name)')"

export RGBMVP_WIF_PROVIDER_NAME="$(gcloud iam workload-identity-pools providers describe "$RGBMVP_WIF_PROVIDER" \
  --location=global --workload-identity-pool="$RGBMVP_WIF_POOL" \
  --project="$RGBMVP_GCP_PROJECT" --format='value(name)')"

printf '%s\n' "$RGBMVP_WIF_POOL_NAME" "$RGBMVP_WIF_PROVIDER_NAME"
```

Allow only this repository identity to impersonate the deploy account:

```bash
gcloud iam service-accounts add-iam-policy-binding "$RGBMVP_DEPLOY_SA" \
  --member="principalSet://iam.googleapis.com/${RGBMVP_WIF_POOL_NAME}/attribute.repository/${RGBMVP_GITHUB_REPO}" \
  --role="roles/iam.workloadIdentityUser" \
  --project="$RGBMVP_GCP_PROJECT"
```

IAM and WIF changes can take several minutes to propagate.

## 5. Configure the GitHub `public-demo` environment

In GitHub, open **Settings → Environments → New environment**, create
`public-demo`, and add required reviewers if desired. Then set environment
variables and secrets:

```bash
gh variable set GCP_PROJECT_ID --env public-demo \
  --body "silicon-pointer-490721-r0"
gh variable set GCP_REGION --env public-demo --body "us-central1"
gh variable set GCP_AR_REPO --env public-demo --body "rgbmvp"
gh variable set GCP_RUNTIME_SERVICE_ACCOUNT --env public-demo \
  --body "rgbmvp-public-run@silicon-pointer-490721-r0.iam.gserviceaccount.com"

printf '%s' "$RGBMVP_WIF_PROVIDER_NAME" | \
  gh secret set GCP_WORKLOAD_IDENTITY_PROVIDER --env public-demo
printf '%s' "$RGBMVP_DEPLOY_SA" | \
  gh secret set GCP_SERVICE_ACCOUNT --env public-demo
```

Verify names, not secret values:

```bash
gh variable list --env public-demo
gh secret list --env public-demo
```

Do not set `LABD_API_TOKEN`. Do not set Mainnet network values. Leave
`LABD_CORS_ORIGINS` unset for the initial single-origin Cloud Run deployment.

## 6. Deploy the functional demo to Cloud Run

The preferred path uses the reviewed workflow and tags the image with the exact
Git commit:

1. Open **GitHub → Actions → deploy-cloudrun**.
2. Select **Run workflow** on `main`.
3. Approve the `public-demo` environment if protection is enabled.
4. Wait for authentication, image push, deployment, and post-deploy smoke.
5. Record the workflow URL, commit SHA, image URI, Cloud Run revision, and URL.

CLI equivalent for starting and watching the workflow:

```bash
gh workflow run deploy-cloudrun.yml --ref main
gh run list --workflow deploy-cloudrun.yml --branch main --limit 5
gh run watch RUN_ID --exit-status
```

Confirm the deployed revision and immutable image digest:

```bash
gcloud run services describe rgbmvp-public \
  --region=us-central1 \
  --project=silicon-pointer-490721-r0 \
  --format='yaml(status.url,status.latestReadyRevisionName,spec.template.spec.serviceAccountName,spec.template.spec.containers[0].image)'
```

## 7. Verify the Cloud Run security boundary

Set the URL returned by the workflow:

```bash
export RGBMVP_PUBLIC_URL="https://REPLACE_WITH_CLOUD_RUN_URL"
```

Run only read-only checks, except for the deliberately unauthenticated POST
that must be rejected before reaching protocol logic:

```bash
curl -fsS "$RGBMVP_PUBLIC_URL/v1/health"
curl -fsS "$RGBMVP_PUBLIC_URL/v1/security"
curl -fsSI "$RGBMVP_PUBLIC_URL/v1/security"

curl -sS -o /tmp/rgbmvp-post.json -w '%{http_code}\n' \
  -X POST "$RGBMVP_PUBLIC_URL/v1/swap/init" \
  -H 'content-type: application/json' \
  -d '{}'
```

Required results:

- `/v1/security` reports `public_read_only: true`.
- The unauthenticated POST returns HTTP `403`.
- Headers include CSP, `X-Content-Type-Options: nosniff`, and
  `X-Frame-Options: DENY`.
- `/`, `/status`, and `/v1/health` return successfully.
- Public swap GET responses never expose a preimage.
- The service account is `rgbmvp-public-run@...`, with no wallet or secret
  volume and at most one instance.

## 8. Publish the optional static presentation to Vercel

This section publishes the repository **as it currently exists**: static pages
and public artifacts only.

### 8.1 Create and link the Vercel project

From the clean repository root:

```bash
vercel login
vercel link
```

Choose the intended Vercel scope, create or select `rgbmvp-public`, and link the
current directory. Vercel writes `.vercel/project.json`; `.vercel/` is local
provider state and must not be committed.

Inspect the identifiers:

```bash
vercel project inspect rgbmvp-public
sed -n '1,80p' .vercel/project.json
```

Create a scoped Vercel token in **Account/Team Settings → Tokens**. Store the
token and the identifiers in GitHub's `public-demo` environment:

```bash
gh secret set VERCEL_TOKEN --env public-demo
gh secret set VERCEL_ORG_ID --env public-demo \
  --body "REPLACE_WITH_ORG_OR_TEAM_ID"
gh secret set VERCEL_PROJECT_ID --env public-demo \
  --body "REPLACE_WITH_PROJECT_ID"
gh secret list --env public-demo
```

Do not print the token and do not commit `.vercel/`.

### 8.2 Deploy through GitHub Actions

1. Open **GitHub → Actions → deploy-vercel**.
2. Select **Run workflow** on `main`.
3. Approve the `public-demo` environment if required.
4. Record the production deployment URL and exact Git SHA.

CLI equivalent:

```bash
gh workflow run deploy-vercel.yml --ref main
gh run list --workflow deploy-vercel.yml --branch main --limit 5
gh run watch RUN_ID --exit-status
```

Verify static routes:

```bash
export RGBMVP_VERCEL_URL="https://REPLACE_WITH_VERCEL_URL"
curl -fsSI "$RGBMVP_VERCEL_URL/"
curl -fsSI "$RGBMVP_VERCEL_URL/status"
curl -fsSI "$RGBMVP_VERCEL_URL/demo"
curl -fsS "$RGBMVP_VERCEL_URL/manifest.json"
```

Expected limitation: `/v1/*` on the Vercel URL is not the Cloud Run API. Label
or share the Vercel URL as a static presentation, and share the Cloud Run URL as
the functional read-only demo.

## 9. First-publication soak

For 24–48 hours:

1. Keep Cloud Run at min `0`, max `1`, 1 CPU, and 512 MiB.
2. Generate only GET traffic after the required POST-denial smoke.
3. Monitor Cloud Run request/error latency, instances, logs, and billing.
4. Confirm there are no mutation successes, secrets, wallet mounts, or RPC
   connections.
5. Keep `LABD_HTTP=legacy` in the image as rollback insurance; do not remove it
   during the soak.
6. Do not announce until administrator and security review sign off.

## 10. Rollback

### Cloud Run

List revisions and route all traffic to the last known-good revision:

```bash
gcloud run revisions list \
  --service=rgbmvp-public \
  --region=us-central1 \
  --project=silicon-pointer-490721-r0

gcloud run services update-traffic rgbmvp-public \
  --to-revisions=KNOWN_GOOD_REVISION=100 \
  --region=us-central1 \
  --project=silicon-pointer-490721-r0
```

If exposure must stop immediately, remove public access or delete the service
only with administrator approval. Prefer revision rollback because it preserves
evidence and is readily reversible.

### Vercel

Use **Project → Deployments → known-good deployment → Promote to Production**,
or revert the offending Git commit and let the workflow deploy the reverted
revision. Record which deployment was promoted and why.

After rollback, repeat the security smoke and retain incident logs. Do not
remove `LABD_HTTP=legacy` until the documented post-soak removal gate is met.

## 11. Evidence to retain

Record in the release/soak notes:

- administrator approval;
- Git SHA and green CI workflow URLs;
- Cloud Run workflow, image URI/digest, revision, and public URL;
- Vercel workflow, deployment ID, and static URL if enabled;
- security-smoke output with no secrets or preimages;
- start/end time of the 24–48h soak and monitoring result;
- rollback revision/deployment IDs;
- final announce or no-go decision.

## Official provider references

- [Google Cloud: Workload Identity Federation for deployment pipelines](https://cloud.google.com/iam/docs/workload-identity-federation-with-deployment-pipelines)
- [Google GitHub Actions authentication](https://github.com/google-github-actions/auth)
- [Google Cloud: deploy container images to Cloud Run](https://cloud.google.com/run/docs/deploying)
- [Google Cloud: service-account practices for deployment pipelines](https://cloud.google.com/iam/docs/best-practices-for-using-service-accounts-in-deployment-pipelines)
- [Vercel: GitHub deployments](https://vercel.com/docs/git/vercel-for-github)
- [Vercel: GitHub Actions setup](https://vercel.com/kb/guide/how-can-i-use-github-actions-with-vercel)

## Repository references

- [PUBLIC_LAUNCH.md](./PUBLIC_LAUNCH.md)
- [U4_PUBLIC_HOSTING.md](./U4_PUBLIC_HOSTING.md)
- [U5_AXUM.md](./U5_AXUM.md)
- [UI_ROLLBACK_PLAN.md](./UI_ROLLBACK_PLAN.md)
- [deploy/README.md](../deploy/README.md)
- [Cloud Run workflow](../.github/workflows/deploy-cloudrun.yml)
- [Vercel workflow](../.github/workflows/deploy-vercel.yml)
