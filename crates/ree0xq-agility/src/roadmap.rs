//! Org-level migration roadmap projector (SEZ-20).
//!
//! Given:
//! - an inventory snapshot (each asset's primitives + `q`
//!   today + BLOCKED flag), and
//! - a migration plan (which asset ids get which target
//!   primitives at which milestone),
//!
//! produces a per-milestone projection of `(org_q,
//! blocked_count, assets_migrated)`. The plan is a list of
//! milestones; each milestone is "by this date, these
//! assets should have moved to these primitives." The
//! projector simulates the rollup at each milestone in
//! chronological order and reports the trajectory.
//!
//! Out of scope here: re-running the q computation against
//! a hypothetical deadline horizon (the projection uses the
//! caller-supplied "today's" τ value to keep the projection
//! cheap and apples-to-apples across milestones).

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One asset's view as the projector sees it. Strict subset
/// of the `InventoryItem` shape ree0xq-server returns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSnapshot {
    pub identity: String,
    /// Today's `q` for the asset. Carries the per-asset
    /// rollup that ree0xq-server's posture module already
    /// computed; we treat it as input.
    pub q: f32,
    /// True when ree0xq-server flagged the asset BLOCKED
    /// (agility ≤ Locked).
    pub blocked: bool,
    /// Primitive names today (the rollup's input list).
    pub primitives: Vec<String>,
}

/// One milestone in the migration plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    /// Operator-chosen label (e.g. `"Q1-2027-fleet-cut"`).
    pub label: String,
    /// Calendar date the milestone projects to.
    pub date: DateTime<Utc>,
    /// Asset identities the milestone migrates.
    pub asset_ids: Vec<String>,
    /// Target primitive names each migrated asset adopts.
    /// The projector treats post-migration assets as
    /// `q = 0.10` (PQ-clean baseline) and `blocked = false`
    /// — a simplification that matches the paper's three-axis
    /// model when the new primitives are all PQ-safe.
    pub target_primitives: Vec<String>,
}

/// One row of the projection output.
#[derive(Debug, Clone, Serialize)]
pub struct MilestoneProjection {
    pub milestone: String,
    pub date: DateTime<Utc>,
    pub org_q_before: f32,
    pub org_q_after: f32,
    pub blocked_before: usize,
    pub blocked_after: usize,
    pub assets_migrated: usize,
    pub assets_remaining_classical: usize,
}

/// The full trajectory.
#[derive(Debug, Clone, Serialize)]
pub struct RoadmapProjection {
    pub today_org_q: f32,
    pub today_blocked: usize,
    pub total_assets: usize,
    pub projections: Vec<MilestoneProjection>,
}

/// Run the projection.
pub fn project_roadmap(
    inventory: &[AssetSnapshot],
    plan: &[Milestone],
) -> Result<RoadmapProjection> {
    let total = inventory.len();
    let today_q = avg_q(inventory);
    let today_blocked = inventory.iter().filter(|a| a.blocked).count();

    // Mutable working copy of each asset's state — the
    // projection mutates this through every milestone.
    let mut state: HashMap<String, AssetSnapshot> = inventory
        .iter()
        .map(|a| (a.identity.clone(), a.clone()))
        .collect();

    // Sort milestones by date so the trajectory is
    // chronologically coherent regardless of caller order.
    let mut sorted = plan.to_vec();
    sorted.sort_by_key(|m| m.date);

    let mut projections = Vec::with_capacity(sorted.len());
    for m in &sorted {
        let q_before = avg_q_state(&state);
        let blocked_before = state.values().filter(|a| a.blocked).count();
        let mut migrated_this_milestone = 0usize;
        for id in &m.asset_ids {
            if let Some(asset) = state.get_mut(id) {
                asset.q = 0.10; // PQ-clean baseline
                asset.blocked = false;
                asset.primitives = m.target_primitives.clone();
                migrated_this_milestone += 1;
            }
        }
        let q_after = avg_q_state(&state);
        let blocked_after = state.values().filter(|a| a.blocked).count();
        let remaining_classical = state.values().filter(|a| a.q >= 0.30).count();

        projections.push(MilestoneProjection {
            milestone: m.label.clone(),
            date: m.date,
            org_q_before: q_before,
            org_q_after: q_after,
            blocked_before,
            blocked_after,
            assets_migrated: migrated_this_milestone,
            assets_remaining_classical: remaining_classical,
        });
    }

    Ok(RoadmapProjection {
        today_org_q: today_q,
        today_blocked,
        total_assets: total,
        projections,
    })
}

