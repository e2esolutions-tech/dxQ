//! Event store.
//!
//! Behind an async [`EventStore`] trait that lets the collector
//! swap between an in-memory implementation (V1 default,
//! [`InMemoryEventStore`]) and a Postgres-backed one
//! ([`crate::store_pg::PgEventStore`]) without touching the
//! handlers in [`crate::routes`]. The trait's method set
//! mirrors the calls the existing axum handlers make.
//!
//! All methods are async because the Postgres backend needs to
//! await the connection pool; the in-memory backend wraps a
//! blocking `Mutex` so its method bodies are effectively
//! synchronous despite the signature.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use dxq_core::{Asset, AssetKind, CryptoInventoryEvent};

use crate::history::{PostureSnapshot, DEFAULT_CAPACITY};

/// Event store abstraction.
// `len` here is the stored-event count for diagnostics; an
// `is_empty` companion would carry no meaning for the storage
// abstraction, so the clippy lint is intentionally suppressed.
#[allow(clippy::len_without_is_empty)]
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Append one event. Updates the per-asset latest pointer
    /// only when `ev.observed_at` is at-or-after the previous
    /// observation for the same dedup key.
    async fn append(&self, ev: CryptoInventoryEvent) -> anyhow::Result<()>;

    /// Total event count across the log (history depth).
    async fn len(&self) -> anyhow::Result<usize>;

    /// Up to `limit` most-recent events, newest-first.
    async fn recent(&self, limit: usize) -> anyhow::Result<Vec<CryptoInventoryEvent>>;

    /// Latest event per `(source_module, asset.kind, identity)`
    /// in arbitrary order; callers sort if they need a stable
    /// display order.
    async fn latest_per_asset(&self) -> anyhow::Result<Vec<CryptoInventoryEvent>>;

    /// Latest event for one specific asset, if any.
    async fn latest_for(
        &self,
        asset: &Asset,
        source_module: &str,
    ) -> anyhow::Result<Option<CryptoInventoryEvent>>;

    /// Persist one org-posture snapshot for the trend series
    /// (SEZ-32). The in-memory backend keeps a bounded ring; the
    /// Postgres backend writes a durable row so the trend survives a
    /// restart. The caller gates write-rate via
    /// [`crate::history::SnapshotThrottle`] before calling this.
    async fn record_snapshot(&self, snap: PostureSnapshot) -> anyhow::Result<()>;

    /// Retained posture snapshots at-or-after `since` (all if
    /// `None`), oldest-first, capped at `limit`. Backs
    /// `GET /v1/posture/history`.
    async fn posture_history(
        &self,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> anyhow::Result<Vec<PostureSnapshot>>;
}

/// Composite dedup key matching the v1 schema contract.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupKey {
    /// Module that emitted the event.
    pub source_module: String,
    /// Asset kind.
    pub kind: AssetKind,
    /// Module-scoped identity string.
    pub identity: String,
}

impl DedupKey {
    fn from_event(ev: &CryptoInventoryEvent) -> Self {
        Self {
            source_module: ev.source_module.clone(),
            kind: ev.asset.kind.clone(),
            identity: ev.asset.identity.clone(),
        }
    }
}

/// In-memory event store: append-only log + per-asset latest map.
///
/// Lock-free reads (DashMap) on the latest map; a single Mutex
/// only around the append-ordered log. Suitable for the V1
/// no-persistence path used by the in-process tests, the
/// acceptance smoke, and the default Docker quickstart.
pub struct InMemoryEventStore {
    log: Mutex<Vec<CryptoInventoryEvent>>,
    latest: DashMap<DedupKey, CryptoInventoryEvent>,
    /// Bounded org-posture trend ring (SEZ-32). Lost on restart —
    /// that is the definition of the in-memory backend; the Postgres
    /// backend is the durable one.
    snapshots: Mutex<VecDeque<PostureSnapshot>>,
}

impl Default for InMemoryEventStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryEventStore {
    /// New empty in-memory store.
    pub fn new() -> Self {
        Self {
            log: Mutex::new(Vec::new()),
            latest: DashMap::new(),
            snapshots: Mutex::new(VecDeque::new()),
        }
    }
}

#[async_trait]
impl EventStore for InMemoryEventStore {
    async fn append(&self, ev: CryptoInventoryEvent) -> anyhow::Result<()> {
        let key = DedupKey::from_event(&ev);
        if let Some(prev) = self.latest.get(&key) {
            if ev.observed_at < prev.observed_at {
                drop(prev);
                self.log.lock().unwrap().push(ev);
                return Ok(());
            }
        }
        self.latest.insert(key, ev.clone());
        self.log.lock().unwrap().push(ev);
        Ok(())
    }

    async fn len(&self) -> anyhow::Result<usize> {
        Ok(self.log.lock().unwrap().len())
    }

    async fn recent(&self, limit: usize) -> anyhow::Result<Vec<CryptoInventoryEvent>> {
        let log = self.log.lock().unwrap();
        let start = log.len().saturating_sub(limit);
        Ok(log[start..].iter().rev().cloned().collect())
    }

    async fn latest_per_asset(&self) -> anyhow::Result<Vec<CryptoInventoryEvent>> {
        Ok(self.latest.iter().map(|kv| kv.value().clone()).collect())
    }

    async fn latest_for(
        &self,
        asset: &Asset,
        source_module: &str,
    ) -> anyhow::Result<Option<CryptoInventoryEvent>> {
        Ok(self
            .latest
            .get(&DedupKey {
                source_module: source_module.into(),
                kind: asset.kind.clone(),
                identity: asset.identity.clone(),
            })
            .map(|kv| kv.value().clone()))
    }

