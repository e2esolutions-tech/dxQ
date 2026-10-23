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

use std::sync::mpsc::Sender;
use std::time::Duration;

use dxq_core::AssetKind;
use serde::Deserialize;

/// Poll cadence. Matches the web dashboard's "feels live" interval
/// without loading the collector.
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);

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

/// Everything one poll gathered, or the error that poll hit. The GUI
/// always holds the most recent `Snapshot`.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub posture: Option<OrgPosture>,
    pub inventory: Vec<InventoryItem>,
    pub blocked: Vec<InventoryItem>,
    pub history: Vec<PostureSnapshot>,
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
    Ok(Snapshot {
        posture: Some(posture),
        inventory: inventory.items,
        blocked: blocked.items,
        history: history.snapshots,
        error: None,
        generation: 0,
    })
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
pub fn spawn_poller<F>(base: String, tx: Sender<Snapshot>, repaint: F)
where
    F: Fn() + Send + 'static,
{
    std::thread::spawn(move || {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("dxq-desktop/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
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
}
