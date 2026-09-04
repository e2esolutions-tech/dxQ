//! `ree0xq-chain` — public-chain crypto monitor (V3).
//!
//! Three address-classifier backends ship in V3:
//!
//! - [`bitcoin`] — Bitcoin script-type classifier (P2PKH /
//!   P2SH / P2WPKH / P2WSH / P2TR). The script type
//!   determines whether the address spends with ECDSA or
//!   Schnorr over secp256k1 plus the SHA-256 / RIPEMD-160
//!   surface.
//! - [`ethereum`] — Ethereum address classifier. Every
//!   `0x` address implies secp256k1-ECDSA + Keccak-256.
//!   Contract-vs-EOA distinction is deferred to a future
//!   live-RPC backend; V3.1 treats every address as
//!   EOA-equivalent for the primitive classification.
//! - [`qrl`] — Quantum Resistant Ledger. QRL addresses
//!   imply XMSS hash-based signatures (PQ-safe, stateful).
//!   The point of including QRL is to prove the existing
//!   `crypto_inventory_event` schema doesn't fall over on
//!   hash-based, stateful PQ signatures.
//!
//! All three backends are **offline**: the operator hands
//! the binary an address list, the binary emits one event
//! per address. A live-RPC backend (block-range scanning
//! over a JSON-RPC endpoint) is out of scope for V3.0 /
//! V3.1 / V3.2 — operators can run a UTXO indexer into the
//! address list themselves until we ship that backend.

#![warn(rust_2018_idioms)]

pub mod bitcoin;
pub mod ethereum;
pub mod event;
pub mod qrl;

/// Module identifier emitted as `source_module` on every
/// event from this crate.
pub const MODULE_NAME: &str = "ree0xq-chain";
