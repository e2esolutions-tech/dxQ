//! REST client for the dxQ collector + the background poller.
//!
//! The GUI runs on the main thread; blocking HTTP would freeze it, so
//! a detached worker thread polls the collector on a fixed cadence and
//! pushes an immutable [`Snapshot`] through an `mpsc` channel. The
//! worker asks egui to repaint whenever fresh data lands.
//!
//! Response structs mirror the JSON the collector serves
//! (`web/src/types/dxq.ts` is the canonical view). `asset_kind`
//! deserializes straight into [`dxq_core::AssetKind`] so the desktop
//! client shares the schema's closed enum rather than re-spelling it.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

use dxq_core::AssetKind;
use serde::Deserialize;

/// Poll cadence. Matches the web dashboard's "feels live" interval
/// without loading the collector.
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Optional TLS material for reaching the collector. The main `/v1`
/// surface sits behind an mTLS listener, so a production collector
/// needs a client cert + key (signed by the internal CA) and the CA
/// cert to verify the server. All `None` → plain HTTP / server-auth
/// HTTPS against a publicly-trusted cert.
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    pub client_cert: Option<PathBuf>,
    pub client_key: Option<PathBuf>,
    pub ca_cert: Option<PathBuf>,
}

impl TlsConfig {
    pub fn is_mtls(&self) -> bool {
        self.client_cert.is_some() || self.client_key.is_some()
    }
}

/// `GET /v1/posture`.
#[derive(Debug, Clone, Deserialize)]
pub struct OrgPosture {
    pub org_q: f32,
    pub deadline: String,
    pub horizon_years: f32,
    pub assets: usize,
    pub blocked_count: usize,
}

/// One row of `GET /v1/inventory` (and `/v1/blocked`). Only the
/// fields the UI renders are kept; serde ignores the rest of the
/// wire shape (`source_module`, `observed_at`, …).
#[derive(Debug, Clone, Deserialize)]
pub struct InventoryItem {
    pub asset_kind: AssetKind,
    pub identity: String,
    #[serde(default)]
    pub host: Option<String>,
    pub q: f32,
    pub blocked: bool,
    #[serde(default)]
    pub primitives: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InventoryResponse {
    pub items: Vec<InventoryItem>,
}

/// One point of `GET /v1/posture/history`. The trend plots `org_q`
/// with a `blocked_count` overlay; the rest of the wire point is
/// ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct PostureSnapshot {
    pub org_q: f32,
    pub blocked_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PostureHistoryResponse {
    pub snapshots: Vec<PostureSnapshot>,
}

/// One replacement suggestion from `GET /v1/recommendations`.
#[derive(Debug, Clone, Deserialize)]
pub struct Recommendation {
    pub replaces: String,
    pub replacement: String,
    pub rationale: String,
    /// `trivial` | `low` | `medium` | `high` (kebab-case on the wire).
    pub cost: String,
    #[serde(default)]
    pub caveats: Vec<String>,
}

/// Per-asset recommendation row.
#[derive(Debug, Clone, Deserialize)]
pub struct RecommendationItem {
    pub asset_kind: AssetKind,
    pub identity: String,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub current_primitives: Vec<String>,
    #[serde(default)]
    pub recommendations: Vec<Recommendation>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RecommendationsResponse {
    pub items: Vec<RecommendationItem>,
}

/// One cell of the `GET /v1/agility/compat` TLS-stack × algorithm
/// matrix.
#[derive(Debug, Clone, Deserialize)]
pub struct CompatEntry {
    pub stack: String,
    pub algorithm: String,
    /// `supported` | `experimental` | `not-implemented` | `unknown`.
    pub status: String,
    #[serde(default)]
    pub min_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompatResponse {
    pub items: Vec<CompatEntry>,
}

/// Everything one poll gathered, or the error that poll hit. The GUI
/// always holds the most recent `Snapshot`.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub posture: Option<OrgPosture>,
    pub inventory: Vec<InventoryItem>,
    pub blocked: Vec<InventoryItem>,
    pub history: Vec<PostureSnapshot>,
    pub recommendations: Vec<RecommendationItem>,
    pub compat: Vec<CompatEntry>,
    /// Human-readable error from the last poll, if it failed.
    pub error: Option<String>,
    /// Monotonic poll counter, so the UI can show liveness.
    pub generation: u64,
}

/// Fetch all four endpoints once. A single failed endpoint fails the
/// whole poll with a message — the GUI keeps showing the previous
/// good data and surfaces the error banner.
fn fetch_once(
    client: &reqwest::blocking::Client,
    base: &str,
    generation: u64,
) -> Snapshot {
    let base = base.trim_end_matches('/');
    match build_snapshot(client, base) {
        Ok(mut s) => {
            s.generation = generation;
            s
        }
        Err(e) => Snapshot {
            error: Some(e.to_string()),
            generation,
            ..Default::default()
        },
    }
}

fn build_snapshot(client: &reqwest::blocking::Client, base: &str) -> anyhow::Result<Snapshot> {
    let posture: OrgPosture = get_json(client, &format!("{base}/v1/posture"))?;
    let inventory: InventoryResponse = get_json(client, &format!("{base}/v1/inventory"))?;
    let blocked: InventoryResponse = get_json(client, &format!("{base}/v1/blocked"))?;
    let history: PostureHistoryResponse =
        get_json(client, &format!("{base}/v1/posture/history"))?;
    let recs: RecommendationsResponse = get_json(client, &format!("{base}/v1/recommendations"))?;
    let compat: CompatResponse = get_json(client, &format!("{base}/v1/agility/compat"))?;
    Ok(Snapshot {
        posture: Some(posture),
        inventory: inventory.items,
        blocked: blocked.items,
        history: history.snapshots,
        recommendations: recs.items,
        compat: compat.items,
        error: None,
        generation: 0,
    })
}

/// Build the blocking HTTP client, wiring in mTLS material when the
/// operator supplied it. Fails loudly (surfaced in the UI) on a bad
/// cert/key/CA rather than silently dropping to an unauthenticated
/// client.
fn build_client(tls: &TlsConfig) -> anyhow::Result<reqwest::blocking::Client> {
    let mut b = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("dxq-desktop/", env!("CARGO_PKG_VERSION")));

