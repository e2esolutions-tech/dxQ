# ree0xQ Architecture

This is the design we're committing to **before V1 code lands**, so
contributors can argue with the shape on paper instead of through
half-built pull requests. Treat the rules as load-bearing — every line
should hold up under "why this and not the cheaper alternative?"

## 1. Goals + non-goals

### Goals

- **One inventory of every crypto asset** in an organisation's
  environment, normalised regardless of the source (TLS handshake,
  X.509 cert, on-chain key, HSM slot).
- **Quantum-readiness scoring** per asset and rolled up to the org.
- **Migration-path guidance** — which assets are at highest risk first,
  what the recommended replacement primitive is, and (where possible)
  how to apply the change.
- **Module-pluggable** — operators turn modules on independently. A
  customer that doesn't custody crypto skips `ree0xq-chain` and pays
  zero for it.

### Non-goals

- **Not a CA or HSM.** ree0xQ reads, does not issue or store keys.
- **Not a SIEM.** Crypto-inventory events are emitted at slow cadence
  (seconds–minutes), not flow-rate.
- **Not a real-time enforcement layer.** ree0xQ can flag a weak
  ciphersuite; blocking it is the network team's job (firewall, MTLS
  policy, etc.).
- **Not a replacement for vendor scanners** (Qualys, Rapid7). Those are
  general-purpose vuln scanners; ree0xQ is a focused crypto inventory
  with PQ-readiness as a first-class dimension.

## 2. The unifying primitive: `crypto_inventory_event`

Every module emits a stream of these events. The schema is owned by
`ree0xq-core` and versioned (`schema_version: u32`). All downstream
storage, dashboards, and rollup logic are written against this single
shape.

See [`docs/crypto-event-schema.md`](./docs/crypto-event-schema.md) for
the field-by-field definition. In broad strokes:

```
crypto_inventory_event {
    schema_version, source_module, observed_at,
    asset {
        kind:    "tls_session" | "x509_cert" | "ssh_session" |
                 "ipsec_sa"    | "blockchain_key" | "hsm_slot" | "dns_dnssec",
        identity: <module-specific identifier>,
        host:    "<network or owner context>",
    },
    primitives [
        { role: "kex" | "sig" | "auth" | "encrypt" | "hash",
          algorithm: "ECDSA-P256" | "Dilithium2" | "SHA-256" | ...,
          parameters: { curve, key_bits, hash, ... },
          pq_resistant: bool,            // null = unknown
          nist_classification: "L1" | "L3" | "L5" | null,
        }
    ],
    posture {
        score: 0..100,
        rationale: "human-readable string",
        recommended_replacement: "Dilithium2" | null,
    }
}
```

The same struct serialises out of every module — `ree0xq-net` for a TLS
handshake it just sniffed, `ree0xq-chain` for a Bitcoin signature it
just observed in the mempool, `ree0xq-cert` for a cert it pulled from a
CT log. The dashboard never has to know which module it came from to
render it.

## 3. Component map

