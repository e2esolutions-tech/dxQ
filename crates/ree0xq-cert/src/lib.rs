//! `ree0xq-cert` — X.509 inventory module (V2).
//!
//! Three planned data sources (see ROADMAP.md):
//! - **Host scan** ([`scan`]): walks well-known cert paths on
//!   the local filesystem (`/etc/ssl`, `/etc/letsencrypt/live`,
//!   …) and emits one `crypto_inventory_event` per cert.
//!   Shipped in V2.0 (SEZ-9). Default backend.
//! - **CT log scan**: pulls every cert issued for a customer's
//!   domains from a public Certificate Transparency log
//!   (crt.sh). V2.1 (SEZ-10).
//! - **Internal CA scan**: HashiCorp Vault PKI in V2.2
//!   (SEZ-11); AD CS and ACME backends follow.
//!
//! The cert parser ([`cert`]) is shared across all three
//! backends — given a DER cert, it produces the
//! `CryptoInventoryEvent` the collector ingests, with
//! signature primitives and hash primitives surfaced
//! separately so the rollup can see both.

#![warn(rust_2018_idioms)]

pub mod cert;
pub mod ct;
pub mod scan;
pub mod vault;

/// Module identifier emitted as `source_module` on every event.
pub const MODULE_NAME: &str = "ree0xq-cert";
