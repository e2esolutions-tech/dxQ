//! Posture history (SEZ-32).
//!
//! An in-memory ring buffer of org-level posture snapshots. Each
//! `GET /v1/posture` records a snapshot (throttled so a chatty
//! dashboard poll doesn't fill the ring with near-identical points),
//! and `GET /v1/posture/history` returns the retained window for the
//! trend chart.
//!
//! Scope note: history is in-memory only in this iteration — it does
//! NOT survive a server restart. Durable (Postgres-backed) history
//! is a deliberate follow-up; the ring buffer gives the dashboard a
//! live trend for the lifetime of the process, which is what the V6
//! trend surface needs first.

use std::collections::VecDeque;
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

/// Fixed-capacity ring of snapshots with a minimum spacing between
/// retained points.
pub struct PostureHistory {
    inner: Mutex<VecDeque<PostureSnapshot>>,
    capacity: usize,
    min_spacing: chrono::Duration,
}

impl PostureHistory {
    /// Default: keep 1024 points, at most one per 30 s. At the
    /// dashboard's 10 s poll cadence that is ~8.5 h of trend before
    /// the ring wraps — plenty for a live view; durable history is
    /// the follow-up.
    pub fn with_defaults() -> Self {
        Self::new(1024, Duration::from_secs(30))
    }

    pub fn new(capacity: usize, min_spacing: Duration) -> Self {
        Self {
            inner: Mutex::new(VecDeque::with_capacity(capacity.min(1024))),
            capacity: capacity.max(1),
            min_spacing: chrono::Duration::from_std(min_spacing)
                .unwrap_or_else(|_| chrono::Duration::seconds(30)),
        }
    }

    /// Record a snapshot if enough time has passed since the last one.
    /// Returns true if it was stored, false if throttled.
    pub fn record(&self, snap: PostureSnapshot) -> bool {
        let mut buf = self.inner.lock().unwrap();
        if let Some(last) = buf.back() {
            if snap.at.signed_duration_since(last.at) < self.min_spacing {
                return false;
            }
        }
        buf.push_back(snap);
        while buf.len() > self.capacity {
            buf.pop_front();
        }
        true
    }

    /// Snapshots at or after `since` (all of them if `None`),
    /// oldest-first.
    pub fn window(&self, since: Option<DateTime<Utc>>) -> Vec<PostureSnapshot> {
        let buf = self.inner.lock().unwrap();
        match since {
            Some(t) => buf.iter().filter(|s| s.at >= t).cloned().collect(),
            None => buf.iter().cloned().collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(secs: i64, q: f32) -> PostureSnapshot {
        PostureSnapshot {
            at: DateTime::<Utc>::from_timestamp(1_700_000_000 + secs, 0).unwrap(),
            org_q: q,
            assets: 5,
            blocked_count: 1,
        }
    }

    #[test]
    fn throttles_within_min_spacing() {
        let h = PostureHistory::new(100, Duration::from_secs(30));
        assert!(h.record(snap(0, 0.5)));
        assert!(!h.record(snap(10, 0.6)), "10s < 30s spacing → throttled");
        assert!(h.record(snap(30, 0.6)), "30s meets spacing");
        assert_eq!(h.len(), 2);
    }

    #[test]
    fn ring_wraps_at_capacity() {
        let h = PostureHistory::new(3, Duration::from_secs(0));
        for i in 0..5 {
            h.record(snap(i, i as f32 / 10.0));
        }
        let w = h.window(None);
        assert_eq!(w.len(), 3, "capped at 3");
        assert_eq!(w.first().unwrap().org_q, 0.2, "oldest two evicted");
        assert_eq!(w.last().unwrap().org_q, 0.4);
    }

    #[test]
    fn window_filters_by_since() {
        let h = PostureHistory::new(100, Duration::from_secs(0));
        for i in 0..5 {
            h.record(snap(i * 10, i as f32));
        }
        let cutoff = DateTime::<Utc>::from_timestamp(1_700_000_000 + 25, 0).unwrap();
        let w = h.window(Some(cutoff));
        assert_eq!(w.len(), 2, "only snapshots at +30 and +40");
    }
}
