//! Integration test: parse the bundled multi-record zgrab2 fixture
//! and validate the resulting events.
//!
//! The fixture covers three realistic posture cases:
//!
//! 1. Modern PQ-capable host (TLS 1.3, X25519MLKEM768, ECDSA-P256 cert).
//! 2. Classical modern host (TLS 1.2, ECDHE+RSA+AES-128-GCM, RSA-SHA256 cert).
//! 3. Legacy host (TLS 1.0, RSA+RC4+SHA1).

use sezar_net::zgrab::{event_from_zgrab, ZgrabRecord};

fn load_fixture() -> Vec<ZgrabRecord> {
    let raw = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/zgrab-tls13-pq.json"
    ))
    .expect("fixture readable");
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("fixture parses"))
        .collect()
}

#[test]
fn pq_capable_host_yields_pq_kex_and_classical_sig() {
    let records = load_fixture();
    let modern = records
        .iter()
        .find(|r| r.domain.as_deref() == Some("api.example.com"))
        .unwrap();
    let ev = event_from_zgrab(modern);
    let algos: Vec<&str> = ev.primitives.iter().map(|p| p.algorithm.as_str()).collect();
    assert!(algos.contains(&"X25519+ML-KEM-768"), "got {algos:?}");
    assert!(algos.contains(&"AES-256-GCM"), "got {algos:?}");
    assert!(algos.contains(&"SHA-384"), "got {algos:?}");
    // Cert sig: ECDSA-P256
    assert!(algos.contains(&"ECDSA-P256"), "got {algos:?}");
}

#[test]
fn classical_modern_host_yields_full_tls12_role_set() {
    let records = load_fixture();
    let classical = records
        .iter()
        .find(|r| r.domain.as_deref() == Some("legacy.example.com"))
        .unwrap();
    let ev = event_from_zgrab(classical);
    let algos: Vec<&str> = ev.primitives.iter().map(|p| p.algorithm.as_str()).collect();
    assert!(algos.contains(&"ECDHE"), "got {algos:?}");
    assert!(algos.contains(&"RSA"), "got {algos:?}");
    assert!(algos.contains(&"AES-128-GCM"), "got {algos:?}");
    assert!(algos.contains(&"SHA-256"), "got {algos:?}");
    assert!(algos.contains(&"RSA-PKCS1-SHA256"), "got {algos:?}");
}

#[test]
fn legacy_host_recovers_deprecated_signals() {
    let records = load_fixture();
    let ancient = records
        .iter()
        .find(|r| r.domain.as_deref() == Some("ancient.example.com"))
        .unwrap();
    let ev = event_from_zgrab(ancient);
    let algos: Vec<&str> = ev.primitives.iter().map(|p| p.algorithm.as_str()).collect();
    assert!(algos.contains(&"RC4"), "got {algos:?}");
    assert!(algos.contains(&"RSA-KEX"), "got {algos:?}");
    // Note: SHA-1 from the cert sig algorithm comes through:
    assert!(algos.contains(&"RSA-PKCS1-SHA1"), "got {algos:?}");
}

#[test]
fn events_carry_distinct_identities_per_host() {
    let records = load_fixture();
    let mut ids = std::collections::HashSet::new();
    for r in &records {
        let ev = event_from_zgrab(r);
        assert!(
            ids.insert(ev.asset.identity.clone()),
            "duplicate identity: {}",
            ev.asset.identity
        );
    }
}
