//! HTTP `/v1` surface helpers and route catalog (P3 lab console).

use lab_core::HealthReport;
use serde_json::{json, Value};

/// Wrap a health report as a `/v1/health`-shaped document.
pub fn health_json(report: &HealthReport) -> Value {
    json!({
        "api": lab_core::API_VERSION,
        "path": "/v1/health",
        "body": report,
    })
}

/// Route catalog for browsers and agents (`GET /v1`).
pub fn root_json() -> Value {
    json!({
        "product": lab_core::PRODUCT,
        "api": lab_core::API_VERSION,
        "phase": "u4-public-ready",
        "message": "RGB Liquid Testnet Lab — U4 security gate: public demos are read-only; keys stay off the Internet.",
        "security": {
            "browser_seeds": false,
            "preimage_redacted_on_swap_get": true,
            "model": "u4-public-read-only-or-operator-loopback",
            "public_read_only_env": "LABD_PUBLIC_READ_ONLY",
            "api_token_env": "LABD_API_TOKEN",
            "cors_env": "LABD_CORS_ORIGINS",
            "doc": "docs/U4_PUBLIC_HOSTING.md"
        },
        "endpoints": {
            "catalog": "GET /v1",
            "health": "GET /v1/health",
            "security": "GET /v1/security",
            "networks": "GET /v1/networks",
            "verify": "POST /v1/rgb/verify",
            "issue": "POST /v1/rgb/issue",
            "transfer": "POST /v1/rgb/transfer",
            "contracts": "GET /v1/rgb/contracts",
            "plans": "GET /v1/rgb/plans/{id}",
            "proofs": "GET /v1/proofs/{id}",
            "swap": "GET /v1/swap/{id}",
            "swaps": "GET /v1/swaps",
            "swap_init": "POST /v1/swap/init",
            "swap_action": "POST /v1/swap/{id}/action",
            "demo_wallets": "GET /v1/demo/wallets",
            "demo_activity": "GET /v1/demo/activity",
            "audit_bfa": "POST /v1/audit/bfa",
            "phases": "GET /v1/phases"
        },
        "pages": {
            "console": "/",
            "demo_board": "/demo",
            "audit": "/audit",
            "docs_u4": "docs/U4_PUBLIC_HOSTING.md",
            "docs_p3": "docs/P3_PLAN.md"
        },
        "cli": [
            "rgbmvp net status",
            "rgbmvp wallet address|balance",
            "rgbmvp rgb issue|transfer|verify",
            "rgbmvp swap init|status|fund-*|claim-*",
            "rgbmvp bfa audit --history …",
            "rgbmvp covenant demo|demo-c1|demo-c2|demo-c4",
            "rgbmvp serve"
        ]
    })
}

/// Public security posture (`GET /v1/security`).
pub fn security_json(public_read_only: bool, loopback_bind: bool, token_configured: bool) -> Value {
    json!({
        "api": lab_core::API_VERSION,
        "path": "/v1/security",
        "u4": true,
        "public_read_only": public_read_only,
        "loopback_bind": loopback_bind,
        "api_token_configured": token_configured,
        "mutations": if public_read_only {
            "require_bearer_token"
        } else if loopback_bind {
            "open_on_loopback_unless_token_set"
        } else {
            "require_bearer_token"
        },
        "public_surface": [
            "GET /",
            "GET /demo",
            "GET /audit",
            "GET /v1",
            "GET /v1/health",
            "GET /v1/phases",
            "GET /v1/networks",
            "GET /v1/security",
            "GET /v1/proofs/{id}",
            "GET /v1/swaps",
            "GET /v1/swap/{id}",
            "GET /v1/rgb/contracts",
            "GET /v1/rgb/plans/{id}",
            "GET /v1/demo/*"
        ],
        "doc": "docs/U4_PUBLIC_HOSTING.md"
    })
}

/// Ladder phase chips for the demo board / console.
pub fn phases_json() -> Value {
    json!({
        "phases": [
            {"id": "0", "name": "Foundations", "status": "done"},
            {"id": "P0", "name": "RGB on Liquid", "status": "done"},
            {"id": "P1", "name": "HTLC twin swap", "status": "closed", "doc": "docs/P1_CLOSED.md"},
            {"id": "P2", "name": "Simplicity + BFA", "status": "closed", "doc": "docs/P2_CLOSED.md",
             "slices": ["C0", "C1", "C2", "C3", "C4"]},
            {"id": "P3", "name": "Browser lab console", "status": "closed", "doc": "docs/P3_CLOSED.md",
             "slices": ["U0", "U1", "U2", "audit"]},
            {"id": "S3", "name": "RGB-wrapped claim", "status": "done", "doc": "docs/S3_RGB_WRAP.md"},
            {"id": "U4", "name": "Public hosting security", "status": "implemented", "doc": "docs/U4_PUBLIC_HOSTING.md"}
        ]
    })
}
