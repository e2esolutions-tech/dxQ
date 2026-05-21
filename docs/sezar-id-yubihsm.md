# sezar-id V4.3 — YubiHSM 2 + smart-card bring-up

YubiHSM 2 and PIV / OpenPGP smart cards are hardware-only
paths. The CI environment doesn't host either, so SEZ-18
closes the same way SEZ-3 (eBPF) and SEZ-16 (PKCS#11) did:
**runbook + reproducer-script**, with the operator
validating end-to-end on their target hardware.

The good news: both YubiHSM 2 and PIV smart cards expose
**PKCS#11-compatible interfaces**, so the existing
`sezar-id pkcs11-scan` subcommand from SEZ-16 works
directly. This doc captures the per-device bring-up
specifics.

## YubiHSM 2

Yubico ships `libykcs11.so`, a PKCS#11 frontend over the
YubiHSM 2's native protocol. After `yubihsm-shell` is
installed and the device is enrolled:

```bash
# 1. Find the library (yubihsm-pkcs11 package on most distros).
LIB=$(find /usr -name 'libykcs11.so' 2>/dev/null | head -1)
echo "YubiHSM PKCS#11 lib: $LIB"

# 2. Configure the connection. yubihsm-pkcs11 reads
#    /etc/yubihsm_pkcs11.conf or $YUBIHSM_PKCS11_CONF.
cat > /tmp/yubihsm-pkcs11.conf <<EOF
connector = http://127.0.0.1:12345
debug = false
timeout = 30
EOF
export YUBIHSM_PKCS11_CONF=/tmp/yubihsm-pkcs11.conf

# 3. PIN follows the auth-key convention
#    "<auth-key-id-decimal><password>", e.g. "0001password".
export SEZAR_HSM_PIN=0001password

# 4. Run sezar-id with the PKCS#11 feature.
./target/release/sezar-id pkcs11-scan \
    --library "$LIB" \
    --pin-env SEZAR_HSM_PIN \
    --collector http://127.0.0.1:8090/v1/events
```

A populated YubiHSM 2 returns one event per asymmetric key
+ each AES-256 wrap key. Identity is
`pkcs11:<token-label>/slot:<n>/<key-label>`; host is
`PKCS#11 YubiHSM`.

## PIV smart cards

OpenSC ships a generic PKCS#11 frontend over PIV:

```bash
LIB=/usr/lib64/opensc-pkcs11.so   # Fedora; /usr/lib/x86_64-linux-gnu/opensc-pkcs11.so on Debian
export SEZAR_HSM_PIN=123456       # default PIV PIN

./target/release/sezar-id pkcs11-scan \
    --library "$LIB" \
    --pin-env SEZAR_HSM_PIN \
    --collector http://127.0.0.1:8090/v1/events
```

PIV exposes a small fixed set of slots (Authentication,
Digital Signature, Key Management, …), each with at most
one key. The scanner produces an event per occupied slot.

## OpenPGP smart cards

OpenPGP cards (Yubikey OpenPGP applet, OpenPGP card v3.x,
…) are GnuPG-native; the easiest path is the OpenSC
PKCS#11 frontend with the `openpgp` driver, or the
dedicated `scute` module. Either way the `pkcs11-scan`
command shape is the same as the PIV path above; the
operator just points `--library` at the right `.so`.

## Reproducer script

[`scripts/sezar-id-bringup.sh`](../scripts/sezar-id-bringup.sh)
walks the pre-flight checks an operator most likely runs
before the scan:

- Vendor library on disk + executable?
- PKCS#11 config file present (where the vendor reads from)?
- PIN env var set?
- Sezar-server reachable?
- `pkcs11-tool --module <lib> --list-objects` returns at
  least one key?

Each check fails loudly with a remediation pointer; the
script is the closure gate for SEZ-18 acceptance.

## SEZ-18 closure rationale

Hardware-only validation can't run in CI; SEZ-3 (eBPF) and
SEZ-16 (PKCS#11) followed the same pattern. The
authoritative gate is the runbook + reproducer-script
pair — once an operator runs the script on their target
hardware, the four code paths exercised (lib load,
session open, object list, event emission) are the same
unit-tested paths that fire under SEZ-16's library-level
tests, just with a different `.so` on the other side.
