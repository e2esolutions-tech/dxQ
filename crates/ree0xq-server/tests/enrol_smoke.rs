//! Integration smoke for the mTLS bootstrap surface (SEZ-6).
//!
//! Boots the collector router in-process on an ephemeral port,
//! exercises:
//!
//! - admin-token gating on `POST /v1/admin/bootstrap-tokens`
//!   (missing token → 401, wrong token → 401, disabled admin
//!   → 503),
//! - the happy-path flow: admin issues a token → agent posts
//!   `/v1/enrol` with it → server returns a signed agent cert
//!   plus the CA cert,
//! - single-use enforcement (the same token redeemed twice →
//!   401 the second time),
//! - agent-id binding (a token issued for `A` does not work for
//!   `B` and does not get burned by the mismatched attempt),
//! - request shape rejections (missing token header, empty
//!   agent_id, validity out of range).
//!
//! Pure async HTTP against the live router; no rustls / TLS
//! termination yet — that arrives in the follow-up commit that
//! wires the cert chain into the listener.

use std::net::SocketAddr;
use std::time::Duration;

use ree0xq_server::enrol::{ADMIN_HEADER, BOOTSTRAP_HEADER};
use ree0xq_server::{router, AppState};

const ADMIN_SECRET: &str = "test-admin-secret-do-not-use-in-prod";

async fn spawn_server(admin_token: Option<&str>) -> SocketAddr {
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = AppState::new_in_memory(tmp.path(), admin_token.map(|s| s.to_string()))
        .expect("AppState init");
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

#[tokio::test]
async fn issue_and_enrol_happy_path() {
    let addr = spawn_server(Some(ADMIN_SECRET)).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // Admin issues a token for agent "ree0xq-net-01".
    let r = client
        .post(format!("{base}/v1/admin/bootstrap-tokens"))
        .header(ADMIN_HEADER, ADMIN_SECRET)
        .json(&serde_json::json!({
            "agent_id": "ree0xq-net-01",
            "ttl_hours": 1
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "issue should succeed");
    let issued: serde_json::Value = r.json().await.unwrap();
    let token = issued["token"].as_str().unwrap().to_string();
    assert_eq!(issued["agent_id"], "ree0xq-net-01");
    assert!(!token.is_empty() && token.len() >= 32);

    // Agent enrols with the token.
    let r = client
        .post(format!("{base}/v1/enrol"))
        .header(BOOTSTRAP_HEADER, &token)
        .json(&serde_json::json!({
            "agent_id": "ree0xq-net-01",
            "validity_days": 7
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "enrol should succeed");
    let cert: serde_json::Value = r.json().await.unwrap();
    assert_eq!(cert["agent_id"], "ree0xq-net-01");
    let cert_pem = cert["cert_pem"].as_str().unwrap();
    let key_pem = cert["key_pem"].as_str().unwrap();
    let ca_pem = cert["ca_cert_pem"].as_str().unwrap();
    assert!(cert_pem.contains("BEGIN CERTIFICATE"));
    assert!(key_pem.contains("BEGIN PRIVATE KEY"));
    assert!(ca_pem.contains("BEGIN CERTIFICATE"));

    // Second enrol with the same token is rejected — single-use.
    let r = client
        .post(format!("{base}/v1/enrol"))
        .header(BOOTSTRAP_HEADER, &token)
        .json(&serde_json::json!({ "agent_id": "ree0xq-net-01" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401, "single-use enforcement");
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["code"], "bootstrap_token_invalid");
}

#[tokio::test]
async fn admin_endpoint_rejects_without_secret() {
    let addr = spawn_server(Some(ADMIN_SECRET)).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // Missing admin header.
    let r = client
        .post(format!("{base}/v1/admin/bootstrap-tokens"))
        .json(&serde_json::json!({ "agent_id": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);

    // Wrong admin header.
    let r = client
        .post(format!("{base}/v1/admin/bootstrap-tokens"))
        .header(ADMIN_HEADER, "wrong-secret")
        .json(&serde_json::json!({ "agent_id": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
}

#[tokio::test]
async fn admin_endpoint_503s_when_admin_disabled() {
    let addr = spawn_server(None).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let r = client
        .post(format!("{base}/v1/admin/bootstrap-tokens"))
        .header(ADMIN_HEADER, "anything")
        .json(&serde_json::json!({ "agent_id": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 503);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["code"], "admin_disabled");
}

#[tokio::test]
async fn mismatched_agent_id_does_not_burn_token() {
    let addr = spawn_server(Some(ADMIN_SECRET)).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let issued: serde_json::Value = client
        .post(format!("{base}/v1/admin/bootstrap-tokens"))
        .header(ADMIN_HEADER, ADMIN_SECRET)
        .json(&serde_json::json!({ "agent_id": "agent-A" }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = issued["token"].as_str().unwrap();

    // Wrong agent id is rejected.
    let r = client
        .post(format!("{base}/v1/enrol"))
        .header(BOOTSTRAP_HEADER, token)
        .json(&serde_json::json!({ "agent_id": "agent-B" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["code"], "agent_id_mismatch");

    // The correct agent can still redeem the token — the
    // mismatched attempt did not burn it.
    let r = client
        .post(format!("{base}/v1/enrol"))
        .header(BOOTSTRAP_HEADER, token)
        .json(&serde_json::json!({ "agent_id": "agent-A" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 200, "correct agent should still redeem");
}

#[tokio::test]
async fn shape_rejections() {
    let addr = spawn_server(Some(ADMIN_SECRET)).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // Empty agent_id on issue.
    let r = client
        .post(format!("{base}/v1/admin/bootstrap-tokens"))
        .header(ADMIN_HEADER, ADMIN_SECRET)
        .json(&serde_json::json!({ "agent_id": "  " }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 422);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["code"], "agent_id_required");

    // ttl_hours out of range on issue.
    let r = client
        .post(format!("{base}/v1/admin/bootstrap-tokens"))
        .header(ADMIN_HEADER, ADMIN_SECRET)
        .json(&serde_json::json!({ "agent_id": "x", "ttl_hours": 0 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 422);

    // Missing bootstrap header on enrol.
    let r = client
        .post(format!("{base}/v1/enrol"))
        .json(&serde_json::json!({ "agent_id": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), 401);
    let body: serde_json::Value = r.json().await.unwrap();
    assert_eq!(body["code"], "missing_bootstrap_token");
}
