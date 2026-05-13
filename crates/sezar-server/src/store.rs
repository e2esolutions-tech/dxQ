//! Event store.
//!
//! V1 implementation: an append-only in-memory event log plus a
//! per-asset latest-event map for fast inventory lookup.
//!
//! The store is `Send + Sync` and uses lock-free maps where it can
//! (DashMap for the latest-event dedup), with a single `Mutex` only
//! around the append-ordered event vector. Reads from `list_events`
//! and `latest_per_asset` do not block writes.

use std::sync::Mutex;

use dashmap::DashMap;
use sezar_core::{Asset, AssetKind, CryptoInventoryEvent};

/// Event-store trait. The V1 impl is in-memory; Postgres lands later
/// at a single point.
pub struct EventStore {
    /// Append-only log of every event we ingested, in arrival order.
    log: Mutex<Vec<CryptoInventoryEvent>>,
    /// Latest event per `(source_module, asset.kind, asset.identity)`
    /// — the dedup key recommended by the v1 schema doc.
    latest: DashMap<DedupKey, CryptoInventoryEvent>,
}

/// Composite dedup key matching the schema contract.
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

impl Default for EventStore {
    fn default() -> Self {
        Self::new_in_memory()
    }
}

impl EventStore {
    /// New empty in-memory store.
    pub fn new_in_memory() -> Self {
        Self {
            log: Mutex::new(Vec::new()),
            latest: DashMap::new(),
        }
    }

    /// Append an event. Updates the latest-per-asset map.
    pub fn append(&self, ev: CryptoInventoryEvent) {
        let key = DedupKey::from_event(&ev);
        // Only replace the latest map if this observation is at-or-after
        // the previous one — late-delivered events must not overwrite
        // newer observations.
        if let Some(prev) = self.latest.get(&key) {
            if ev.observed_at < prev.observed_at {
                drop(prev);
                self.log.lock().unwrap().push(ev);
                return;
            }
        }
        self.latest.insert(key, ev.clone());
        self.log.lock().unwrap().push(ev);
    }

    /// Number of events ingested.
    pub fn len(&self) -> usize {
        self.log.lock().unwrap().len()
    }

    /// Is the store empty?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return up to `limit` most recent events. `limit = 0` returns
    /// nothing (defensive — clients must explicitly pass a positive
    /// number).
    pub fn recent(&self, limit: usize) -> Vec<CryptoInventoryEvent> {
        let log = self.log.lock().unwrap();
        let start = log.len().saturating_sub(limit);
        log[start..].iter().rev().cloned().collect()
    }

    /// Snapshot of the latest event per asset. Returned vector is
    /// unordered — callers sort if they need a stable display order.
    pub fn latest_per_asset(&self) -> Vec<CryptoInventoryEvent> {
        self.latest.iter().map(|kv| kv.value().clone()).collect()
    }

    /// Latest event for one specific asset, if any.
    pub fn latest_for(&self, asset: &Asset, source_module: &str) -> Option<CryptoInventoryEvent> {
        self.latest
            .get(&DedupKey {
                source_module: source_module.into(),
                kind: asset.kind.clone(),
                identity: asset.identity.clone(),
            })
            .map(|kv| kv.value().clone())
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

    #[test]
    fn append_increments_count_and_latest_map() {
        let s = EventStore::new_in_memory();
        let t = chrono::Utc.with_ymd_and_hms(2026, 5, 13, 0, 0, 0).unwrap();
        s.append(ev("a", t, 10));
        s.append(ev("b", t, 20));
        assert_eq!(s.len(), 2);
        assert_eq!(s.latest_per_asset().len(), 2);
    }

    #[test]
    fn newer_event_replaces_latest_but_log_grows() {
        let s = EventStore::new_in_memory();
        let t1 = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t2 = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        s.append(ev("a", t1, 10));
        s.append(ev("a", t2, 80));
        assert_eq!(s.len(), 2, "log keeps both events");
        assert_eq!(s.latest_per_asset().len(), 1);
        let latest = &s.latest_per_asset()[0];
        assert_eq!(latest.posture.score, 80);
    }

    #[test]
    fn out_of_order_event_does_not_overwrite_latest() {
        let s = EventStore::new_in_memory();
        let t1 = chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t2 = chrono::Utc.with_ymd_and_hms(2026, 5, 1, 0, 0, 0).unwrap();
        // Insert newer first, then older.
        s.append(ev("a", t2, 80));
        s.append(ev("a", t1, 10));
        assert_eq!(s.len(), 2);
        let latest = &s.latest_per_asset()[0];
        assert_eq!(latest.posture.score, 80, "late-delivered older event must not overwrite");
    }

    #[test]
    fn recent_returns_in_reverse_arrival_order() {
        let s = EventStore::new_in_memory();
        let t = chrono::Utc.with_ymd_and_hms(2026, 5, 13, 0, 0, 0).unwrap();
        for id in ["a", "b", "c"] {
            s.append(ev(id, t, 0));
        }
        let r = s.recent(10);
        let ids: Vec<&str> = r.iter().map(|e| e.asset.identity.as_str()).collect();
        assert_eq!(ids, vec!["c", "b", "a"]);
    }
}
