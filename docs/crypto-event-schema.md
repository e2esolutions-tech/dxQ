# Crypto Inventory Event — Schema v1

The single load-bearing data structure in ree0xQ. Every module — from
the eBPF TLS sniffer to the Bitcoin mempool subscriber — emits this
shape. The dashboard, the rollup engine, the alert rules, and the
storage layer all read it. **Adding a new field that's required is a
schema-version bump.**

## Wire format

JSON over HTTP/POST in V1. CBOR / Protobuf considered as a follow-up
once we measure event volume in production (probably mid-V3).

## Top-level fields

```json
{
  "schema_version": 1,
  "source_module": "ree0xq-net",
  "observed_at": "2026-08-15T11:42:03.421Z",
  "asset": { ... },
  "primitives": [ ... ],
  "posture": { ... }
}
```

| Field            | Type     | Required | Notes |
|------------------|----------|----------|-------|
| `schema_version` | u32      | ✓        | Always equal to the emitting library's `SCHEMA_VERSION`. Consumers reject any event whose major schema-version they don't understand. |
| `source_module`  | string   | ✓        | One of `ree0xq-net`, `ree0xq-cert`, `ree0xq-chain`, `ree0xq-id`. Reserved for future modules: `ree0xq-dns` (forwarded from Nizam DNSSEC observations). |
| `observed_at`    | RFC3339  | ✓        | UTC. Module clock; the server records its own ingest time separately. |
| `asset`          | object   | ✓        | What we observed — see below. |
| `primitives`     | array    | ✓        | One entry per primitive role; an asset can list 1+ primitives. |
| `posture`        | object   | ✓        | ree0xQ's verdict. Computed by the emitting module using `ree0xq-core::rollup` — module is allowed to override if it has stronger context. |

## `asset`

```json
{
  "kind": "tls_session",
  "identity": "abc123-...",
  "host": "api.example.com"
}
```

| Field      | Type     | Required | Notes |
|------------|----------|----------|-------|
| `kind`     | enum     | ✓        | Closed set — see below. |
| `identity` | string   | ✓        | Stable identifier within the module. The combination `(source_module, kind, identity)` is the dedup key in the server's inventory store. |
| `host`     | string   | optional | Network-context hint. Hostname, IP, owner email — whatever the module can supply. Some events (mempool observations, root-of-trust HSM keys) have no meaningful `host`. |

### `asset.kind` enumeration

| Value             | Emitting module    | Identity meaning                                            |
|-------------------|--------------------|-------------------------------------------------------------|
| `tls_session`     | `ree0xq-net`        | hash(`client_random` ‖ `server_ip:port`)                    |
| `ssh_session`     | `ree0xq-net`        | hash(`session_id` from SSH transport)                       |
| `ipsec_sa`        | `ree0xq-net`        | SPI value                                                    |
| `x509_cert`       | `ree0xq-cert`       | `sha256` of the DER-encoded cert                             |
| `blockchain_key`  | `ree0xq-chain`      | chain-prefixed address (e.g. `bc1q...`, `0x...`)             |
| `hsm_slot`        | `ree0xq-id`         | PKCS#11 URI / cloud KMS ARN / vendor-specific URI            |
| `dns_dnssec`      | (forwarded; reserved) | RRSIG fingerprint                                          |

New variants are a major schema bump. Consumers that don't recognise a
kind store the event but render it under "unknown" — they don't reject
it.

## `primitives[]`

Each entry describes one cryptographic role. A typical TLS session
will list 4 primitives (kex / sig / encrypt / hash); a Bitcoin
holding only 1 (sig).

```json
{
  "role": "kex",
  "algorithm": "X25519",
  "parameters": { "curve": "Curve25519" },
  "pq_resistant": false,
  "nist_classification": null
}
```

