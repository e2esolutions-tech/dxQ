//! Integration smoke for the posture-history endpoint (SEZ-32).
//!
//! Boots the router, hits /v1/posture a few times (each records a
//! snapshot — the ring's min-spacing is disabled via a 0-spacing
//! test history so every poll lands), and asserts
//! /v1/posture/history returns the trend.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use ree0xq_core::{
    Asset, AssetKind, CryptoInventoryEvent, Posture, Primitive, PrimitiveRole, SCHEMA_MINOR,
    SCHEMA_VERSION,
};
use ree0xq_server::{history::SnapshotThrottle, router, AppState};
use serde_json::Value;

async fn spawn() -> SocketAddr {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = AppState::new_in_memory(tmp.path(), None).unwrap();
    std::mem::forget(tmp);
    // Replace the throttle with a 0-spacing one so every poll in this
    // fast test is retained (defaults throttle to one per 30 s).
    state.snapshot_throttle = Arc::new(SnapshotThrottle::new(Duration::from_secs(0)));
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

fn event(identity: &str) -> CryptoInventoryEvent {
    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: "test".into(),
        observed_at: chrono::Utc::now(),
        asset: Asset {
            kind: AssetKind::TlsSession,
            identity: identity.into(),
            host: Some("h".into()),
        },
        primitives: vec![Primitive {
            role: PrimitiveRole::Kex,
            algorithm: "X25519".into(),
            parameters: Default::default(),
            pq_resistant: Some(false),
            nist_classification: None,
        }],
        channel_protection: None,
        agility: None,
        posture: Posture {
            score: 50,
            rationale: "t".into(),
            recommended_replacement: None,
        },
    }
}

#[tokio::test]
async fn posture_history_accumulates_snapshots() {
    let addr = spawn().await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // Empty history before any posture read.
    let h0: Value = client
        .get(format!("{base}/v1/posture/history"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(h0["count"], 0);

    // Ingest an asset, then poll posture three times.
    client
        .post(format!("{base}/v1/events"))
        .json(&event("a1"))
        .send()
        .await
        .unwrap();
    for _ in 0..3 {
        client.get(format!("{base}/v1/posture")).send().await.unwrap();
    }

    let h: Value = client
        .get(format!("{base}/v1/posture/history"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(h["count"], 3, "three polls → three snapshots");
    let snaps = h["snapshots"].as_array().unwrap();
    assert_eq!(snaps[0]["assets"], 1);
    assert!(snaps[0]["org_q"].as_f64().unwrap() >= 0.0);

    // since_secs=0 → only snapshots newer than "now" → empty.
    let recent: Value = client
        .get(format!("{base}/v1/posture/history?since_secs=0"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(recent["count"], 0, "since_secs=0 excludes past snapshots");
}
