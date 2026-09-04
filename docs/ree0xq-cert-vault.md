# ree0xq-cert V2.2 — Vault PKI bring-up

`ree0xq-cert vault-scan` lists every active cert under a
HashiCorp Vault PKI mount, fetches each PEM, and emits one
`crypto_inventory_event` per cert through the same library
parser the host-scan (V2.0) and CT-scan (V2.1) backends use.

## Host requirements

- Vault binary on `PATH` for the dev-mode bring-up below (or
  any Vault you already operate that runs ≥ 1.10).
- The ree0xq-cert release binary built (`cargo build --release
  -p ree0xq-cert`).
- The same `ree0xq-server` you point other agents at — the
  scanner POSTs events to its `/v1/events` endpoint.

## Five-minute reproducer (Vault dev mode)

```bash
# 1. Spin a single-node dev Vault.
vault server -dev -dev-root-token-id=ree0xq-dev-root &
export VAULT_ADDR=http://127.0.0.1:8200
export VAULT_TOKEN=ree0xq-dev-root

# 2. Enable the PKI secret engine.
vault secrets enable pki
vault secrets tune -max-lease-ttl=87600h pki

# 3. Generate a root CA.
vault write -field=certificate pki/root/generate/internal \
    common_name="ree0xQ Dev Root CA" \
    ttl=87600h > /tmp/ca.crt

# 4. Configure a role and issue a few leaf certs so we have
#    something to scan.
vault write pki/roles/example.com \
    allowed_domains="example.com" \
    allow_subdomains=true \
    max_ttl=720h
for n in api web db; do
    vault write pki/issue/example.com common_name="$n.example.com" ttl=720h >/dev/null
done

# 5. Scan. Token comes from $VAULT_TOKEN (the default
#    --token-env name).
./target/release/ree0xq-cert vault-scan \
    --addr http://127.0.0.1:8200 \
    --mount pki \
    --collector http://127.0.0.1:8090/v1/events

# 6. Verify on the collector side.
curl -s http://127.0.0.1:8090/v1/inventory | jq '.items[] | select(.asset_kind=="x509_cert") | {host, primitives}'
```

Expected output: four entries — the dev root plus the three
leaf certs we issued, each carrying a `Sig` + `Hash`
primitive split.

## Production deployment

- Replace `VAULT_TOKEN=ree0xq-dev-root` with an AppRole or a
  short-lived token scoped to `read` + `list` on the mount.
  The CLI's `--token-env` flag lets you keep that out of
  `ps auxw` and shell history.
- Vault's audit log records every list / read; expect noise
  proportional to the cert count × scan frequency. Tune
  `--rate-delay-ms` (default 250 ms) up if the audit
  pressure matters.
- The scanner reads from a single mount per invocation; loop
  the command over your mount list (`pki`, `pki_int`, …) or
  pass `--mount` multiple times — both work.
- Multi-tier setups: scan every leaf mount you care about.
  The root + intermediate mounts often hold only the CA's
  own cert; the leaves carry the certs issued to apps.

## Sample event

```json
{
  "schema_version": 1,
  "schema_minor": 1,
  "source_module": "ree0xq-cert",
  "observed_at": "2026-05-21T10:00:00Z",
  "asset": {
    "kind": "x509_cert",
    "identity": "sha256:7c1d…",
    "host": "api.example.com"
  },
  "primitives": [
    {"role": "sig",  "algorithm": "RSA-PKCS1",  "pq_resistant": false},
    {"role": "hash", "algorithm": "SHA-256",    "pq_resistant": true}
  ],
  "posture": {
    "score": 50,
    "rationale": "X.509 cert CN=api.example.com sig=RSA-PKCS1-SHA256 key=270 bytes; valid 2026-05-21T…..2026-06-20T…"
  }
}
```

## Troubleshooting

**`missing Vault token: env var \`VAULT_TOKEN\` is not set`.**
Set `VAULT_TOKEN` or pass `--token-env <YOUR_ENV>`. The
scanner does not read `~/.vault-token` on purpose — explicit
env var beats ambient state for an unattended agent.

**`vault LIST returned 403`.** The token doesn't have
permission on the mount. Verify with `vault token capabilities
<token> pki/certs`; you need at minimum `list` on
`<mount>/certs` and `read` on `<mount>/cert/*`.

**`vault LIST returned 404`.** Vault returns 404 when the
mount has no certs, not just when the mount itself is wrong.
The scanner treats 404 as an empty result and prints
`serials_listed=0`. If you expected certs, double-check the
mount path (`vault secrets list`).

**Certs are listed but none parse.** Vault wraps the PEM in a
JSON envelope (`{data: {certificate: "…"}}`). If the
deserialiser logs `vault GET JSON` errors, the mount is
returning something the scanner doesn't recognise — file an
issue with the raw Vault response.
