//! `sezar-net` — TLS / SSH / IPsec ciphersuite observation.
//!
//! # Phase 1 (this release)
//!
//! Static / ingest-based observation:
//!
//! - [`tls`] — TLS handshake byte parser. Extracts the ClientHello /
//!   ServerHello fields needed to classify a session under the
//!   three-axis posture model.
//! - [`algos`] — canonical mapping from observed wire identifiers
//!   (IANA ciphersuites, supported_groups, signature_algorithms) to
//!   `sezar_core::Primitive` records.
//! - [`zgrab`] — adapter that consumes the JSON output of the
//!   ZMap project's `zgrab2` scanner and emits
//!   [`CryptoInventoryEvent`] records. This is the ingest path
//!   exercised by the paper's Study 1.
//!
//! # Phase 2 (planned, repo issue `#SEZ-3`)
//!
//! Live eBPF observation via `aya`. Until that lands the module
//! takes its observations from `zgrab2` JSON or libpcap captures.

#![deny(missing_docs)]
#![warn(rust_2018_idioms)]

pub mod algos;
pub mod live;
#[cfg(feature = "live-interface")]
pub mod live_iface;
pub mod pq_probe;
pub mod spool;
pub mod tls;
pub mod zgrab;

use sezar_core::{
    Asset, AssetKind, CryptoInventoryEvent, Posture, Primitive, SCHEMA_MINOR, SCHEMA_VERSION,
};

/// Module identifier emitted as `source_module` on every event.
pub const MODULE_NAME: &str = "sezar-net";

/// Build a `tls_session` event from the host + primitives that the
/// TLS parser (or zgrab adapter) recovered. The posture rationale
/// is a one-liner derived from the primitive list — full rollup is
/// the responsibility of `sezar-core`.
pub fn build_tls_event(
    host: impl Into<String>,
    identity: impl Into<String>,
    primitives: Vec<Primitive>,
) -> CryptoInventoryEvent {
    let rationale = if primitives.is_empty() {
        "TLS handshake observed but no primitives extracted".into()
    } else {
        format!(
            "{} primitive(s) observed: {}",
            primitives.len(),
            primitives
                .iter()
                .map(|p| p.algorithm.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: MODULE_NAME.into(),
        observed_at: chrono::Utc::now(),
        asset: Asset {
            kind: AssetKind::TlsSession,
            identity: identity.into(),
            host: Some(host.into()),
        },
        primitives,
        channel_protection: None,
        agility: None,
        posture: Posture {
            // Placeholder; downstream consumers may recompute via
            // `sezar-core::rollup` once that lives outside this crate.
            score: 50,
            rationale,
            recommended_replacement: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_tls_event_carries_module_identity_and_primitives() {
        use sezar_core::PrimitiveRole;
        let prims = vec![Primitive {
            role: PrimitiveRole::Kex,
            algorithm: "X25519".into(),
            parameters: Default::default(),
            pq_resistant: Some(false),
            nist_classification: None,
        }];
        let ev = build_tls_event("example.com", "tls-abc", prims);
        assert_eq!(ev.source_module, MODULE_NAME);
        assert_eq!(ev.asset.kind, AssetKind::TlsSession);
        assert_eq!(ev.primitives.len(), 1);
        // Round-trip through JSON to catch any skip_serializing regressions.
        let j = serde_json::to_string(&ev).unwrap();
        assert!(j.contains("X25519"));
        assert!(j.contains("sezar-net"));
    }

    #[test]
    fn build_tls_event_with_no_primitives_still_validates() {
        let ev = build_tls_event("example.com", "tls-empty", vec![]);
        assert_eq!(ev.primitives.len(), 0);
        assert!(ev.posture.rationale.contains("no primitives"));
    }
}
