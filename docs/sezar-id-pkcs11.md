# sezar-id V4.1 — PKCS#11 backend bring-up

`sezar-id pkcs11-scan` opens a vendor PKCS#11 library, walks
every slot's public and secret-key objects, and emits one
`crypto_inventory_event` per key. The same library code that
backs the offline classifier (`crate::algos::primitives_for`)
maps `CKA_KEY_TYPE` + `CKA_MODULUS_BITS` / `CKA_EC_PARAMS`
onto the primitive list — so PKCS#11 events look identical
to offline events on the collector side.

This document is the operator runbook. End-to-end live
validation is host-side because the CI environment doesn't
ship with a vendor PKCS#11 library; the
[`scripts/sezar-id-bringup.sh`](../scripts/sezar-id-bringup.sh)
script codifies the pre-flight checks.

## Host requirements

| Requirement                  | Why                                                            |
|------------------------------|----------------------------------------------------------------|
| A vendor PKCS#11 library     | `cryptoki` opens a vendor `.so` at run time.                   |
| `softhsm2-util` (for testing)| Quickest way to mint a token + a few keys without an HSM.      |
| C toolchain                  | `cryptoki` uses bindgen on the PKCS#11 headers.                |

## One-time install (SoftHSM on Fedora / Debian)

```bash
# Fedora
sudo dnf install softhsm softhsm-devel

# Debian / Ubuntu
sudo apt install softhsm2 libsofthsm2-dev

# Initialise a fresh token in your home softhsm slot.
softhsm2-util --init-token --free --label sezar-test --pin 1234 --so-pin 5678
```

## Bring-up

```bash
# 1. Build sezar-id with the PKCS#11 feature.
cargo build --release -p sezar-id --features pkcs11

# 2. Find the SoftHSM library on your host.
LIB=$(find /usr -name 'libsofthsm2.so' 2>/dev/null | head -1)
echo "PKCS#11 lib: $LIB"

# 3. Mint a couple of keys so the scan has something to find.
pkcs11-tool --module "$LIB" --login --pin 1234 --keypairgen \
    --key-type RSA:4096 --label sezar-rsa-test
pkcs11-tool --module "$LIB" --login --pin 1234 --keypairgen \
    --key-type EC:secp256r1 --label sezar-ec-test

# 4. Run the scanner. The pin lives in an env var so it
#    doesn't show up in `ps`.
export SEZAR_HSM_PIN=1234
./target/release/sezar-id pkcs11-scan \
    --library "$LIB" \
    --pin-env SEZAR_HSM_PIN \
    --collector http://127.0.0.1:8090/v1/events
```

Expected log lines:

```
[sezar_id] pkcs11-scan starting library="…/libsofthsm2.so" slot=None pin=true
[sezar_id] pkcs11-scan complete slots_seen=1 objects_seen=2 events_emitted=2
```

`GET /v1/inventory` on the collector then returns two
`hsm_slot` entries — one RSA-4096, one ECDSA-P256.

## Production deployment

- Replace SoftHSM with the actual vendor library
  (e.g. `/usr/lib/nfast/libnfkmcryptoki.so` for Thales
  nShield, `/usr/lib/luna/libCryptoki2.so` for SafeNet
  Luna, the Yubico module for YubiHSM 2).
- Use a session-scoped PKCS#11 user PIN rather than the SO
  PIN; the SO PIN can mint keys, the User PIN only reads
  them, which matches sezar-id's read-only contract.
- Run sezar-id under systemd with
  `AmbientCapabilities=` empty and the PKCS#11 device
  whitelisted via `DeviceAllow=`.
- The scanner is idempotent: re-runs on the same HSM
  produce the same `asset.identity`
  (`pkcs11:<token>/slot:<id>/<label>`), so the per-asset
  latest-event map in sezar-server deduplicates correctly.

## Troubleshooting

**`load PKCS#11 lib`.** `cryptoki` can't open the `.so`.
Most common causes: wrong path, missing executable bit on
the library, SELinux blocking the open. Try `dlopen` from
a tiny C program first to isolate.

**`C_Initialize`.** The library is loaded but rejected the
initialization. Vendor-specific — check the vendor's
configuration file (`softhsm2.conf`, `cs_pkcs11_R3.cfg`,
…) is on `$PATH` or pointed at via the vendor's env var.

**`C_Login`.** The PIN env var is empty or wrong. Run
`pkcs11-tool --module $LIB --login --pin "$SEZAR_HSM_PIN"
--list-objects` to isolate.

**Empty inventory but keys exist.** sezar-id walks
`CKO_PUBLIC_KEY` and `CKO_SECRET_KEY` objects only — private
keys whose public half is *not* present on the token
(unusual) aren't visible. Generate the public half or
import it, then re-scan.

## SEZ-16 closure rationale

The kernel-side PKCS#11 surface, the per-key classifier
plumbing, and the CLI all live behind the `pkcs11` feature
in this commit, exercised by the cryptoki-typed library
code without a live HSM. Per-vendor live validation is
host-side because the CI environment doesn't have SoftHSM
or any vendor PKCS#11 library installed; this runbook is
the authoritative gate.
