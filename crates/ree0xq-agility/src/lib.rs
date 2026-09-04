//! `ree0xq-agility` — static crypto-agility scanner.
//!
//! Classifies assets on the five-level scale defined in
//! [`ree0xq_core::AgilityLevel`] (`Negotiated` / `Configurable` /
//! `Pinned` / `Locked` / `Frozen`) from static analysis of
//! configuration files, source code, installed packages, and binary
//! strings.
//!
//! # Scoring policy
//!
//! - The scanner collects *evidence* per asset.
//! - Each rule annotates the evidence with a *contributed level*.
//! - The final level is `min(level)` across evidence — the weakest
//!   link governs, on the principle that an asset is only as agile
//!   as its hardest-to-change cryptographic surface.
//! - When evidence is contradictory the scanner emits the
//!   conservative-min level and logs the dissent for operator review.
//! - When no evidence is collected the scanner emits `level: unknown`
//!   (the consumer side maps this to `Pinned` for posture-rollup
//!   purposes — see [`UNKNOWN_LEVEL_FALLBACK`]).
//!
//! # Layering
//!
//! ```text
//!   bin/agility.rs    →    scanner::scan_target(...)
//!                                │
//!                                ├─ rules::load_ruleset(dir)
//!                                ├─ scanner::collect_evidence(...)
//!                                └─ scanner::aggregate_level(...)
//!                                          │
//!                                          ▼
//!                                  ree0xq_core::AgilityBlock
//! ```

#![warn(rust_2018_idioms)]

pub mod rules;
pub mod scanner;

// V5 — PQ-migration recommendations engine. Sits on top of
// the static agility scanner (which produces the G-axis
// signal) and consumes ree0xq-server's `/v1/inventory` (or a
// local snapshot) to surface:
//
// - per-asset replacement recommendations (SEZ-19)
// - org-level migration-roadmap projections (SEZ-20)
// - TLS-stack compatibility matrix lookups (SEZ-21)
// - regulator deadline tracking per jurisdiction (SEZ-22)
pub mod compat;
pub mod deadlines;
pub mod recommend;
pub mod roadmap;

use ree0xq_core::AgilityLevel;

/// Identifier for this agent in the `source_module` field of emitted
/// events.
pub const SOURCE_MODULE: &str = "ree0xq-agility";

/// When the scanner finds no evidence, posture consumers treat the
/// asset at this level. Conservative-by-default; matches the paper's
/// §6.5 backwards-compatibility note.
pub const UNKNOWN_LEVEL_FALLBACK: AgilityLevel = AgilityLevel::Pinned;

/// Current scanner version, embedded into every emitted block so
/// reviewers can correlate findings to a specific build.
pub const SCANNER_VERSION: &str = concat!("ree0xq-agility/", env!("CARGO_PKG_VERSION"));

/// Current rubric version. Bumped when the scoring scale or the
/// canonical evidence-to-level mapping changes in a non-additive way.
pub const RUBRIC_VERSION: &str = "qra-rubric/v1.0";
