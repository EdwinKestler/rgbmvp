//! U4 public-hosting security primitives (no HTTP stack dependency).

use std::collections::HashMap;
use std::env;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

/// Max path segment length for plan/proof/swap ids.
pub const MAX_PATH_ID_LEN: usize = 128;

/// Default max HTTP body size for labd (bytes).
pub const DEFAULT_MAX_BODY_BYTES: usize = 65_536;

/// Policy for HTTP methods that mutate lab state.
#[derive(Debug, Clone)]
pub struct MutationPolicy {
    /// When true, deny all mutations unless a valid Bearer token is presented
    /// (and `api_token` is configured). Public Cloud Run / Vercel API posture.
    pub public_read_only: bool,
    /// Optional API token (never log). Empty = unset.
    pub api_token: Option<String>,
    /// Bind address used to decide loopback vs public.
    pub bind: String,
    /// Max accepted Content-Length / body size.
    pub max_body_bytes: usize,
    /// CORS allowlist (exact origins). Empty → default localhost only.
    pub cors_origins: Vec<String>,
}

impl MutationPolicy {
    pub fn from_env(bind: &str) -> Self {
        let public_read_only = env_truthy("LABD_PUBLIC_READ_ONLY")
            || env_truthy("PUBLIC_READ_ONLY");
        let api_token = env::var("LABD_API_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let max_body_bytes = env::var("LABD_MAX_BODY_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(DEFAULT_MAX_BODY_BYTES);
        let cors_raw = env::var("LABD_CORS_ORIGINS").unwrap_or_else(|_| {
            "http://127.0.0.1:8080,http://localhost:8080".into()
        });
        let mut cors_origins = parse_cors_origins(&cors_raw);
        // Public mode: never allow wildcard CORS.
        if public_read_only {
            cors_origins.retain(|o| o != "*");
        }
        Self {
            public_read_only,
            api_token,
            bind: bind.to_string(),
            max_body_bytes,
            cors_origins,
        }
    }

    /// Decide whether a request may perform a state-changing method.
    pub fn authorize_mutation(&self, authorization_header: Option<&str>) -> AuthDecision {
        let token_ok = self.api_token.as_ref().is_some_and(|expected| {
            bearer_token_matches(authorization_header, expected)
        });

        if self.public_read_only {
            if token_ok {
                return AuthDecision::Allow;
            }
            return AuthDecision::Deny {
                status: 403,
                code: "public_read_only",
                message: "public labd is read-only; mutations require Authorization: Bearer <LABD_API_TOKEN>",
            };
        }

        // Non-loopback bind without a configured token: refuse mutations.
        if !is_loopback_bind(&self.bind) {
            if self.api_token.is_none() {
                return AuthDecision::Deny {
                    status: 403,
                    code: "token_required",
                    message: "non-loopback bind requires LABD_API_TOKEN for mutations",
                };
            }
            if !token_ok {
                return AuthDecision::Deny {
                    status: 401,
                    code: "unauthorized",
                    message: "missing or invalid Authorization: Bearer token",
                };
            }
        } else if self.api_token.is_some() && !token_ok {
            // Loopback with token configured: still require it (operator lock).
            return AuthDecision::Deny {
                status: 401,
                code: "unauthorized",
                message: "missing or invalid Authorization: Bearer token",
            };
        }

        AuthDecision::Allow
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthDecision {
    Allow,
    Deny {
        status: u16,
        code: &'static str,
        message: &'static str,
    },
}

fn env_truthy(key: &str) -> bool {
    env::var(key)
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            t == "1" || t == "true" || t == "yes" || t == "on"
        })
        .unwrap_or(false)
}

/// Parse comma-separated CORS origins.
pub fn parse_cors_origins(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Pick Access-Control-Allow-Origin value for a request Origin header.
/// Returns `None` if the origin is not allowed (omit ACAO or echo nothing useful).
pub fn cors_allow_origin(policy: &MutationPolicy, request_origin: Option<&str>) -> Option<String> {
    let Some(origin) = request_origin.map(str::trim).filter(|o| !o.is_empty()) else {
        // Non-browser clients: no ACAO needed; allow simple *
        // only for non-public operator default when origins include *.
        if !policy.public_read_only && policy.cors_origins.iter().any(|o| o == "*") {
            return Some("*".into());
        }
        return None;
    };
    if policy.cors_origins.iter().any(|o| o == "*") && !policy.public_read_only {
        return Some("*".into());
    }
    if policy.cors_origins.iter().any(|o| o == origin) {
        return Some(origin.to_string());
    }
    None
}

/// True if bind string is loopback (127.0.0.1 / ::1 / localhost).
pub fn is_loopback_bind(bind: &str) -> bool {
    let host = bind
        .rsplit_once(':')
        .map(|(h, _)| h.trim().trim_start_matches('[').trim_end_matches(']'))
        .unwrap_or(bind.trim());
    matches!(
        host,
        "127.0.0.1" | "localhost" | "::1" | "0:0:0:0:0:0:0:1"
    )
}

/// GET and OPTIONS are non-mutating for U4.
pub fn is_mutation_method(method: &str) -> bool {
    !matches!(method.to_ascii_uppercase().as_str(), "GET" | "HEAD" | "OPTIONS")
}

/// Path id: alnum + `._~-` only, length 1..=MAX_PATH_ID_LEN.
pub fn is_safe_path_id(id: &str) -> bool {
    let id = id.trim();
    if id.is_empty() || id.len() > MAX_PATH_ID_LEN {
        return false;
    }
    id.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '~'))
}

