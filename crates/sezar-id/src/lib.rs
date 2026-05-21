//! `sezar-id` — HSM / KMS / smart-card inventory (V4).
//!
//! Four backends (one default + three gated):
//!
//! - [`inventory`] — offline JSON classifier. Operator
//!   hands in a file describing HSMs, slots, and keys; the
//!   classifier emits one `crypto_inventory_event` per
//!   (slot, key) pair. Default; no extra feature flag.
//! - [`pkcs11`] — live PKCS#11 backend over `cryptoki`.
//!   Gated behind the `pkcs11` Cargo feature because the
//!   runtime needs a vendor PKCS#11 library (libsofthsm,
//!   libnss3, or the HSM's proprietary `.so`). End-to-end
//!   live validation is operator-side; see
//!   `docs/sezar-id-pkcs11.md`.
//! - [`aws_kms`] — AWS KMS backend over `aws-sdk-kms`.
//!   Gated behind the `aws-kms` feature. The trait stays
//!   narrow so GCP KMS and Azure Key Vault impls drop in
//!   for V4.x.
//! - YubiHSM 2 + PIV / OpenPGP smart cards land as
//!   operator runbooks under `docs/sezar-id-yubihsm.md`
//!   and `docs/sezar-id-smartcard.md`; the
//!   hardware-dependent paths follow the SEZ-3 / SEZ-11
//!   "runbook + reproducer-script" closure.

#![warn(rust_2018_idioms)]

pub mod algos;
pub mod event;
pub mod inventory;

#[cfg(feature = "pkcs11")]
pub mod pkcs11;

#[cfg(feature = "aws-kms")]
pub mod aws_kms;

/// Module identifier emitted as `source_module` on every
/// event from this crate.
pub const MODULE_NAME: &str = "sezar-id";