    if let Some(ca) = &tls.ca_cert {
        let pem = std::fs::read(ca)
            .map_err(|e| anyhow::anyhow!("read CA cert {}: {e}", ca.display()))?;
        let cert = reqwest::Certificate::from_pem(&pem)
            .map_err(|e| anyhow::anyhow!("parse CA cert: {e}"))?;
        b = b.add_root_certificate(cert);
    }

    match (&tls.client_cert, &tls.client_key) {
        (Some(cert), Some(key)) => {
            // reqwest's rustls Identity wants cert chain + key in one
            // PEM buffer; concatenate the two files.
            let mut pem = std::fs::read(cert)
                .map_err(|e| anyhow::anyhow!("read client cert {}: {e}", cert.display()))?;
            pem.push(b'\n');
            pem.extend_from_slice(
                &std::fs::read(key)
                    .map_err(|e| anyhow::anyhow!("read client key {}: {e}", key.display()))?,
            );
            let id = reqwest::Identity::from_pem(&pem)
                .map_err(|e| anyhow::anyhow!("build client identity (cert+key): {e}"))?;
            b = b.identity(id);
        }
        (None, None) => {}
        _ => anyhow::bail!("mTLS needs BOTH --client-cert and --client-key"),
    }

    b.build().map_err(|e| anyhow::anyhow!("build HTTP client: {e}"))
}

fn get_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::blocking::Client,
    url: &str,
) -> anyhow::Result<T> {
    let resp = client.get(url).send()?;
    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("{url} -> HTTP {status}");
    }
    Ok(resp.json::<T>()?)
}