pub fn validate_path_id(id: &str) -> Result<()> {
    if is_safe_path_id(id) {
        Ok(())
    } else {
        bail!(
            "invalid id {id:?}: use 1..={MAX_PATH_ID_LEN} chars [A-Za-z0-9._~-]"
        );
    }
}

/// Constant-time equality for equal-length secrets; length mismatch → false.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

fn bearer_token_matches(authorization: Option<&str>, expected: &str) -> bool {
    let Some(raw) = authorization.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    let token = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .unwrap_or(raw)
        .trim();
    constant_time_eq(token.as_bytes(), expected.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_id_rules() {
        assert!(is_safe_path_id("s3-20260722-1251"));
        assert!(is_safe_path_id("plan.foo_bar~1"));
        assert!(!is_safe_path_id(""));
        assert!(!is_safe_path_id("../etc/passwd"));
        assert!(!is_safe_path_id("a/b"));
        assert!(!is_safe_path_id(&"x".repeat(MAX_PATH_ID_LEN + 1)));
    }

    #[test]
    fn loopback_detect() {
        assert!(is_loopback_bind("127.0.0.1:8080"));
        assert!(is_loopback_bind("localhost:8080"));
        assert!(is_loopback_bind("[::1]:8080"));
        assert!(!is_loopback_bind("0.0.0.0:8080"));
        assert!(!is_loopback_bind("10.0.0.5:8080"));
    }

    #[test]
    fn constant_time_eq_ok() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"Secret"));
        assert!(!constant_time_eq(b"a", b"ab"));
    }

    #[test]
    fn public_read_only_denies_without_token() {
        let p = MutationPolicy {
            public_read_only: true,
            api_token: Some("tok".into()),
            bind: "0.0.0.0:8080".into(),
            max_body_bytes: 1000,
            cors_origins: vec!["https://example.com".into()],
        };
        assert!(matches!(
            p.authorize_mutation(None),
            AuthDecision::Deny { code: "public_read_only", .. }
        ));
        assert_eq!(
            p.authorize_mutation(Some("Bearer tok")),
            AuthDecision::Allow
        );
    }

    #[test]
    fn non_loopback_requires_token_configured() {
        let p = MutationPolicy {
            public_read_only: false,
            api_token: None,
            bind: "0.0.0.0:8080".into(),
            max_body_bytes: 1000,
            cors_origins: vec![],
        };
        assert!(matches!(
            p.authorize_mutation(None),
            AuthDecision::Deny { code: "token_required", .. }
        ));
    }

    #[test]
    fn loopback_open_without_token() {
        let p = MutationPolicy {
            public_read_only: false,
            api_token: None,
            bind: "127.0.0.1:8080".into(),
            max_body_bytes: 1000,
            cors_origins: vec![],
        };
        assert_eq!(p.authorize_mutation(None), AuthDecision::Allow);
    }

    #[test]
    fn cors_public_no_star() {
        let p = MutationPolicy {
            public_read_only: true,
            api_token: None,
            bind: "0.0.0.0:8080".into(),
            max_body_bytes: 1000,
            cors_origins: vec!["https://demo.example".into()],
        };
        assert_eq!(
            cors_allow_origin(&p, Some("https://demo.example")),
            Some("https://demo.example".into())
        );
        assert_eq!(cors_allow_origin(&p, Some("https://evil.example")), None);
    }

    #[test]
    fn rate_limit_window() {
        let lim = RateLimiter::new(3, Duration::from_secs(60));
        assert!(lim.check("1.1.1.1"));
        assert!(lim.check("1.1.1.1"));
        assert!(lim.check("1.1.1.1"));
        assert!(!lim.check("1.1.1.1"));
        assert!(lim.check("2.2.2.2"));
    }
}

/// Simple in-process sliding-window rate limiter (per key, e.g. peer IP).
#[derive(Debug)]
pub struct RateLimiter {
    max: usize,
    window: Duration,
    hits: Mutex<HashMap<String, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(max: usize, window: Duration) -> Self {
        Self {
            max: max.max(1),
            window,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// From env: `LABD_VERIFY_RATE_LIMIT` (default 30) per minute.
    pub fn from_env_verify() -> Self {
        let max = env::var("LABD_VERIFY_RATE_LIMIT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);
        Self::new(max, Duration::from_secs(60))
    }

    /// Returns true if the request is allowed.
    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut guard = self.hits.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard.entry(key.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        if entry.len() >= self.max {
            return false;
        }
        entry.push(now);
        true
    }
}
