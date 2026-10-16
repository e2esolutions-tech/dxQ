//! BLOCKED-transition webhook alerting (SEZ-33).
//!
//! When an asset first enters the BLOCKED set — agility ≤ Locked, so
//! the crypto cannot be rotated in place and migration is forced —
//! operators want a push, not a poll of `/v1/blocked`. This module
//! POSTs a small JSON alert to an operator-configured webhook URL,
//! with a bounded exponential-backoff retry.
//!
//! The alert fires only on the *transition into* BLOCKED (not on
//! every subsequent still-blocked observation), and the delivery runs
//! on a detached task so ingest latency never pays for the webhook
//! round-trip.
//!
//! Payload contains only observed asset facts (kind, identity, host,
//! q score, rationale, timestamp). It carries NO server secrets —
//! no admin token, no database URL, nothing from the environment.

use std::time::Duration;

use serde::Serialize;
use tracing::{info, warn};

/// JSON body POSTed to the operator webhook on a BLOCKED transition.
#[derive(Debug, Clone, Serialize)]
pub struct BlockedAlert {
    /// Constant discriminator so a shared receiver can route by type.
    pub event: &'static str,
    pub asset_kind: String,
    pub identity: String,
    pub host: Option<String>,
    /// Posture rationale explaining why the asset is blocked.
    pub rationale: String,
    /// Deadline-adjusted q score at the time of the transition.
    pub q_score: u8,
    pub observed_at: chrono::DateTime<chrono::Utc>,
}

impl BlockedAlert {
    pub const EVENT: &'static str = "asset_blocked";
}

/// Delivery timeout per attempt. A webhook receiver that cannot ack
/// within this window counts as a failed attempt and is retried.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Total attempts (1 initial + retries) before giving up.
const MAX_ATTEMPTS: u32 = 4;
/// First backoff step; doubles each retry (0.5s, 1s, 2s).
const BASE_BACKOFF: Duration = Duration::from_millis(500);

/// Operator-configured BLOCKED-transition alerter. Disabled when no
/// URL is set — every call becomes a no-op so handlers need not care.
#[derive(Clone)]
pub struct WebhookAlerter {
    url: Option<String>,
    client: reqwest::Client,
    max_attempts: u32,
    base_backoff: Duration,
}

impl WebhookAlerter {
    /// Build an alerter. `None` → disabled.
    pub fn new(url: Option<String>) -> Self {
        // reqwest here is built with `rustls-tls-*-no-provider`, so it
        // relies on the process-default rustls CryptoProvider. Install
        // the workspace's `ring` provider (idempotent) before building
        // the client so an https webhook works on every path — the TLS
        // listener installs the same provider, but the in-memory /
        // acceptance paths may not have yet.
        crate::tls::install_default_crypto_provider();
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(concat!("dxq-server/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            url,
            client,
            max_attempts: MAX_ATTEMPTS,
            base_backoff: BASE_BACKOFF,
        }
    }

    /// Test/ops override for the retry schedule.
    pub fn with_retry(mut self, max_attempts: u32, base_backoff: Duration) -> Self {
        self.max_attempts = max_attempts.max(1);
        self.base_backoff = base_backoff;
        self
    }

    pub fn enabled(&self) -> bool {
        self.url.is_some()
    }

    /// Fire-and-forget: spawn a detached task that delivers the alert
    /// with retry/backoff. No-op when disabled.
    pub fn notify_blocked(&self, alert: BlockedAlert) {
        let Some(url) = self.url.clone() else {
            return;
        };
        let client = self.client.clone();
        let max = self.max_attempts;
        let backoff = self.base_backoff;
        tokio::spawn(async move {
            deliver(&client, &url, &alert, max, backoff).await;
        });
    }
}

/// Deliver one alert, retrying on transport error or non-2xx up to
/// `max_attempts` with exponential backoff. Returns true on success.
/// Exposed at crate level so the integration test can await delivery
/// deterministically instead of racing the detached task.
pub(crate) async fn deliver(
    client: &reqwest::Client,
    url: &str,
    alert: &BlockedAlert,
    max_attempts: u32,
    base_backoff: Duration,
) -> bool {
    let mut backoff = base_backoff;
    for attempt in 1..=max_attempts {
        match client.post(url).json(alert).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    identity = %alert.identity,
                    attempt,
                    "BLOCKED alert delivered"
                );
                return true;
            }
            Ok(resp) => warn!(
                identity = %alert.identity,
                status = %resp.status(),
                attempt,
                "BLOCKED alert rejected by receiver"
            ),
            Err(e) => warn!(
                identity = %alert.identity,
                error = %e,
                attempt,
                "BLOCKED alert transport error"
            ),
        }
        if attempt < max_attempts {
            tokio::time::sleep(backoff).await;
            backoff = backoff.saturating_mul(2);
        }
    }
    warn!(
        identity = %alert.identity,
        attempts = max_attempts,
        "giving up on BLOCKED alert after exhausting retries"
    );
    false
}
