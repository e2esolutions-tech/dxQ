//! Shared `crypto_inventory_event` builder for chain
//! classifiers. Each backend (bitcoin / ethereum / qrl)
//! decomposes an address into a `Vec<Primitive>` plus a
//! rationale string, then hands them to [`build_event`].
//!
//! Identity convention: `<chain>:<address>` so events from
//! different chains can't collide on a long address that
//! happens to share a prefix.

use sezar_core::{
    Asset, AssetKind, CryptoInventoryEvent, Posture, Primitive, SCHEMA_MINOR, SCHEMA_VERSION,
};

use crate::MODULE_NAME;

/// Build one `blockchain_key` event.
pub fn build_event(
    chain: &str,
    address: &str,
    primitives: Vec<Primitive>,
    rationale: String,
) -> CryptoInventoryEvent {
    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: MODULE_NAME.into(),
        observed_at: chrono::Utc::now(),
        asset: Asset {
            kind: AssetKind::BlockchainKey,
            identity: format!("{chain}:{address}"),
            host: Some(chain.into()),
        },
        primitives,
        channel_protection: None,
        agility: None,
        posture: Posture {
            score: 50,
            rationale,
            recommended_replacement: None,
        },
    }
}

/// Read an address list from a file or stdin. One address
/// per line; blank lines and lines starting with `#` are
/// skipped. `-` reads stdin.
pub fn load_addresses(path: &str) -> anyhow::Result<Vec<String>> {
    use std::io::{BufRead, BufReader, Read};
    let raw = if path == "-" {
        let mut s = String::new();
        std::io::stdin().lock().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(path)?
    };
    let mut out = Vec::new();
    for line in BufReader::new(raw.as_bytes()).lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        out.push(trimmed.to_string());
    }
    Ok(out)
}
