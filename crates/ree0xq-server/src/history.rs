//! Posture-trend snapshots (SEZ-32).
//!
//! The org-level `q` score is a point-in-time number. To draw a
//! trend we persist a snapshot on every `GET /v1/posture` read and
//! serve the retained series at `GET /v1/posture/history`.
//!
//! Storage lives in the [`crate::store::EventStore`] so both backends
//! participate: the in-memory store keeps a bounded ring (lost on
//! restart, which is what "in-memory" means), and the Postgres store
//! writes a durable `posture_snapshots` row so the trend survives a
//! restart. This module owns only the snapshot *type* and the
//! write-rate gate; the ring/table live behind the store trait.

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One org-posture observation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PostureSnapshot {
    pub at: DateTime<Utc>,
    pub org_q: f32,
    pub assets: usize,
    pub blocked_count: usize,
}

/// Default retained-series cap. Shared by the in-memory ring and as
/// the default read limit; at the dashboard's 10 s poll cadence and
/// the 30 s write gate below, 1024 points is ~8.5 h of trend.
pub const DEFAULT_CAPACITY: usize = 1024;

/// Minimum spacing between two retained snapshots. A chatty
/// dashboard polling `/v1/posture` every few seconds would otherwise
/// fill the series with near-identical points; this gate keeps at
/// most one snapshot per interval regardless of poll rate.
///
/// The gate is deliberately storage-agnostic: it decides *whether* a
/// read should persist a snapshot, then the store decides *where*.
pub struct SnapshotThrottle {
    last: Mutex<Option<DateTime<Utc>>>,
    min_spacing: chrono::Duration,
}

impl SnapshotThrottle {
    /// Default: at most one snapshot per 30 s.
    pub fn with_defaults() -> Self {
        Self::new(Duration::from_secs(30))
    }

    pub fn new(min_spacing: Duration) -> Self {
        Self {
            last: Mutex::new(None),
            min_spacing: chrono::Duration::from_std(min_spacing)
                .unwrap_or_else(|_| chrono::Duration::seconds(30)),
        }
    }

    /// Returns true (and arms the gate at `now`) if enough time has
    /// passed since the last admitted snapshot; false if the caller
    /// should skip persisting this one.
    pub fn admit(&self, now: DateTime<Utc>) -> bool {
        let mut last = self.last.lock().unwrap();
        if let Some(prev) = *last {
            if now.signed_duration_since(prev) < self.min_spacing {
                return false;
            }
        }
        *last = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_700_000_000 + secs, 0).unwrap()
    }

    #[test]
    fn admits_first_then_throttles_within_spacing() {
        let t = SnapshotThrottle::new(Duration::from_secs(30));
        assert!(t.admit(at(0)), "first snapshot always admitted");
        assert!(!t.admit(at(10)), "10s < 30s spacing → throttled");
        assert!(t.admit(at(30)), "30s meets spacing");
        assert!(!t.admit(at(45)), "15s after last admit → throttled");
    }

    #[test]
    fn zero_spacing_admits_everything() {
        let t = SnapshotThrottle::new(Duration::from_secs(0));
        for i in 0..5 {
            assert!(t.admit(at(i)), "zero spacing never throttles");
        }
    }
}