fn avg_q(items: &[AssetSnapshot]) -> f32 {
    if items.is_empty() {
        return 0.0;
    }
    let sum: f32 = items.iter().map(|a| a.q).sum();
    sum / items.len() as f32
}

fn avg_q_state(state: &HashMap<String, AssetSnapshot>) -> f32 {
    if state.is_empty() {
        return 0.0;
    }
    let sum: f32 = state.values().map(|a| a.q).sum();
    sum / state.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn snap(id: &str, q: f32, blocked: bool) -> AssetSnapshot {
        AssetSnapshot {
            identity: id.into(),
            q,
            blocked,
            primitives: vec!["RSA-PKCS1-2048".into()],
        }
    }

    #[test]
    fn empty_plan_gives_today_unchanged() {
        let inv = vec![snap("a", 0.6, false), snap("b", 0.4, true)];
        let r = project_roadmap(&inv, &[]).unwrap();
        assert!((r.today_org_q - 0.5).abs() < 1e-6);
        assert_eq!(r.today_blocked, 1);
        assert_eq!(r.total_assets, 2);
        assert!(r.projections.is_empty());
    }

    #[test]
    fn migrating_all_assets_drops_org_q_toward_baseline() {
        let inv = vec![snap("a", 0.7, true), snap("b", 0.6, false)];
        let plan = vec![Milestone {
            label: "Q1".into(),
            date: Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap(),
            asset_ids: vec!["a".into(), "b".into()],
            target_primitives: vec!["ML-DSA-65".into()],
        }];
        let r = project_roadmap(&inv, &plan).unwrap();
        assert_eq!(r.projections.len(), 1);
        let p = &r.projections[0];
        assert_eq!(p.assets_migrated, 2);
        assert!(p.org_q_after < p.org_q_before);
        assert!((p.org_q_after - 0.10).abs() < 1e-6, "PQ-clean baseline");
        assert_eq!(p.blocked_after, 0, "migrated assets clear BLOCKED");
    }

    #[test]
    fn unknown_asset_id_is_silently_skipped() {
        let inv = vec![snap("a", 0.8, false)];
        let plan = vec![Milestone {
            label: "Q1".into(),
            date: Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap(),
            asset_ids: vec!["does-not-exist".into()],
            target_primitives: vec!["ML-DSA-65".into()],
        }];
        let r = project_roadmap(&inv, &plan).unwrap();
        assert_eq!(r.projections[0].assets_migrated, 0);
        // org_q stays at today's level.
        assert!((r.projections[0].org_q_after - 0.8).abs() < 1e-6);
    }

    #[test]
    fn milestones_are_sorted_by_date_regardless_of_input_order() {
        let inv = vec![snap("a", 0.9, false), snap("b", 0.9, false)];
        let plan = vec![
            // Caller passes Q2 before Q1; projector should
            // still report Q1 first.
            Milestone {
                label: "Q2".into(),
                date: Utc.with_ymd_and_hms(2027, 4, 1, 0, 0, 0).unwrap(),
                asset_ids: vec!["b".into()],
                target_primitives: vec!["ML-DSA-65".into()],
            },
            Milestone {
                label: "Q1".into(),
                date: Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap(),
                asset_ids: vec!["a".into()],
                target_primitives: vec!["ML-DSA-65".into()],
            },
        ];
        let r = project_roadmap(&inv, &plan).unwrap();
        assert_eq!(r.projections[0].milestone, "Q1");
        assert_eq!(r.projections[1].milestone, "Q2");
        // Trajectory is monotone: Q1 ends at avg(0.10, 0.9)
        // = 0.5, Q2 ends at avg(0.10, 0.10) = 0.10.
        assert!((r.projections[0].org_q_after - 0.5).abs() < 1e-6);
        assert!((r.projections[1].org_q_after - 0.10).abs() < 1e-6);
    }
}