| Field                 | Type    | Required | Notes |
|-----------------------|---------|----------|-------|
| `role`                | enum    | ✓        | `kex` / `sig` / `auth` / `encrypt` / `hash`. |
| `algorithm`           | string  | ✓        | Canonical name. Use IANA/RFC names where they exist; vendor names where they don't (and document in the module's README). |
| `parameters`          | object  | optional | Free-form. Curve name, key size, modes — whatever the algorithm needs. Consumers must tolerate unknown keys. |
| `pq_resistant`        | bool    | optional | `null` = unknown. `true` only when the primitive is believed PQ-secure under standard assumptions. Symmetric primitives at ≥256 bits get `true` (Grover-resistant). |
| `nist_classification` | enum    | optional | `L1` / `L3` / `L5` per FIPS 203/204/205. Set only for PQ primitives where the standard explicitly assigns a level. |

### Algorithm naming guide

- **Classical asymmetric:** `ECDSA-P256`, `ECDSA-secp256k1`, `RSA-2048`, `Ed25519`, `X25519`, `DH-2048`.
- **Post-quantum:** `Kyber512` / `Kyber768` / `Kyber1024`, `ML-KEM-512` / `ML-KEM-768` / `ML-KEM-1024` (the FIPS-203 names are preferred), `Dilithium2` / `Dilithium3` / `Dilithium5`, `ML-DSA-44` / `ML-DSA-65` / `ML-DSA-87`, `SPHINCS+-SHA2-128s`, `Falcon-512`.
- **Symmetric:** `AES-128-GCM`, `AES-256-GCM`, `ChaCha20-Poly1305`, `AES-256-CBC` (deprecated; flag).
- **Hash:** `SHA-256`, `SHA-384`, `SHA3-256`, `SHA-1` (deprecated; flag), `MD5` (deprecated; flag).

When in doubt, prefer the IETF / NIST official name over a colloquial
one. `Dilithium` is being renamed to `ML-DSA` upstream — emit
`ML-DSA-65` and the schema's PQ classifier will pick it up; emitting
`Dilithium3` also works (alias).

## `posture`

```json
{
  "score": 40,
  "rationale": "X25519 + ECDSA-P256 are classical-only",
  "recommended_replacement": "Kyber768 + ML-DSA-65 hybrid"
}
```

| Field                       | Type    | Required | Notes |
|-----------------------------|---------|----------|-------|
| `score`                     | u8 0–100 | ✓       | 100 = fully PQ-ready. Computed per asset; org rollup is a separate dashboard concept. |
| `rationale`                 | string  | ✓        | Single sentence. Renders verbatim in the dashboard. |
| `recommended_replacement`   | string  | optional | What to migrate to. `null` when already at the recommended primitive or when the rollup engine doesn't have a recommendation yet. |

## Module emission contract

A module is required to:

1. Set `schema_version` to its compile-time `SCHEMA_VERSION` constant.
2. Set `source_module` to the canonical module name (`ree0xq-net`, etc.).
3. Set `observed_at` to actual observation time, **not** event-emission
   time. Late delivery is fine; backdated events are not.
4. Provide every required field on every event. If something is
   genuinely unknown, emit `null` for the optional fields rather than
   leaving them out.
5. Compute `posture` using `ree0xq-core::rollup::score(&primitives)`.
   Override only with explicit reason in the `rationale` string.

Modules **must not**:

- Add top-level fields without coordinated schema bump.
- Emit duplicate events for the same `(asset.kind, asset.identity)`
  within a 60-second window unless the primitive list materially
  changed.
- Carry private key material. ree0xQ is observability — fingerprints
  and metadata only.

## Future extensions (planned)

- **Confidence score** on primitive detection (e.g. heuristic vs.
  byte-perfect parse). Optional field; additive, non-breaking.
- **Asset relationships** (cert-issued-by-CA, key-rotates-from-key)
  via a separate `links[]` array. Probably ships in V4.
- **Telemetry context** (host fingerprint, agent version) as a
  metadata block. V2.

## Validation

The `ree0xq-core` crate ships JSON Schema generation for this struct
(via `schemars`, when added). CI will validate every example event in
this doc against the generated schema. Until that lands, the
`#[test]` in `crates/ree0xq-core/src/lib.rs` (`event_round_trips_through_json`)
is the canonical proof that the Rust struct + JSON shape agree.
