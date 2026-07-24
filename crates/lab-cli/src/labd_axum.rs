//! U5 — Axum/Hyper labd (default for `rgbmvp serve`).
//!
//! Same `/v1` shapes and U4 security as the legacy TCP server; mutations call
//! shared `http_api` handlers (often via `spawn_blocking` for LWK I/O).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::body::Body;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use lab_core::{
    cors_allow_origin, is_loopback_bind, is_mutation_method, validate_path_id, AuthDecision, Config,
    RateLimiter,
};
use lab_rgb::storage::RgbStore;
use lab_rgb::swap::SwapStore;
use serde_json::{json, Value};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;

use crate::http_api::{
    demo_activity, demo_wallets, handle_bfa_audit_post, handle_rgb_issue_post,
    handle_rgb_transfer_post, handle_swap_action_post, handle_swap_init_post, handle_verify_post,
    list_rgb_contracts, list_swap_ids, public_swap_view,
};

#[derive(Clone)]
struct AppState {
    cfg: Config,
    web_dir: PathBuf,
    artifacts_dir: PathBuf,
    verify_limiter: Arc<RateLimiter>,
}

/// Run labd on Axum (blocks the calling thread via Tokio runtime).
pub fn serve(cfg: &Config, bind: &str) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("tokio runtime")?;
    rt.block_on(serve_async(cfg.clone(), bind.to_string()))
}

async fn serve_async(cfg: Config, bind: String) -> Result<()> {
    let sec = cfg.security.clone();
    eprintln!("labd (axum/U5) listening on http://{bind}");
    eprintln!(
        "  U4 security: public_read_only={} loopback_bind={} token_configured={} max_body={}",
        sec.public_read_only,
        is_loopback_bind(&bind),
        sec.api_token.is_some(),
        sec.max_body_bytes
    );
    eprintln!("  GET  /  /demo  /audit  /status  /v1/*");
    if sec.public_read_only {
        eprintln!("  POST (mutations) require Authorization: Bearer <LABD_API_TOKEN>");
    } else {
        eprintln!("  POST /v1/rgb/* · /v1/swap/* · /v1/audit/bfa");
    }
    eprintln!("  (set LABD_HTTP=legacy for handwritten TCP server)");

    let web_dir = PathBuf::from(std::env::var("LABD_WEB_DIR").unwrap_or_else(|_| "web".into()));
    let artifacts_dir = PathBuf::from(
        std::env::var("LABD_ARTIFACTS_DIR").unwrap_or_else(|_| "artifacts/public".into()),
    );
    let state = AppState {
        cfg: cfg.clone(),
        web_dir,
        artifacts_dir,
        verify_limiter: Arc::new(RateLimiter::from_env_verify()),
    };

    let app = router(state.clone()).layer(
        ServiceBuilder::new()
            .layer(TraceLayer::new_for_http())
            .layer(DefaultBodyLimit::max(sec.max_body_bytes)),
    );

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("axum serve")?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(page_index))
        .route("/index.html", get(page_index))
        .route("/demo", get(page_demo))
        .route("/demo.html", get(page_demo))
        .route("/audit", get(page_audit))
        .route("/audit.html", get(page_audit))
        .route("/status", get(page_status))
        .route("/status.html", get(page_status))
        .route("/manifest.json", get(artifact_manifest))
        .route("/artifacts/public/{*rest}", get(artifact_public))
        .route("/v1", get(v1_root))
        .route("/v1/phases", get(v1_phases))
        .route("/v1/health", get(v1_health))
        .route("/v1/networks", get(v1_networks))
        .route("/v1/security", get(v1_security))
        .route("/v1/proofs/{id}", get(v1_proof))
        .route("/v1/swaps", get(v1_swaps))
        .route("/v1/swap/{id}", get(v1_swap_get))
        .route("/v1/swap/init", post(v1_swap_init))
        .route("/v1/swap/{id}/action", post(v1_swap_action))
        .route("/v1/demo/wallets", get(v1_demo_wallets))
        .route("/v1/demo/activity", get(v1_demo_activity))
        .route("/v1/rgb/contracts", get(v1_rgb_contracts))
        .route("/v1/rgb/plans/{id}", get(v1_rgb_plan))
        .route("/v1/rgb/verify", post(v1_rgb_verify))
        .route("/v1/rgb/issue", post(v1_rgb_issue))
        .route("/v1/rgb/transfer", post(v1_rgb_transfer))
        .route("/v1/audit/bfa", post(v1_audit_bfa))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            u4_middleware,
        ))
        .with_state(state)
}

