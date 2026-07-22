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
        "phase": "p3-closed",
        "message": "RGB Liquid Testnet Lab — P3 lab console closed. Keys stay on labd; UI is thin.",
        "security": {
            "browser_seeds": false,
            "preimage_redacted_on_swap_get": true,
            "model": "operator-lab-console"
        },
        "endpoints": {
            "catalog": "GET /v1",
            "health": "GET /v1/health",
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
            "docs_p3": "docs/P3_PLAN.md"
        },
        "cli": [
            "rgbmvp net status",
            "rgbmvp wallet address|balance",
            "rgbmvp rgb issue|transfer|verify",
            "rgbmvp swap init|status|fund-*|claim-*",
            "rgbmvp bfa audit --history …",
            "rgbmvp covenant demo|demo-c1",
            "rgbmvp serve"
        ]
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
             "slices": ["C0", "C1", "C3"]},
            {"id": "P3", "name": "Browser lab console", "status": "closed", "doc": "docs/P3_CLOSED.md",
             "slices": ["U0", "U1", "U2", "audit"]}
        ]
    })
}
