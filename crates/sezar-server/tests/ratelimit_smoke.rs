//! Integration smoke for the bootstrap-endpoint rate limit
//! (security HIGH-2).
//!
//! Boots the collector router in-process *with connection info
//! wired* (mirroring how `main.rs` serves the bootstrap listener),
//! then floods `POST /v1/admin/bootstrap-tokens` from one client
//! past the limit and asserts the limiter starts returning 429.
//!
//! A second test confirms the body-size limit (HIGH-1): an
//! oversize POST is rejected with 413 rather than buffered.

use std::net::SocketAddr;
use std::time::Duration;

use sezar_server::enrol::ADMIN_HEADER;
use sezar_server::{ratelimit, router, AppState};

const ADMIN_SECRET: &str = "test-admin-secret-do-not-use-in-prod";

/// Spawn the router with `ConnectInfo` wired, so the rate-limit
/// middleware keys on the real peer address — exactly as the
/// production bootstrap listener does.
async fn spawn_with_connect_info(admin_token: Option<&str>) -> SocketAddr {
    let tmp = tempfile::tempdir().expect("tempdir");
    let state = AppState::new_in_memory(tmp.path(), admin_token.map(|s| s.to_string()))
        .expect("AppState init");
    std::mem::forget(tmp);
    let app = router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    addr
}

#[tokio::test]
async fn admin_endpoint_rate_limits_a_flooding_client() {
    let addr = spawn_with_connect_info(Some(ADMIN_SECRET)).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // The default limit is DEFAULT_MAX_REQUESTS per window. Fire a
    // bit more than that from the same client (all from 127.0.0.1)
    // and confirm the surplus is rejected with 429. We use a wrong
    // admin token so each request is cheap (401 before the limit,
    // 429 after) — the limiter runs before the handler.
    let limit = ratelimit::DEFAULT_MAX_REQUESTS;
    let mut statuses = Vec::new();
    for _ in 0..(limit + 5) {
        let r = client
            .post(format!("{base}/v1/admin/bootstrap-tokens"))
            .header(ADMIN_HEADER, "wrong-token")
            .json(&serde_json::json!({ "agent_id": "flooder" }))
            .send()
            .await
            .unwrap();
        statuses.push(r.status().as_u16());
    }

    let too_many = statuses.iter().filter(|&&s| s == 429).count();
    let unauthorized = statuses.iter().filter(|&&s| s == 401).count();

    assert!(
        too_many >= 1,
        "expected the flood to trip the rate limit (429); got statuses {statuses:?}"
    );
    // The first `limit` requests pass the limiter (and 401 on the
    // wrong token); the rest are 429.
    assert!(
        unauthorized <= limit,
        "no more than the limit should reach the handler; got {unauthorized} x 401"
    );
    assert_eq!(
        unauthorized + too_many,
        limit + 5,
        "every request is either rejected by the limiter or the handler"
    );
}

#[tokio::test]
async fn healthz_is_not_rate_limited() {
    let addr = spawn_with_connect_info(Some(ADMIN_SECRET)).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    // Far more than the limit — the liveness probe must never 429.
    for _ in 0..(ratelimit::DEFAULT_MAX_REQUESTS + 10) {
        let r = client.get(format!("{base}/healthz")).send().await.unwrap();
        assert_eq!(r.status(), 200, "healthz must not be rate-limited");
    }
}

#[tokio::test]
async fn oversize_body_is_rejected_with_413() {
    // HIGH-1: a body larger than MAX_BODY_BYTES is rejected before
    // it is buffered into memory.
    let addr = spawn_with_connect_info(None).await;
    let base = format!("http://{addr}");
    let client = reqwest::Client::new();

    let oversize = vec![b'x'; sezar_server::MAX_BODY_BYTES + 1024];
    let r = client
        .post(format!("{base}/v1/events"))
        .header("content-type", "application/json")
        .body(oversize)
        .send()
        .await
        .unwrap();

    assert_eq!(
        r.status(),
        413,
        "oversize body should be rejected with 413 Payload Too Large"
    );
}