/// Spawn the detached poll loop. `repaint` is called after every poll
/// so egui wakes and renders the fresh snapshot even while idle.
pub fn spawn_poller<F>(base: String, tls: TlsConfig, tx: Sender<Snapshot>, repaint: F)
where
    F: Fn() + Send + 'static,
{
    std::thread::spawn(move || {
        let client = match build_client(&tls) {
            Ok(c) => c,
            Err(e) => {
                // Surface the TLS/setup error once and stop; the UI
                // shows it in the header banner.
                let _ = tx.send(Snapshot {
                    error: Some(e.to_string()),
                    generation: 1,
                    ..Default::default()
                });
                repaint();
                return;
            }
        };
        let mut generation = 0u64;
        loop {
            generation += 1;
            let snap = fetch_once(&client, &base, generation);
            // A closed receiver means the UI is gone; stop polling.
            if tx.send(snap).is_err() {
                break;
            }
            repaint();
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_org_posture() {
        let j = r#"{"org_q":0.61,"deadline":"2030-01-01T00:00:00Z",
                    "horizon_years":5.0,"assets":5,"blocked_count":1}"#;
        let p: OrgPosture = serde_json::from_str(j).unwrap();
        assert_eq!(p.assets, 5);
        assert_eq!(p.blocked_count, 1);
        assert!((p.org_q - 0.61).abs() < 1e-6);
    }

    #[test]
    fn deserializes_inventory_item_with_real_asset_kind() {
        let j = r#"{"source_module":"dxq-net","asset_kind":"ssh_session",
                    "identity":"10.0.0.4:22","host":null,"q":0.55,
                    "blocked":false,"primitives":["sntrup761x25519"],
                    "observed_at":"2026-08-20T10:00:00Z"}"#;
        let it: InventoryItem = serde_json::from_str(j).unwrap();
        assert_eq!(it.asset_kind, AssetKind::SshSession);
        assert!(!it.blocked);
        assert_eq!(it.primitives, vec!["sntrup761x25519"]);
        assert!(it.host.is_none());
    }

    #[test]
    fn deserializes_history() {
        let j = r#"{"count":2,"snapshots":[
            {"at":"2026-08-20T10:00:00Z","org_q":0.5,"assets":5,"blocked_count":1},
            {"at":"2026-08-20T10:00:30Z","org_q":0.6,"assets":5,"blocked_count":1}]}"#;
        let h: PostureHistoryResponse = serde_json::from_str(j).unwrap();
        assert_eq!(h.snapshots.len(), 2);
        assert!((h.snapshots[1].org_q - 0.6).abs() < 1e-6);
        assert_eq!(h.snapshots[0].blocked_count, 1);
    }

    #[test]
    fn deserializes_recommendations() {
        let j = r#"{"count":1,"items":[{
            "source_module":"dxq-cert","asset_kind":"x509_cert",
            "identity":"sha256:ab","host":null,
            "current_primitives":["RSA-2048"],
            "recommendations":[{"replaces":"RSA-2048","replacement":"ML-DSA-65",
              "rationale":"PQ signature","cost":"medium","caveats":["chain depth"]}]}]}"#;
        let r: RecommendationsResponse = serde_json::from_str(j).unwrap();
        assert_eq!(r.items.len(), 1);
        let it = &r.items[0];
        assert_eq!(it.asset_kind, AssetKind::X509Cert);
        assert_eq!(it.recommendations[0].cost, "medium");
        assert_eq!(it.recommendations[0].replacement, "ML-DSA-65");
    }

    #[test]
    fn deserializes_compat_with_kebab_status() {
        let j = r#"{"count":2,"items":[
            {"stack":"openssl-3.x","algorithm":"ML-KEM-768","status":"supported","min_version":"3.5"},
            {"stack":"go-crypto-tls","algorithm":"ML-DSA-65","status":"not-implemented","min_version":null}]}"#;
        let c: CompatResponse = serde_json::from_str(j).unwrap();
        assert_eq!(c.items.len(), 2);
        assert_eq!(c.items[0].status, "supported");
        assert_eq!(c.items[1].status, "not-implemented");
        assert_eq!(c.items[0].min_version.as_deref(), Some("3.5"));
    }
}