```
┌────────────────────── data-collection plane ────────────────────────┐
│                                                                     │
│  ree0xq-net  ── eBPF/aya agent on each host. Sniffs TLS/SSH/IPsec.   │
│  ree0xq-cert ── periodic scanners (CT, internal CA, host filesystem). │
│  ree0xq-chain── opt-in chain watcher (RPC + mempool subscriber).     │
│  ree0xq-id   ── pluggable HSM/KMS adapters (PKCS#11, AWS KMS, etc.). │
│                                                                     │
│  All modules emit JSON events over HTTP/POST or QUIC to:            │
│                                                                     │
└──────────────┬──────────────────────────────────────────────────────┘
               │ (ree0xq-core::crypto_inventory_event)
               ▼
┌────────────────────── control + storage plane ──────────────────────┐
│                                                                     │
│  ree0xq-server (axum) ──┬─►  Postgres (config, posture rules,        │
│                        │     module registrations, alert rules)     │
│                        │                                            │
│                        └─►  events store (V1: same Postgres;        │
│                              V3+: ClickHouse / Parquet for scale)   │
│                                                                     │
└──────────────┬──────────────────────────────────────────────────────┘
               │ REST + SSE
               ▼
┌────────────────────── presentation plane ───────────────────────────┐
│                                                                     │
│  ree0xQ UI (React + Vite)                                            │
│    • Posture dashboard (org-level rollup)                           │
│    • Asset inventory (filterable, by module)                        │
│    • Migration roadmap (assets sorted by risk × effort)             │
│    • Module status (which agents are reporting, last-seen, etc.)    │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

## 4. Why Rust everywhere?

- **eBPF agents need it.** `aya` (or `redbpf`) gives Rust eBPF programs
  with no separate C toolchain. Going Go would force a second
  language; going Python is a non-starter for a single-binary agent.
- **Single workspace** = `cargo test --workspace` covers every module
  and the server. One CI matrix, one release process, one set of
  dependency upgrades.
- **Existing in-house expertise.** Nizam is Rust. Operators already run
  Rust binaries in production from us; ree0xQ fits the same operational
  model.

The UI is React + Vite (not Leptos) because the dashboard is
analytics-heavy (charts, filters, drill-downs). React's ecosystem for
this is unmatched, and we already maintain a React codebase in
`cortexdns-ui`. Reusing components and conventions is worth the
language-boundary tax.

## 5. Storage choices (V1 → V5)

- **V1 (Network module only):** all events land in Postgres alongside
  config. JSONB column for the event body, indexed on
  `(asset.kind, asset.identity, observed_at)`. Operators are expected
  to retain ~7 days of events; volume on a 1000-host install is in
  the low-MB/day range.
- **V2 / V3 (Cert + Chain):** Postgres still works for inventory but
  begins to hurt for histogram queries over events. Add ClickHouse as
  an optional sink behind a feature flag; Postgres continues to hold
  the *current* posture per asset.
- **V4+:** decision point. If customers run multi-million-asset
  inventories (large enterprise + crypto custodian), promote
  ClickHouse to default and Postgres to config-only.

We do **not** introduce ClickHouse in V1 — it's the same trap
CortexDNS fell into with cerebrum (memory: profile split deleted it).
Defer until the Postgres pain is real and measured.

## 6. Module isolation

Each module is a separate crate in the workspace. Inter-module
communication is **only** through `ree0xq-server` over the event
schema. No module imports another. This means:

- Building `ree0xq-net` doesn't pull `ree0xq-chain`'s blockchain SDK
  dependencies.
- A bug in `ree0xq-chain`'s mempool subscriber can't take down the
  TLS observation path.
- Customers turn modules on/off via Docker compose profiles or by
  deploying only the agent binaries they need. No module is mandatory
  except `ree0xq-server` + `ree0xq-core`.

## 7. Deployment shape

- **Single-host install (lab / demo):** `docker compose up` brings up
  `ree0xq-server` + Postgres + the `ree0xq-net` agent on the same host.
- **Multi-host install (prod):** `ree0xq-server` + Postgres on one
  control node; `ree0xq-net` (or other module agents) deployed on every
  observation host as a separate service.
- **Kubernetes:** out of scope for V1. Helm chart targeted for V3 once
  the agent surface stabilises.
- **Air-gapped:** the existing CortexDNS offline installer pattern
  applies — bundle images, push to private registry, document.

## 8. Security model (initial cut)

- All inter-component links use mTLS. The `ree0xq-server` boots a CA
  on first run; agents enrol with a one-time bootstrap token, then
  each session is a unique cert.
- Events carry no key material. Only metadata (algorithm names, key
  sizes, fingerprints, certificate hashes). ree0xQ never sees the
  private key.
- The dashboard is gated by Keycloak (when deployed alongside
  CortexDNS) or by a built-in OIDC/SAML adapter for standalone deploys.
- RBAC at the asset-kind level — a CISO sees everything, a SOC analyst
  sees `tls_session` + `ssh_session`, a crypto custodian sees
  `blockchain_key`. Configurable.

## 9. Versioning + compat

- Event schema: SemVer-versioned. Major bumps are gated on a
  multi-version deprecation window so older agents keep reporting.
- Server / agent: independently released. The server must be at least
  one minor version ahead of any connected agent.
- DB migrations: Alembic-style additive; no destructive changes
  without a manual migration script + an op runbook.

## 10. What's deliberately not decided yet

- The exact wire format (JSON vs. CBOR vs. Protobuf). V1 ships JSON
  for human-friendliness; we'll re-evaluate after the V2 cert volume
  hits us.
- Whether `ree0xq-chain` runs full nodes or relies on third-party RPC.
  Tradeoff: data freshness + completeness vs. operational cost.
  Decision deferred to the V3 design doc.
- Pricing tiers. ree0xQ is MIT-licensed; commercial support / managed
  hosting are open questions for the business side, not the
  architecture.

If any of the above turns out to be load-bearing while writing V1
code, the answer is to update this doc *before* writing the code, not
to bury the decision in an implementation PR.
