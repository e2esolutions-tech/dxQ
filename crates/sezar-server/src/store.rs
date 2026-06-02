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

use std::sync::Mutex;

use async_trait::async_trait;
use dashmap::DashMap;
use sezar_core::{Asset, AssetKind, CryptoInventoryEvent};

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use sezar_core::{Asset, AssetKind, Posture, SCHEMA_MINOR, SCHEMA_VERSION};

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
}
