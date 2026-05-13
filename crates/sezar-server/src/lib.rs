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
//! POST /v1/events             ingest a single event (JSON body)
//! POST /v1/events/batch       ingest an array of events
//! GET  /v1/events?limit=N     paginated event log (most recent first)
//! GET  /v1/inventory          deduplicated asset list with latest posture
//! GET  /v1/posture            org-level rollup under default weights
//! GET  /v1/qkd/links          QKD links observed by sezar-qkd
//! GET  /v1/blocked            assets flagged BLOCKED (low agility)
//! GET  /healthz               liveness probe
//! ```
//!
//! Storage is in-memory (DashMap) in V1; Postgres swap-in lives at
//! `store::EventStore` so the upgrade is a single trait swap.

#![warn(rust_2018_idioms)]

pub mod posture;
pub mod routes;
pub mod store;

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
}

impl AppState {
    /// Construct an in-memory state with defaults matching the paper
    /// (NSA CNSA 2.0 browser/server class deadline).
    pub fn new_in_memory() -> Self {
        Self {
            store: Arc::new(store::EventStore::new_in_memory()),
            default_deadline: chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            horizon_years: 5.0,
        }
    }
}

/// Build the Axum router carrying [`AppState`].
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(routes::healthz))
        .route("/v1/events", post(routes::ingest_one).get(routes::list_events))
        .route("/v1/events/batch", post(routes::ingest_batch))
        .route("/v1/inventory", get(routes::inventory))
        .route("/v1/posture", get(routes::org_posture))
        .route("/v1/qkd/links", get(routes::qkd_links))
        .route("/v1/blocked", get(routes::blocked_assets))
        .with_state(state)
}
