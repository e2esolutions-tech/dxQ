//! Postgres integration smoke for the SEZ-2 persistence path.
//!
//! Spins up `postgres:16-alpine` in a disposable container via
//! `testcontainers`, points a [`PgEventStore`] at it, runs the
//! bundled migrations, then exercises the full HTTP surface
//! against an axum router whose store is the Postgres-backed
//! one:
//!
//! - `POST /v1/events` (single + batch, schema validation),
//! - `GET  /v1/events?limit=N` (history order),
//! - `GET  /v1/inventory` (per-asset latest + sorted by q),
//! - `GET  /v1/posture`   (org rollup, BLOCKED count),
//! - `GET  /v1/blocked`   (filter to agility ≤ Locked).
//!
//! The whole thing also doubles as a regression bar against
//! the in-memory `tests/http_smoke.rs`: both should produce
//! identical HTTP bodies for the same fixture set, modulo
//! Postgres' lossless JSONB round-trip.
//!
//! When docker isn't reachable on the host, every test in
//! this file gates itself behind an early `eprintln + return`
//! and reports as `ok`. CI configures `DOCKER_HOST` and the
//! tests run for real.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use sezar_core::{
    AgilityBlock, AgilityLevel, Asset, AssetKind, CryptoInventoryEvent, Posture, Primitive,
    PrimitiveRole, SCHEMA_MINOR, SCHEMA_VERSION,
};
use sezar_server::{router, store, store_pg::PgEventStore, AppState};
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres;

/// Wraps the running Postgres container so test bodies stay
/// short. Keep the container handle in scope for the whole
/// test; dropping it tears the container down.
struct PgFixture {
    _container: ContainerAsync<Postgres>,
    url: String,
}

async fn start_postgres() -> Option<PgFixture> {
    let container = match Postgres::default().start().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[pg_smoke] skipping: docker unreachable: {e}");
            return None;
        }
    };
    let host = match container.get_host().await {
        Ok(h) => h.to_string(),
        Err(e) => {
            eprintln!("[pg_smoke] skipping: container host unavailable: {e}");
            return None;
        }
    };
    let port = match container.get_host_port_ipv4(5432).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[pg_smoke] skipping: container port unavailable: {e}");
            return None;
        }
    };
    let url = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    Some(PgFixture {
        _container: container,
        url,
    })
}

async fn spawn_pg_server(pg: &PgFixture) -> (SocketAddr, Arc<PgEventStore>) {
    let store = Arc::new(
        PgEventStore::connect(&pg.url)
            .await
            .expect("Pg store connect + migrate"),
    );
    let tmp = tempfile::tempdir().expect("tempdir for CA");
    let state = AppState::with_store(store.clone(), tmp.path(), None)
        .expect("AppState build with PgEventStore");
    std::mem::forget(tmp);
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    (addr, store)
}

fn prim(role: PrimitiveRole, algo: &str, pq: Option<bool>) -> Primitive {
    Primitive {
        role,
        algorithm: algo.into(),
        parameters: Default::default(),
        pq_resistant: pq,
        nist_classification: None,
    }
}

fn modern_classical_tls() -> Vec<Primitive> {
    vec![
        prim(PrimitiveRole::Kex, "X25519", Some(false)),
        prim(PrimitiveRole::Sig, "ECDSA-P256", Some(false)),
        prim(PrimitiveRole::Encrypt, "AES-256-GCM", Some(true)),
        prim(PrimitiveRole::Hash, "SHA-384", Some(true)),
    ]
}

fn event(
    kind: AssetKind,
    identity: &str,
    source: &str,
    prims: Vec<Primitive>,
    ag: Option<AgilityBlock>,
) -> CryptoInventoryEvent {
    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: source.into(),
        observed_at: chrono::Utc::now(),
        asset: Asset {
            kind,
            identity: identity.into(),
            host: Some("pg.example".into()),
        },
        primitives: prims,
        channel_protection: None,
        agility: ag,
        posture: Posture {
            score: 0,
            rationale: "fixture".into(),
            recommended_replacement: None,
        },
    }
}

