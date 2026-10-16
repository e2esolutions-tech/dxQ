//! Integration smoke for BLOCKED-transition webhook alerting (SEZ-33).
//!
//! Stands up a local receiver (a tiny axum app that captures every
//! POSTed JSON body), points a configured `WebhookAlerter` at it via
//! the real ingest path, and asserts:
//!
//! - an asset that ingests already-BLOCKED fires exactly one alert
//!   carrying its identity + rationale,
//! - a second still-BLOCKED observation of the same asset does NOT
//!   re-fire (alert is on the transition, not every observation),
//! - the payload contains no server secrets.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{extract::State, routing::post, Json, Router};
use dxq_core::{
    AgilityBlock, AgilityLevel, Asset, AssetKind, CryptoInventoryEvent, Posture, Primitive,
    PrimitiveRole, SCHEMA_MINOR, SCHEMA_VERSION,
};
use dxq_server::{alert::WebhookAlerter, router, AppState};
use serde_json::Value;

/// Shared capture buffer for the receiver.
type Captured = Arc<Mutex<Vec<Value>>>;

async fn receive(State(buf): State<Captured>, Json(body): Json<Value>) -> &'static str {
    buf.lock().unwrap().push(body);
    "ok"
}

/// Spawn the local webhook receiver; returns its URL and the capture
/// buffer.
async fn spawn_receiver() -> (String, Captured) {
    let buf: Captured = Arc::new(Mutex::new(Vec::new()));
    let app = Router::new()
        .route("/hook", post(receive))
        .with_state(buf.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}/hook"), buf)
}

/// Spawn the dxq-server router with an alerter pointed at `url`.
async fn spawn_server(url: &str) -> SocketAddr {
    let tmp = tempfile::tempdir().unwrap();
    let mut state = AppState::new_in_memory(tmp.path(), None).unwrap();
    std::mem::forget(tmp);
    // Fast retry schedule so a flaky receiver wouldn't stall the test.
    state.alerter = Arc::new(
        WebhookAlerter::new(Some(url.to_string()))
            .with_retry(3, Duration::from_millis(20)),
    );
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

fn locked_block() -> AgilityBlock {
    AgilityBlock {
        level: AgilityLevel::Locked,
        level_score: AgilityLevel::Locked.score(),
        evidence: vec![],
        scanner_version: "test".into(),
        rubric_version: "test".into(),
    }
}

fn configurable_block() -> AgilityBlock {
    AgilityBlock {
        level: AgilityLevel::Configurable,
        level_score: AgilityLevel::Configurable.score(),
        evidence: vec![],
        scanner_version: "test".into(),
        rubric_version: "test".into(),
    }
}

fn event(identity: &str, ag: AgilityBlock, offset_secs: i64, rationale: &str) -> CryptoInventoryEvent {
    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: "dxq-net".into(),
        observed_at: chrono::Utc::now() + chrono::Duration::seconds(offset_secs),
        asset: Asset {
            kind: AssetKind::TlsSession,
            identity: identity.into(),
            host: Some("host.example".into()),
        },
        primitives: vec![Primitive {
            role: PrimitiveRole::Sig,
            algorithm: "RSA-2048".into(),
            parameters: Default::default(),
            pq_resistant: Some(false),
            nist_classification: None,
        }],
        channel_protection: None,
        agility: Some(ag),
        posture: Posture {
            score: 15,
            rationale: rationale.into(),
            recommended_replacement: None,
        },
    }
}

/// Poll the capture buffer until it holds `n` entries or the deadline
/// passes. Returns the captured bodies.
async fn wait_for(buf: &Captured, n: usize, within: Duration) -> Vec<Value> {
    let start = tokio::time::Instant::now();
    loop {
        {
            let got = buf.lock().unwrap();
            if got.len() >= n {
                return got.clone();
            }
        }
        if start.elapsed() > within {
            return buf.lock().unwrap().clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test]
async fn blocked_transition_fires_one_alert_with_no_secrets() {
    let (url, buf) = spawn_receiver().await;
    let addr = spawn_server(&url).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // 1) A non-blocked observation → must NOT alert.
    client
        .post(format!("{base}/v1/events"))
        .json(&event("tls-x", configurable_block(), 0, "configurable, not blocked"))
        .send()
        .await
        .unwrap();

    // 2) The asset enters BLOCKED → exactly one alert.
    let rationale = "firmware-pinned RSA-2048, no in-place rotation";
    client
        .post(format!("{base}/v1/events"))
        .json(&event("tls-x", locked_block(), 10, rationale))
        .send()
        .await
        .unwrap();

    let got = wait_for(&buf, 1, Duration::from_secs(3)).await;
    assert_eq!(got.len(), 1, "exactly one alert on the transition into BLOCKED");
    let a = &got[0];
    assert_eq!(a["event"], "asset_blocked");
    assert_eq!(a["identity"], "tls-x");
    assert_eq!(a["rationale"], rationale);
    assert_eq!(a["asset_kind"], "tls_session");
    assert_eq!(a["host"], "host.example");
    // No server secrets leaked into the payload.
    let flat = a.to_string().to_lowercase();
    for banned in ["admin", "token", "database", "password", "secret"] {
        assert!(!flat.contains(banned), "payload must not contain '{banned}'");
    }

    // 3) A later still-BLOCKED observation of the same asset → no new
    //    alert (transition already happened).
    client
        .post(format!("{base}/v1/events"))
        .json(&event("tls-x", locked_block(), 20, "still firmware-pinned"))
        .send()
        .await
        .unwrap();

    // Give any (erroneous) alert time to land, then assert still one.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        buf.lock().unwrap().len(),
        1,
        "staying BLOCKED must not re-fire the alert"
    );
}

#[tokio::test]
async fn no_webhook_configured_is_silent() {
    // Default AppState has a disabled alerter; a blocked event must
    // ingest fine and raise nothing.
    let tmp = tempfile::tempdir().unwrap();
    let state = AppState::new_in_memory(tmp.path(), None).unwrap();
    assert!(!state.alerter.enabled());
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let r = client
        .post(format!("http://{addr}/v1/events"))
        .json(&event("tls-y", locked_block(), 0, "blocked"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 202, "ingest succeeds with no webhook configured");
}
