# Posture Rollup — How ree0xQ Scores

Two rollup levels:

1. **Per-asset** — an event arrives with a `primitives` list; the
   rollup library returns a 0–100 score and a one-sentence
   `rationale`. Computed by the emitting module before the event hits
   the wire (so the dashboard can paginate without re-running rollups).
2. **Org-level** — across all assets, weighted by asset class, surfaced
   on the main dashboard. Computed by `ree0xq-server` on read.

Both share the same primitive-classification table.

## Primitive classification table (V1)

For each known algorithm we record:

| Field        | Meaning                                                |
|--------------|--------------------------------------------------------|
| `pq_status`  | `pq` / `pq_hybrid` / `classical` / `deprecated` / `unknown` |
| `weight`     | how heavily this primitive's status counts toward the asset score |
| `replacement` | recommended migration (if any) |

V1 ships with these classifications:

### Asymmetric — key exchange

| Algorithm        | `pq_status`   | Replacement                      |
|------------------|---------------|----------------------------------|
| `X25519`         | classical     | `Kyber768+X25519` hybrid         |
| `ECDH-P256`      | classical     | `ML-KEM-768+X25519` hybrid       |
| `ECDH-P384`      | classical     | `ML-KEM-1024+X448` hybrid        |
| `DH-2048`        | deprecated    | `ML-KEM-768+X25519` hybrid       |
| `Kyber512` / `ML-KEM-512` | pq | (already current)                |
| `Kyber768` / `ML-KEM-768` | pq | (already current)                |
| `Kyber1024` / `ML-KEM-1024` | pq | (already current)              |
| `Kyber768+X25519` | pq_hybrid    | (recommended hybrid)             |

### Asymmetric — signatures

| Algorithm                     | `pq_status` | Replacement                |
|-------------------------------|-------------|----------------------------|
| `ECDSA-P256`                  | classical   | `ML-DSA-65`                |
| `ECDSA-secp256k1`             | classical   | `ML-DSA-65` or `SLH-DSA`   |
| `ECDSA-P384`                  | classical   | `ML-DSA-87`                |
| `Ed25519`                     | classical   | `ML-DSA-65`                |
| `RSA-2048`                    | classical   | `ML-DSA-65`                |
| `RSA-3072`                    | classical   | `ML-DSA-87`                |
| `RSA-1024`                    | deprecated  | `ML-DSA-65` (urgent)       |
| `Dilithium2` / `ML-DSA-44`    | pq          | (already current)          |
| `Dilithium3` / `ML-DSA-65`    | pq          | (already current)          |
| `Dilithium5` / `ML-DSA-87`    | pq          | (already current)          |
| `SPHINCS+-*` / `SLH-DSA-*`    | pq          | (already current)          |
| `Falcon-512` / `FN-DSA-512`   | pq          | (already current)          |

### Symmetric

| Algorithm            | `pq_status` | Replacement |
|----------------------|-------------|-------------|
| `AES-128-GCM`        | classical (Grover-weak) | `AES-256-GCM` |
| `AES-256-GCM`        | pq          | (Grover-resistant at 256 bits) |
| `ChaCha20-Poly1305`  | pq          | (256-bit key) |
| `AES-256-CBC`        | deprecated (CBC + padding oracles) | `AES-256-GCM` |
| `RC4`, `3DES`        | deprecated  | `AES-256-GCM` |

### Hash

| Algorithm | `pq_status` | Replacement |
|-----------|-------------|-------------|
| `SHA-256` | pq          | (256-bit hash, Grover-resistant for collision) |
| `SHA-384` | pq          | (already current) |
| `SHA3-256` / `SHA3-384` | pq | (already current) |
| `SHA-1`   | deprecated  | `SHA-256` |
| `MD5`     | deprecated  | `SHA-256` |

### Anything else

`pq_status: unknown`. Score is computed assuming worst-case
(`classical`) but `rationale` notes the unknown so the operator can
extend the table.

## Per-asset scoring formula (V1)

Inputs: a list of `Primitive`s. Output: integer 0–100.

```
weights:
    sig:     0.40   # the loudest signal — sigs forge backwards
    kex:     0.30   # ephemeral, but harvest-now-decrypt-later applies
    encrypt: 0.20
    hash:    0.10

per primitive:
    pq         → 1.0
    pq_hybrid  → 0.9
    classical  → 0.3
    deprecated → 0.0
    unknown    → 0.4   # mid-low, encourages investigation

asset_score = round(100 * Σ(weight[role] × status_value[primitive]))
```

Roles missing from the asset (e.g. a Bitcoin address has no `kex`)
contribute nothing to numerator and denominator — the formula
re-normalises across the present roles.

`rationale` is a templated single sentence:

- "All primitives PQ-resistant." → score 100
- "X25519 + ECDSA-P256 are classical-only." → score 40
- "ECDSA-secp256k1 signature is classical; recommend ML-DSA-65." →
  Bitcoin holding score ~20.

## Org-level rollup (V1)

```
org_score = weighted_average(
    asset_scores,
    weight = asset_kind_weights[asset.kind] × asset_count
)
```

`asset_kind_weights`:

| Kind             | Weight | Rationale |
|------------------|--------|-----------|
| `x509_cert`      | 1.0    | A weak cert is a public liability. |
| `tls_session`    | 0.7    | Ephemeral; harvest-now-decrypt-later applies but not all sessions are equally sensitive. |
| `ssh_session`    | 0.7    | Same. |
| `ipsec_sa`       | 0.7    | Same. |
| `blockchain_key` | 1.5    | Pubkey is permanent + on-chain forever. Quantum break = funds gone. |
| `hsm_slot`       | 1.0    | Long-lived, cross-cuts many other assets. |
| `dns_dnssec`     | 0.5    | Cached + well-mitigated by DNS resolver behaviour. |

Weights are mutable (operator-tunable from the dashboard, persisted
in Postgres). Defaults above are the V1 starting point and will be
revisited after pilot deployments.

## What V1 does **not** do

- **Time-decay.** A weak cert observed 3 months ago is treated the
  same as one observed today. Rotation tracking is a V2 feature.
- **Compliance regime overlays.** "FIPS 140-3" or "EU CRA" specific
  scoring lives on top of the base posture; defer to V5.
- **Adversary modelling.** "What if Shor is 5 years away vs. 15 years
  away?" Interesting but speculative; not in V1.

## Reference implementation

Lives in `crates/ree0xq-core/src/rollup.rs` (lands with V1 backlog
issue `#SEZ-4`). Pure function, no I/O, easy to fuzz. CI will fuzz
it on every PR.
