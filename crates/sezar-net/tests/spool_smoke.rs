//! Integration smoke for the SEZ-6 spool path.
//!
//! Models the "server down, agent buffers, agent recovers"
//! sequence:
//!
//! 1. With no live server, three events are pushed straight
//!    into the on-disk spool via `Spool::append`. (In the real
//!    agent this happens inside `Sink::send` when a `POST`
//!    fails.)
//! 2. We bring sezar-server's router up in-process on an
//!    ephemeral port — same pattern the existing end-to-end
//!    smoke uses.
//! 3. `Spool::drain` walks every spooled line and POSTs it to
//!    `/v1/events` via a blocking reqwest client, the exact
//!    closure shape the binary's `Sink::drain_spool` uses.
//! 4. The spool ends up empty and `/v1/events?limit=10`
//!    reports all three events on the server side.
//!
//! This exercises the SEZ-6 fourth acceptance criterion (no
//! events lost across a server outage) without depending on
//! the binary, libpcap, or any spawned process.

use std::net::SocketAddr;
use std::time::Duration;

use sezar_core::{
    Asset, AssetKind, CryptoInventoryEvent, Posture, Primitive, PrimitiveRole, SCHEMA_MINOR,
    SCHEMA_VERSION,
};
use sezar_net::spool::Spool;
use sezar_server::{router, AppState};

async fn spawn_server() -> SocketAddr {
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = AppState::new_in_memory(tmp.path(), None).expect("AppState init");
    std::mem::forget(tmp);
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

fn fixture(identity: &str) -> CryptoInventoryEvent {
    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: "spool-smoke".into(),
        observed_at: chrono::Utc::now(),
        asset: Asset {
            kind: AssetKind::TlsSession,
            identity: identity.into(),
            host: Some("spool.example".into()),
        },
        primitives: vec![Primitive {
            role: PrimitiveRole::Kex,
            algorithm: "X25519MLKEM768".into(),
            parameters: Default::default(),
            pq_resistant: Some(true),
            nist_classification: None,
        }],
        channel_protection: None,
        agility: None,
        posture: Posture {
            score: 0,
            rationale: "spool-smoke".into(),
            recommended_replacement: None,
        },
    }
}

#[tokio::test]
async fn server_outage_buffers_then_drains_on_recovery() {
    // Pretend the server is down: stash three events in the
    // spool directly. In the real agent this is what
    // `Sink::send` does on POST failure.
    let spool_dir = tempfile::tempdir().expect("spool tempdir");
    let spool = Spool::open(spool_dir.path()).unwrap();
    for id in ["a", "b", "c"] {
        spool.append(&fixture(id)).unwrap();
    }
    assert_eq!(spool.len().unwrap(), 3, "spool should hold all three");

    // Bring the server up. Drain across an actual HTTP loop
    // through reqwest::blocking, the same shape the binary's
    // `Sink::drain_spool` runs.
    let addr = spawn_server().await;
    let url = format!("http://{addr}/v1/events");

    let stats = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::new();
        spool
            .drain(|ev| match client.post(&url).json(ev).send() {
                Ok(r) if r.status().is_success() => Ok(()),
                Ok(r) => Err(anyhow::anyhow!("status {}", r.status())),
                Err(e) => Err(anyhow::anyhow!(e)),
            })
            .unwrap()
    })
    .await
    .unwrap();

    assert_eq!(stats.seen, 3);
    assert_eq!(stats.delivered, 3);
    assert_eq!(stats.retained, 0);
    assert_eq!(stats.corrupt_dropped, 0);

    // Spool should be drained.
    let spool = Spool::open(spool_dir.path()).unwrap();
    assert_eq!(spool.len().unwrap(), 0, "spool should be empty");

    // Server should hold the three events.
    let body: serde_json::Value = reqwest::Client::new()
        .get(format!("http://{addr}/v1/events?limit=10"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(body["count"], 3);
    let identities: Vec<&str> = body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["asset"]["identity"].as_str().unwrap())
        .collect();
    // The server returns most-recent-first; ours were appended
    // a → b → c, so c should lead.
    assert_eq!(identities, vec!["c", "b", "a"]);
}

#[tokio::test]
async fn server_keeps_rejecting_keeps_spool_full() {
    // Drain pass against an unreachable URL: every entry
    // should be retained, the spool should still contain the
    // original three events afterwards.
    let spool_dir = tempfile::tempdir().expect("spool tempdir");
    let spool = Spool::open(spool_dir.path()).unwrap();
    for id in ["x", "y", "z"] {
        spool.append(&fixture(id)).unwrap();
    }
    let stats = tokio::task::spawn_blocking(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        // Pick a port nothing will be listening on.
        let url = "http://127.0.0.1:1/v1/events";
        spool
            .drain(|ev| match client.post(url).json(ev).send() {
                Ok(r) if r.status().is_success() => Ok(()),
                Ok(r) => Err(anyhow::anyhow!("status {}", r.status())),
                Err(e) => Err(anyhow::anyhow!(e)),
            })
            .unwrap()
    })
    .await
    .unwrap();

    assert_eq!(stats.seen, 3);
    assert_eq!(stats.delivered, 0);
    assert_eq!(stats.retained, 3);
    let spool = Spool::open(spool_dir.path()).unwrap();
    assert_eq!(spool.len().unwrap(), 3);
}
