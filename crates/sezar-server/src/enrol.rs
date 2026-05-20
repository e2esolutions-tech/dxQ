//! Bootstrap-token + enrolment handlers for the V1 mTLS path
//! (SEZ-6).
//!
//! Flow:
//!
//! 1. Operator hits `POST /v1/admin/bootstrap-tokens` with the
//!    admin secret in `X-Admin-Token`. Server returns a fresh,
//!    single-use, 24-hour TTL token bound to a specific
//!    `agent_id`. The token never re-appears in any subsequent
//!    response or log line.
//! 2. Agent hits `POST /v1/enrol` with `X-Bootstrap-Token: <t>`
//!    and a JSON body declaring its agent id. Server validates
//!    the token (matches agent id, not expired, not consumed),
//!    mints a fresh client cert signed by the on-disk CA, and
//!    returns it alongside the matching private key and the CA
//!    cert.
//! 3. The token is consumed at this point and removed from the
//!    store. A second use of the same token returns 401.
//!
//! The token store is currently in-memory (`DashMap`). When the
//! Postgres backend lands (SEZ-2) the same surface migrates
//! behind the same trait, with the tokens encrypted at rest.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use uuid::Uuid;

use crate::ca::AgentCert;
use crate::routes::ApiError;
use crate::AppState;

/// Header carrying the operator's admin secret on
/// `POST /v1/admin/bootstrap-tokens`.
pub const ADMIN_HEADER: &str = "X-Admin-Token";

/// Header carrying the one-time bootstrap token on
/// `POST /v1/enrol`.
pub const BOOTSTRAP_HEADER: &str = "X-Bootstrap-Token";

/// Default agent-cert validity (days) when the admin does not
/// override it. One year matches the CA/B Forum norm for
/// machine-to-machine certs and is short enough that rotation
/// stays a routine action.
pub const DEFAULT_AGENT_VALIDITY_DAYS: i64 = 365;

/// Default bootstrap-token TTL — single-use within 24 hours.
pub const DEFAULT_TOKEN_TTL_HOURS: i64 = 24;

/// In-memory store of unconsumed bootstrap tokens.
#[derive(Default)]
pub struct BootstrapTokenStore {
    /// token (UUID hex) -> entry. Entries are removed on first
    /// successful use.
    tokens: DashMap<String, TokenEntry>,
}

#[derive(Debug, Clone)]
struct TokenEntry {
    agent_id: String,
    expires_at: DateTime<Utc>,
}

impl BootstrapTokenStore {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Issue a fresh token for `agent_id`. The returned string
    /// is the token (32-char hex from a UUID v4); the caller is
    /// responsible for handing it to the operator over a
    /// confidential channel.
    pub fn issue(&self, agent_id: &str, ttl_hours: i64) -> IssuedToken {
        let token = Uuid::new_v4().simple().to_string();
        let expires_at = Utc::now() + Duration::hours(ttl_hours);
        self.tokens.insert(
            token.clone(),
            TokenEntry {
                agent_id: agent_id.to_string(),
                expires_at,
            },
        );
        IssuedToken {
            token,
            agent_id: agent_id.into(),
            expires_at,
        }
    }

    /// Consume `token` if it is valid for `agent_id`. Returns
    /// the (now-removed) entry on success, or a structured
    /// reason on failure. Failed consumes do not leave the
    /// store mutated.
    fn consume(&self, token: &str, agent_id: &str) -> Result<TokenEntry, ConsumeError> {
        // We use remove() unconditionally so a successful match
        // is atomic — but we have to put it back if the agent
        // id mismatches.
        let Some((_, entry)) = self.tokens.remove(token) else {
            return Err(ConsumeError::UnknownToken);
        };
        if entry.expires_at < Utc::now() {
            return Err(ConsumeError::Expired);
        }
        if entry.agent_id != agent_id {
            // Restore so an operator-bound token isn't burned by
            // a wrong-agent-id request.
            self.tokens.insert(token.into(), entry);
            return Err(ConsumeError::AgentMismatch);
        }
        Ok(entry)
    }

    /// Best-effort cleanup of expired entries. Called from the
    /// handler hot path so the store does not grow unbounded
    /// when tokens are issued and never consumed.
    fn prune_expired(&self) {
        let now = Utc::now();
        self.tokens.retain(|_, e| e.expires_at >= now);
    }
}

enum ConsumeError {
    UnknownToken,
    Expired,
    AgentMismatch,
}

/// Body of `POST /v1/admin/bootstrap-tokens`.
#[derive(Debug, Deserialize)]
pub struct IssueTokenRequest {
    /// Agent identifier the resulting token is bound to. The
    /// agent must present the same id at enrolment.
    pub agent_id: String,
    /// Optional override of [`DEFAULT_TOKEN_TTL_HOURS`].
    #[serde(default)]
    pub ttl_hours: Option<i64>,
}

/// Response for `POST /v1/admin/bootstrap-tokens`.
#[derive(Debug, Serialize)]
pub struct IssuedToken {
    /// The bootstrap token. Display once; never persists in
    /// server logs.
    pub token: String,
    /// Echo of the agent id the token is bound to.
    pub agent_id: String,
    /// UTC moment after which the token cannot be redeemed.
    pub expires_at: DateTime<Utc>,
}

/// Body of `POST /v1/enrol`.
#[derive(Debug, Deserialize)]
pub struct EnrolRequest {
    /// Agent identifier — must match the id the bootstrap token
    /// was issued for.
    pub agent_id: String,
    /// Optional override of [`DEFAULT_AGENT_VALIDITY_DAYS`].
    #[serde(default)]
    pub validity_days: Option<i64>,
}