/// U4: CORS echo, mutation auth, OPTIONS short-circuit.
async fn u4_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let acao = cors_allow_origin(&state.cfg.security, origin.as_deref());
    let method = req.method().clone();

    if method == Method::OPTIONS {
        return cors_response(StatusCode::NO_CONTENT, acao.as_deref(), Body::empty());
    }

    if is_mutation_method(method.as_str()) {
        let auth = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        match state.cfg.security.authorize_mutation(auth) {
            AuthDecision::Allow => {}
            AuthDecision::Deny {
                status,
                code,
                message,
            } => {
                let sc = StatusCode::from_u16(status).unwrap_or(StatusCode::FORBIDDEN);
                let body = json!({"error": message, "status": "error", "code": code});
                return cors_json(sc, acao.as_deref(), body);
            }
        }
    }

    let mut res = next.run(req).await;
    apply_cors(res.headers_mut(), acao.as_deref());
    res
}

fn apply_cors(headers: &mut HeaderMap, acao: Option<&str>) {
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Content-Type, Authorization"),
    );
    if let Some(o) = acao {
        if let Ok(v) = HeaderValue::from_str(o) {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, v);
            headers.insert(header::VARY, HeaderValue::from_static("Origin"));
        }
    }
}

fn cors_response(status: StatusCode, acao: Option<&str>, body: Body) -> Response {
    let mut res = Response::builder().status(status).body(body).unwrap();
    apply_cors(res.headers_mut(), acao);
    res
}

fn cors_json(status: StatusCode, acao: Option<&str>, body: Value) -> Response {
    let bytes = serde_json::to_vec(&body).unwrap_or_default();
    let mut res = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(bytes))
        .unwrap();
    apply_cors(res.headers_mut(), acao);
    res
}

fn err_json(status: StatusCode, msg: impl ToString) -> Response {
    (
        status,
        Json(json!({"error": msg.to_string(), "status": "error"})),
    )
        .into_response()
}

fn err_code(status: StatusCode, code: &str, msg: impl ToString) -> Response {
    (
        status,
        Json(json!({"error": msg.to_string(), "status": "error", "code": code})),
    )
        .into_response()
}

async fn read_html(web_dir: &PathBuf, name: &str, fallback: &str) -> Html<String> {
    let path = web_dir.join(name);
    let html = tokio::fs::read_to_string(&path)
        .await
        .unwrap_or_else(|_| fallback.to_string());
    Html(html)
}

async fn page_index(State(s): State<AppState>) -> Html<String> {
    read_html(
        &s.web_dir,
        "index.html",
        "<html><body><h1>rgbmvp verifier</h1><p>missing web/index.html</p></body></html>",
    )
    .await
}

async fn page_demo(State(s): State<AppState>) -> Html<String> {
    read_html(
        &s.web_dir,
        "demo.html",
        "<html><body><h1>/demo</h1><p>missing web/demo.html</p></body></html>",
    )
    .await
}

async fn page_audit(State(s): State<AppState>) -> Html<String> {
    read_html(
        &s.web_dir,
        "audit.html",
        "<html><body><h1>/audit</h1><p>missing web/audit.html</p></body></html>",
    )
    .await
}

async fn page_status(State(s): State<AppState>) -> Html<String> {
    read_html(
        &s.web_dir,
        "status.html",
        "<html><body><h1>/status</h1><p>missing web/status.html</p></body></html>",
    )
    .await
}

async fn artifact_manifest(State(s): State<AppState>) -> Response {
    serve_artifact(&s, "manifest.json").await
}

async fn artifact_public(State(s): State<AppState>, Path(rest): Path<String>) -> Response {
    let name = rest.trim_start_matches('/');
    if name.is_empty() || name.contains("..") || !lab_core::is_safe_path_id(name) {
        if name == "manifest.json" || name == "s3-rgbmvp-live.json" {
            return serve_artifact(&s, name).await;
        }
        // allow known public names even if path rules are strict on dots
        if name.ends_with(".json")
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            return serve_artifact(&s, name).await;
        }
        return err_code(StatusCode::BAD_REQUEST, "bad_path", "bad artifact path");
    }
    serve_artifact(&s, name).await
}

