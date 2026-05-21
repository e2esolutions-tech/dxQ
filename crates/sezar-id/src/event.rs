//! Shared `crypto_inventory_event` builder for sezar-id
//! backends. Each backend collects per-key metadata and
//! hands it to [`build_event`].

use sezar_core::{
    Asset, AssetKind, CryptoInventoryEvent, Posture, Primitive, SCHEMA_MINOR, SCHEMA_VERSION,
};

use crate::MODULE_NAME;

/// Build one `hsm_slot` event from the backend's view of a
/// single key.
pub fn build_event(
    identity: String,
    host: Option<String>,
    primitives: Vec<Primitive>,
    rationale: String,
) -> CryptoInventoryEvent {
    CryptoInventoryEvent {
        schema_version: SCHEMA_VERSION,
        schema_minor: SCHEMA_MINOR,
        source_module: MODULE_NAME.into(),
        observed_at: chrono::Utc::now(),
        asset: Asset {
            kind: AssetKind::HsmSlot,
            identity,
            host,
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
