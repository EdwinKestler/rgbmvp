//! HTTP `/v1` surface helpers.
//!
//! Phase 0: pure JSON helpers only (no server bind yet). P0 will add `labd` serve.

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

/// Placeholder root document for future static web verifier.
pub fn root_json() -> Value {
    json!({
        "product": lab_core::PRODUCT,
        "api": lab_core::API_VERSION,
        "phase": "p0",
        "message": "RGB Liquid Testnet Lab — P0 RGB issue/transfer/verify enabled (testnet only).",
        "endpoints": {
            "health": "/v1/health",
            "networks": "/v1/networks",
            "verify": "POST /v1/rgb/verify",
            "proofs": "GET /v1/proofs/{id}",
            "swap": "GET /v1/swap/{id}",
            "swaps": "GET /v1/swaps"
        },
        "cli": [
            "rgbmvp net status",
            "rgbmvp wallet create|address|balance|utxos",
            "rgbmvp rgb issue|invoice|transfer|verify",
            "rgbmvp swap init|fund-btc|fund-lq|claim-lq|claim-btc|status",
            "rgbmvp serve"
        ]
    })
}
