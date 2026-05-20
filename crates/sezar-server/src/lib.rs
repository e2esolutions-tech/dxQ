//! `sezar-server` — collector library.
//!
//! The library exposes the Axum [`Router`] and the in-memory state
//! type behind it. The binary entry-point (`bin/main.rs`) wires
//! these together with CLI flags. Splitting library from binary
//! lets integration tests spin the whole server up against an
//! ephemeral port without going through the CLI.
//!
//! # V1 surface
//!
//! ```text
//! POST /v1/events                    ingest a single event (JSON body)
//! POST /v1/events/batch              ingest an array of events
//! GET  /v1/events?limit=N            paginated event log (most recent first)
//! GET  /v1/inventory                 deduplicated asset list with latest posture
//! GET  /v1/posture                   org-level rollup under default weights
//! GET  /v1/qkd/links                 QKD links observed by sezar-qkd
//! GET  /v1/blocked                   assets flagged BLOCKED (low agility)
//! POST /v1/enrol                     redeem a bootstrap token → agent cert
//! POST /v1/admin/bootstrap-tokens    admin-only: mint a new bootstrap token
//! GET  /healthz                      liveness probe
//! ```
//!
//! Storage is in-memory (DashMap) in V1; Postgres swap-in lives at
//! `store::EventStore` so the upgrade is a single trait swap.

#![warn(rust_2018_idioms)]

pub mod ca;
pub mod enrol;
pub mod posture;
pub mod routes;
pub mod store;
pub mod tls;

use std::sync::Arc;

use axum::{
    routing::{get, post},
    Router,
};

/// Shared application state injected into every handler.
#[derive(Clone)]
pub struct AppState {
    /// Event store. Trait-object so the Postgres backend can swap in.
    pub store: Arc<store::EventStore>,
    /// Default deadline used by org-level rollup. May be operator-tunable.
    pub default_deadline: chrono::DateTime<chrono::Utc>,
    /// Horizon constant (years) for deadline-tension computation.
    pub horizon_years: f32,
    /// Internal CA — signs agent certs at enrolment time.
    pub ca: ca::Ca,
    /// One-time bootstrap-token store. Tokens are admin-issued
    /// and consumed on first successful `/v1/enrol`.
    pub tokens: Arc<enrol::BootstrapTokenStore>,
    /// Admin secret expected in `X-Admin-Token` on
    /// `/v1/admin/bootstrap-tokens`. When `None`, the admin
    /// endpoint short-circuits to 503 — operators must boot
    /// with `--admin-token` (or `SEZAR_ADMIN_TOKEN`) to enable
    /// token issuance.
    pub admin_token: Option<String>,
}

impl AppState {
    /// Construct an in-memory state with defaults matching the paper
    /// (NSA CNSA 2.0 browser/server class deadline). The CA is
    /// loaded from (or generated into) `ca_dir`; `admin_token` is
    /// used for `/v1/admin/bootstrap-tokens` auth.
    pub fn new_in_memory(
        ca_dir: &std::path::Path,
        admin_token: Option<String>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            store: Arc::new(store::EventStore::new_in_memory()),
            default_deadline: chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            horizon_years: 5.0,
            ca: ca::Ca::load_or_init(ca_dir)?,
            tokens: enrol::BootstrapTokenStore::new(),
            admin_token,
        })
    }
}

/// Combined router carrying every V1 route. This is the
/// plain-HTTP entry point used when `--tls` is off (default
/// for development, the in-process integration tests, and the
/// `scripts/acceptance.sh` smoke). When `--tls` is on the
/// server boots [`router_main`] and [`router_bootstrap`] on
/// separate listeners instead — see those for the rationale.
pub fn router(state: AppState) -> Router {
    router_bootstrap(state.clone()).merge(router_main(state))
}

/// Bootstrap-side routes: reachable on the TLS-without-client-
/// cert listener so un-enrolled agents can still pick up a
/// cert. Carries `/healthz`, `/v1/enrol`,
/// `/v1/admin/bootstrap-tokens`.
pub fn router_bootstrap(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(routes::healthz))
        .route("/v1/enrol", post(enrol::enrol))
        .route(
            "/v1/admin/bootstrap-tokens",
            post(enrol::issue_bootstrap_token),
        )
        .with_state(state)
}

/// Main routes: served behind the mTLS listener when `--tls` is
/// on. A successful TLS handshake on this listener already
/// proved the peer holds a CA-signed client cert, so handlers
/// don't need to re-check.
pub fn router_main(state: AppState) -> Router {
    Router::new()
        .route("/v1/events", post(routes::ingest_one).get(routes::list_events))
        .route("/v1/events/batch", post(routes::ingest_batch))
        .route("/v1/inventory", get(routes::inventory))
        .route("/v1/posture", get(routes::org_posture))
        .route("/v1/qkd/links", get(routes::qkd_links))
        .route("/v1/blocked", get(routes::blocked_assets))
        .with_state(state)
}