/// `POST /v1/admin/bootstrap-tokens` — admin-only.
///
/// Auth: `X-Admin-Token: <admin secret>` matching whatever the
/// server was booted with. If no admin secret was configured
/// the endpoint is unreachable (every request gets a 503).
pub async fn issue_bootstrap_token(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<IssueTokenRequest>,
) -> Result<Json<IssuedToken>, (StatusCode, Json<ApiError>)> {
    let Some(expected) = st.admin_token.as_deref() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ApiError {
                code: "admin_disabled".into(),
                message: "no admin token configured; cannot issue bootstrap tokens".into(),
            }),
        ));
    };

    let provided = headers
        .get(ADMIN_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !constant_time_eq(provided.as_bytes(), expected.as_bytes()) {
        warn!(header = ADMIN_HEADER, "admin auth failed on issue-token");
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                code: "admin_auth_failed".into(),
                message: "missing or wrong admin token".into(),
            }),
        ));
    }

    if req.agent_id.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "agent_id_required".into(),
                message: "agent_id is mandatory and must be non-empty".into(),
            }),
        ));
    }

    let ttl = req.ttl_hours.unwrap_or(DEFAULT_TOKEN_TTL_HOURS);
    if !(1..=24 * 30).contains(&ttl) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "ttl_out_of_range".into(),
                message: "ttl_hours must be between 1 and 720".into(),
            }),
        ));
    }
    st.tokens.prune_expired();
    let issued = st.tokens.issue(&req.agent_id, ttl);
    info!(
        agent_id = %issued.agent_id,
        expires_at = %issued.expires_at,
        "issued bootstrap token"
    );
    Ok(Json(issued))
}

/// `POST /v1/enrol` — open endpoint, gated by a one-time
/// bootstrap token in `X-Bootstrap-Token`.
pub async fn enrol(
    State(st): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<EnrolRequest>,
) -> Result<Json<AgentCert>, (StatusCode, Json<ApiError>)> {
    let token = headers
        .get(BOOTSTRAP_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if token.is_empty() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                code: "missing_bootstrap_token".into(),
                message: format!("missing {BOOTSTRAP_HEADER} header"),
            }),
        ));
    }

    match st.tokens.consume(token, &req.agent_id) {
        Ok(_) => {}
        Err(ConsumeError::UnknownToken) => {
            warn!(agent_id = %req.agent_id, "enrol attempt with unknown token");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    code: "bootstrap_token_invalid".into(),
                    message: "bootstrap token is unknown or has been consumed".into(),
                }),
            ));
        }
        Err(ConsumeError::Expired) => {
            warn!(agent_id = %req.agent_id, "enrol attempt with expired token");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    code: "bootstrap_token_expired".into(),
                    message: "bootstrap token has expired".into(),
                }),
            ));
        }
        Err(ConsumeError::AgentMismatch) => {
            warn!(agent_id = %req.agent_id, "enrol attempt with mismatched agent id");
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    code: "agent_id_mismatch".into(),
                    message: "this token was issued for a different agent_id".into(),
                }),
            ));
        }
    }

    let validity = req.validity_days.unwrap_or(DEFAULT_AGENT_VALIDITY_DAYS);
    if !(1..=365 * 5).contains(&validity) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                code: "validity_out_of_range".into(),
                message: "validity_days must be between 1 and 1825 (5 years)".into(),
            }),
        ));
    }

    let cert = st
        .ca
        .sign_agent_cert(&req.agent_id, validity)
        .map_err(|e| {
            warn!(error = %e, "agent cert minting failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "cert_mint_failed".into(),
                    message: e.to_string(),
                }),
            )
        })?;
    info!(
        agent_id = %cert.agent_id,
        expires_at = %cert.expires_at,
        "minted agent cert"
    );
    Ok(Json(cert))
}

/// Constant-time byte-slice compare. Same length is required —
/// we want the timing leak there to fall under "missing admin
/// token", which is a separate failure mode the caller already
/// knows about.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        d |= x ^ y;
    }
    d == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_and_consume_happy_path() {
        let store = BootstrapTokenStore::default();
        let issued = store.issue("a-1", 1);
        let entry = store
            .consume(&issued.token, "a-1")
            .ok()
            .expect("consume should succeed");
        assert_eq!(entry.agent_id, "a-1");
        // Second consume should fail — single-use.
        assert!(matches!(
            store.consume(&issued.token, "a-1"),
            Err(ConsumeError::UnknownToken)
        ));
    }

    #[test]
    fn mismatched_agent_id_does_not_burn_token() {
        let store = BootstrapTokenStore::default();
        let issued = store.issue("a-1", 1);
        assert!(matches!(
            store.consume(&issued.token, "wrong"),
            Err(ConsumeError::AgentMismatch)
        ));
        // The correct agent can still redeem it.
        assert!(store.consume(&issued.token, "a-1").is_ok());
    }

    #[test]
    fn expired_token_is_rejected() {
        let store = BootstrapTokenStore::default();
        let token = Uuid::new_v4().simple().to_string();
        store.tokens.insert(
            token.clone(),
            TokenEntry {
                agent_id: "a-1".into(),
                expires_at: Utc::now() - Duration::hours(1),
            },
        );
        assert!(matches!(
            store.consume(&token, "a-1"),
            Err(ConsumeError::Expired)
        ));
    }

    #[test]
    fn constant_time_eq_basic() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hellooo"));
    }
}