async fn serve_artifact(s: &AppState, name: &str) -> Response {
    let p = s.artifacts_dir.join(name);
    match tokio::fs::read(&p).await {
        Ok(b) => {
            let ct = if name.ends_with(".json") {
                "application/json"
            } else {
                "text/plain; charset=utf-8"
            };
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, ct)
                .body(Body::from(b))
                .unwrap()
        }
        Err(_) => err_json(StatusCode::NOT_FOUND, "artifact not found"),
    }
}

async fn v1_root() -> Response {
    Json(lab_api::root_json()).into_response()
}

async fn v1_phases() -> Response {
    Json(lab_api::phases_json()).into_response()
}

async fn v1_security(State(s): State<AppState>) -> Response {
    Json(lab_api::security_json(
        s.cfg.security.public_read_only,
        is_loopback_bind(&s.cfg.labd_bind),
        s.cfg.security.api_token.is_some(),
    ))
    .into_response()
}

async fn v1_networks() -> Response {
    Json(json!({
        "networks": ["liquid-testnet", "bitcoin-testnet"],
        "default": "liquid-testnet",
        "mainnet": false
    }))
    .into_response()
}

async fn v1_health(State(s): State<AppState>) -> Response {
    let cfg = s.cfg.clone();
    let report = tokio::task::spawn_blocking(move || {
        lab_chain::network_status(&cfg).unwrap_or_else(|e| {
            let mut r = lab_core::HealthReport::phase0_base(cfg.network);
            r.status = "error".into();
            r.checks.push(lab_core::HealthCheck {
                name: "status".into(),
                ok: false,
                detail: Some(e.to_string()),
            });
            r
        })
    })
    .await
    .unwrap_or_else(|e| {
        let mut r = lab_core::HealthReport::phase0_base(s.cfg.network);
        r.status = "error".into();
        r.checks.push(lab_core::HealthCheck {
            name: "join".into(),
            ok: false,
            detail: Some(e.to_string()),
        });
        r
    });
    Json(lab_api::health_json(&report)).into_response()
}

async fn v1_proof(State(s): State<AppState>, Path(id): Path<String>) -> Response {
    if let Err(e) = validate_path_id(&id) {
        return err_code(StatusCode::BAD_REQUEST, "bad_id", e);
    }
    let store = RgbStore::new(&s.cfg.data_dir);
    match store.load_proof(&id) {
        Ok(p) => Json(p).into_response(),
        Err(e) => err_json(StatusCode::NOT_FOUND, e),
    }
}