    async fn record_snapshot(&self, snap: PostureSnapshot) -> anyhow::Result<()> {
        let mut buf = self.snapshots.lock().unwrap();
        buf.push_back(snap);
        while buf.len() > DEFAULT_CAPACITY {
            buf.pop_front();
        }
        Ok(())
    }

    async fn posture_history(
        &self,
        since: Option<DateTime<Utc>>,
        limit: usize,
    ) -> anyhow::Result<Vec<PostureSnapshot>> {
        let buf = self.snapshots.lock().unwrap();
        let mut out: Vec<PostureSnapshot> = match since {
            Some(t) => buf.iter().filter(|s| s.at >= t).cloned().collect(),
            None => buf.iter().cloned().collect(),
        };
        // Oldest-first already (push_back order); keep the newest
        // `limit` when the window is larger than the cap.
        if out.len() > limit {
            out.drain(0..out.len() - limit);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use dxq_core::{Asset, AssetKind, Posture, SCHEMA_MINOR, SCHEMA_VERSION};

    fn ev(identity: &str, ts: chrono::DateTime<chrono::Utc>, score: u8) -> CryptoInventoryEvent {
        CryptoInventoryEvent {
            schema_version: SCHEMA_VERSION,
            schema_minor: SCHEMA_MINOR,
            source_module: "test".into(),
            observed_at: ts,
            asset: Asset {
                kind: AssetKind::TlsSession,
                identity: identity.into(),
                host: Some("h".into()),
            },
            primitives: vec![],
            channel_protection: None,
            agility: None,
            posture: Posture {
                score,
                rationale: "fixture".into(),
                recommended_replacement: None,
            },
        }
    }

    #[tokio::test]
    async fn append_increments_count_and_latest_map() {
        let s = InMemoryEventStore::new();
        let t = chrono::Utc.with_ymd_and_hms(2026, 5, 13, 0, 0, 0).unwrap();
        s.append(ev("a", t, 10)).await.unwrap();
        s.append(ev("b", t, 20)).await.unwrap();
        assert_eq!(s.len().await.unwrap(), 2);
        assert_eq!(s.latest_per_asset().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn newer_event_replaces_latest_but_log_grows() {
        let s = InMemoryEventStore::new();
        let t1 = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t2 = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        s.append(ev("a", t1, 10)).await.unwrap();
        s.append(ev("a", t2, 80)).await.unwrap();
        assert_eq!(s.len().await.unwrap(), 2, "log keeps both events");
        let latest = s.latest_per_asset().await.unwrap();
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].posture.score, 80);
    }

    #[tokio::test]
    async fn out_of_order_event_does_not_overwrite_latest() {
        let s = InMemoryEventStore::new();
        let t1 = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t2 = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        s.append(ev("a", t2, 80)).await.unwrap();
        s.append(ev("a", t1, 10)).await.unwrap();
        assert_eq!(s.len().await.unwrap(), 2);
        let latest = s.latest_per_asset().await.unwrap();
        assert_eq!(latest[0].posture.score, 80);
    }

    #[tokio::test]
    async fn recent_returns_in_reverse_arrival_order() {
        let s = InMemoryEventStore::new();
        let t = chrono::Utc.with_ymd_and_hms(2026, 5, 13, 0, 0, 0).unwrap();
        for id in ["a", "b", "c"] {
            s.append(ev(id, t, 0)).await.unwrap();
        }
        let r = s.recent(10).await.unwrap();
        let ids: Vec<&str> = r.iter().map(|e| e.asset.identity.as_str()).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }

    fn snap(secs: i64, q: f32) -> PostureSnapshot {
        PostureSnapshot {
            at: DateTime::<Utc>::from_timestamp(1_700_000_000 + secs, 0).unwrap(),
            org_q: q,
            assets: 5,
            blocked_count: 1,
        }
    }

    #[tokio::test]
    async fn snapshot_ring_wraps_at_capacity() {
        let s = InMemoryEventStore::new();
        for i in 0..(DEFAULT_CAPACITY as i64 + 5) {
            s.record_snapshot(snap(i, i as f32)).await.unwrap();
        }
        let all = s.posture_history(None, usize::MAX).await.unwrap();
        assert_eq!(all.len(), DEFAULT_CAPACITY, "ring capped at capacity");
        assert_eq!(all.first().unwrap().org_q, 5.0, "oldest five evicted");
        assert_eq!(all.last().unwrap().org_q, (DEFAULT_CAPACITY as f32 + 4.0));
    }

    #[tokio::test]
    async fn posture_history_filters_by_since_and_limit() {
        let s = InMemoryEventStore::new();
        for i in 0..5 {
            s.record_snapshot(snap(i * 10, i as f32)).await.unwrap();
        }
        let cutoff = DateTime::<Utc>::from_timestamp(1_700_000_000 + 25, 0).unwrap();
        let w = s.posture_history(Some(cutoff), 100).await.unwrap();
        assert_eq!(w.len(), 2, "only snapshots at +30 and +40");
        // limit keeps the newest N, oldest-first.
        let last2 = s.posture_history(None, 2).await.unwrap();
        assert_eq!(last2.len(), 2);
        assert_eq!(last2.first().unwrap().org_q, 3.0);
        assert_eq!(last2.last().unwrap().org_q, 4.0);
    }
}