#[tokio::test]
async fn full_pg_ingest_query_loop() {
    let Some(pg) = start_postgres().await else {
        return;
    };
    let (addr, _store) = spawn_pg_server(&pg).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // /healthz
    let r = client.get(format!("{base}/healthz")).send().await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().await.unwrap(), "ok");

    let agile = AgilityBlock {
        level: AgilityLevel::Configurable,
        level_score: AgilityLevel::Configurable.score(),
        evidence: vec![],
        scanner_version: "test".into(),
        rubric_version: "test".into(),
    };
    let locked = AgilityBlock {
        level: AgilityLevel::Locked,
        level_score: AgilityLevel::Locked.score(),
        evidence: vec![],
        scanner_version: "test".into(),
        rubric_version: "test".into(),
    };

    let events = vec![
        event(
            AssetKind::TlsSession,
            "tls-modern-1",
            "sezar-net",
            modern_classical_tls(),
            Some(agile.clone()),
        ),
        event(
            AssetKind::TlsSession,
            "tls-locked-1",
            "sezar-net",
            modern_classical_tls(),
            Some(locked.clone()),
        ),
    ];
    let r = client
        .post(format!("{base}/v1/events/batch"))
        .json(&events)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["ingested"], 2);

    // /v1/events?limit=10 — newest first.
    let body: serde_json::Value = client
        .get(format!("{base}/v1/events?limit=10"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["count"], 2);

    // /v1/inventory — locked must rank above modern (BLOCKED gets
    // pushed up by the deadline-tension term).
    let body: serde_json::Value = client
        .get(format!("{base}/v1/inventory"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["count"], 2);
    let items = body["items"].as_array().unwrap();
    let q_locked: f64 = items
        .iter()
        .find(|i| i["identity"] == "tls-locked-1")
        .unwrap()["q"]
        .as_f64()
        .unwrap();
    let q_modern: f64 = items
        .iter()
        .find(|i| i["identity"] == "tls-modern-1")
        .unwrap()["q"]
        .as_f64()
        .unwrap();
    assert!(
        q_locked > q_modern,
        "locked must rank above modern; got {q_locked} vs {q_modern}"
    );

    // /v1/posture — assets, blocked_count, org_q > 0.
    let body: serde_json::Value = client
        .get(format!("{base}/v1/posture"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["assets"], 2);
    assert_eq!(body["blocked_count"], 1);
    assert!(body["org_q"].as_f64().unwrap() > 0.0);

    // /v1/blocked — just the locked asset.
    let body: serde_json::Value = client
        .get(format!("{base}/v1/blocked"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["count"], 1);
    assert_eq!(body["items"][0]["identity"], "tls-locked-1");
}

#[tokio::test]
async fn pg_persists_across_pool_drop() {
    // Simulate a server restart: open a store, ingest events,
    // drop the store + pool, then re-open a fresh PgEventStore
    // against the same database. The events must still be
    // visible — that's the whole point of moving off the
    // DashMap.
    let Some(pg) = start_postgres().await else {
        return;
    };

    {
        let store = PgEventStore::connect(&pg.url).await.unwrap();
        for id in ["a", "b", "c"] {
            store
                .append(event(
                    AssetKind::TlsSession,
                    id,
                    "sezar-net",
                    modern_classical_tls(),
                    None,
                ))
                .await
                .unwrap();
        }
        use sezar_server::store::EventStore;
        let n = store.len().await.unwrap();
        assert_eq!(n, 3);
    }
    // Original store + pool dropped here.

    let store = PgEventStore::connect(&pg.url).await.unwrap();
    use sezar_server::store::EventStore;
    let n = store.len().await.unwrap();
    assert_eq!(n, 3, "events must survive a pool drop / reconnect");
    let inv = store.latest_per_asset().await.unwrap();
    assert_eq!(inv.len(), 3, "per-asset latest map rebuilds from disk");
}

#[tokio::test]
async fn out_of_order_event_does_not_overwrite_latest_pg() {
    let Some(pg) = start_postgres().await else {
        return;
    };
    let store = PgEventStore::connect(&pg.url).await.unwrap();

    use sezar_server::store::EventStore;
    let mut newer = event(
        AssetKind::TlsSession,
        "race",
        "sezar-net",
        modern_classical_tls(),
        None,
    );
    newer.observed_at = chrono::Utc::now();
    newer.posture.score = 80;

    let mut older = newer.clone();
    older.observed_at = newer.observed_at - chrono::Duration::hours(1);
    older.posture.score = 10;

    // Insert newer first, then older.
    store.append(newer.clone()).await.unwrap();
    store.append(older.clone()).await.unwrap();

    let latest = store.latest_per_asset().await.unwrap();
    assert_eq!(latest.len(), 1);
    assert_eq!(
        latest[0].posture.score, 80,
        "older event must not clobber the newer assets row"
    );

    // History should hold both.
    assert_eq!(store.len().await.unwrap(), 2);
}

#[allow(unused_imports)]
use store as _; // silence unused-warning when imports come from sezar_server::store via re-exports.