async fn v1_swaps(State(s): State<AppState>) -> Response {
    let dir = s.cfg.data_dir.clone();
    match tokio::task::spawn_blocking(move || list_swap_ids(&dir)).await {
        Ok(Ok(ids)) => Json(json!({"swaps": ids})).into_response(),
        Ok(Err(e)) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_swap_get(State(s): State<AppState>, Path(id): Path<String>) -> Response {
    if let Err(e) = validate_path_id(&id) {
        return err_code(StatusCode::BAD_REQUEST, "bad_id", e);
    }
    let cfg = s.cfg.clone();
    let id2 = id.clone();
    match tokio::task::spawn_blocking(move || {
        let store = SwapStore::new(&cfg.data_dir);
        store.load(&id2).map(|sess| public_swap_view(&sess, &cfg))
    })
    .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::NOT_FOUND, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_swap_init(State(s): State<AppState>, body: bytes::Bytes) -> Response {
    let cfg = s.cfg.clone();
    let body = String::from_utf8_lossy(&body).into_owned();
    match tokio::task::spawn_blocking(move || {
        let store = SwapStore::new(&cfg.data_dir);
        handle_swap_init_post(&cfg, &store, &body)
    })
    .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::BAD_REQUEST, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_swap_action(
    State(s): State<AppState>,
    Path(id): Path<String>,
    body: bytes::Bytes,
) -> Response {
    if let Err(e) = validate_path_id(&id) {
        return err_code(StatusCode::BAD_REQUEST, "bad_id", e);
    }
    let cfg = s.cfg.clone();
    let body = String::from_utf8_lossy(&body).into_owned();
    match tokio::task::spawn_blocking(move || {
        let store = SwapStore::new(&cfg.data_dir);
        handle_swap_action_post(&cfg, &store, &id, &body)
    })
    .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::BAD_REQUEST, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_demo_wallets(State(s): State<AppState>) -> Response {
    let cfg = s.cfg.clone();
    match tokio::task::spawn_blocking(move || demo_wallets(&cfg)).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_demo_activity(State(s): State<AppState>) -> Response {
    let cfg = s.cfg.clone();
    match tokio::task::spawn_blocking(move || demo_activity(&cfg)).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_rgb_contracts(State(s): State<AppState>) -> Response {
    let cfg = s.cfg.clone();
    match tokio::task::spawn_blocking(move || list_rgb_contracts(&cfg)).await {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_rgb_plan(State(s): State<AppState>, Path(id): Path<String>) -> Response {
    if let Err(e) = validate_path_id(&id) {
        return err_code(StatusCode::BAD_REQUEST, "bad_id", e);
    }
    let store = RgbStore::new(&s.cfg.data_dir);
    match store.load_transfer(&id) {
        Ok(p) => Json(json!({"plan_id": id, "plan": p})).into_response(),
        Err(e) => err_json(StatusCode::NOT_FOUND, e),
    }
}

async fn v1_rgb_verify(
    State(s): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    body: bytes::Bytes,
) -> Response {
    let peer = addr.ip().to_string();
    if !s.verify_limiter.check(&peer) {
        return err_code(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "verify rate limit exceeded; retry later",
        );
    }
    let cfg = s.cfg.clone();
    let body = String::from_utf8_lossy(&body).into_owned();
    match tokio::task::spawn_blocking(move || {
        let store = RgbStore::new(&cfg.data_dir);
        handle_verify_post(&cfg, &store, &body)
    })
    .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::BAD_REQUEST, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_rgb_issue(State(s): State<AppState>, body: bytes::Bytes) -> Response {
    let cfg = s.cfg.clone();
    let body = String::from_utf8_lossy(&body).into_owned();
    match tokio::task::spawn_blocking(move || {
        let store = RgbStore::new(&cfg.data_dir);
        handle_rgb_issue_post(&cfg, &store, &body)
    })
    .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::BAD_REQUEST, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_rgb_transfer(State(s): State<AppState>, body: bytes::Bytes) -> Response {
    let cfg = s.cfg.clone();
    let body = String::from_utf8_lossy(&body).into_owned();
    match tokio::task::spawn_blocking(move || {
        let store = RgbStore::new(&cfg.data_dir);
        handle_rgb_transfer_post(&cfg, &store, &body)
    })
    .await
    {
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => err_json(StatusCode::BAD_REQUEST, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

async fn v1_audit_bfa(body: bytes::Bytes) -> Response {
    let body = String::from_utf8_lossy(&body).into_owned();
    match tokio::task::spawn_blocking(move || handle_bfa_audit_post(&body)).await {
        Ok(Ok(v)) => {
            let status = if v.ok {
                StatusCode::OK
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            (status, Json(v)).into_response()
        }
        Ok(Err(e)) => err_json(StatusCode::BAD_REQUEST, e),
        Err(e) => err_json(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_state() -> AppState {
        let _ = dotenvy::dotenv();
        // Minimal config without full env: use load if possible
        let cfg = Config::load().unwrap_or_else(|_| {
            panic!("Config::load failed in test — set RGBMVP_NETWORK=liquid-testnet")
        });
        AppState {
            cfg,
            web_dir: PathBuf::from("web"),
            artifacts_dir: PathBuf::from("artifacts/public"),
            verify_limiter: Arc::new(RateLimiter::new(100, std::time::Duration::from_secs(60))),
        }
    }

    #[tokio::test]
    async fn catalog_and_security_get() {
        let state = test_state();
        let app = router(state);
        for path in ["/v1", "/v1/security", "/v1/phases", "/v1/networks"] {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK, "path {path}");
        }
    }

    #[tokio::test]
    async fn public_read_only_blocks_post_without_token() {
        std::env::set_var("LABD_PUBLIC_READ_ONLY", "1");
        std::env::remove_var("LABD_API_TOKEN");
        let mut cfg = Config::load().expect("config");
        cfg.security = lab_core::MutationPolicy::from_env(&cfg.labd_bind);
        let state = AppState {
            cfg,
            web_dir: PathBuf::from("web"),
            artifacts_dir: PathBuf::from("artifacts/public"),
            verify_limiter: Arc::new(RateLimiter::new(100, std::time::Duration::from_secs(60))),
        };
        let app = router(state);
        let res = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/swap/init")
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        std::env::remove_var("LABD_PUBLIC_READ_ONLY");
    }
}
