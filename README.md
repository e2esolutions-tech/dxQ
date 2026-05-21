# Sezar — Crypto-Posture Observability

> **Status:** pre-alpha. Repository scaffolding only. V1 (Network module)
> targeted for Q3 2026.

Sezar maps every cryptographic asset in your environment — TLS handshakes
on the wire, certificates on disk, signing keys in your HSM, **and even
crypto holdings on public blockchains** — to a single posture: how
quantum-resistant is each asset, and where do you have to migrate first?

It is **not** a DNS resolver, certificate authority, or blockchain node.
It is an observability layer that sits next to all of these and answers:

```
> Which of my crypto assets are still ECDSA / RSA-2048 / SHA-1?
> Which have already migrated to Dilithium / SPHINCS+ / hash-based?
> When the NIST 2030 deadline lands, what do I touch first?
```

## Why a separate product?

Sezar started life as backlog items on
[CortexDNS](https://github.com/e2esolutions-tech/cortexdns):

- *PQC algorithm tracking in DNSSEC visibility*
- *Network Crypto Posture roll-up*
- *Crypto-posture telemetry*

But these are not DNS problems. They span TLS termination, X.509 PKI,
SSH, IPsec, hardware tokens, and on-chain assets. Stuffing them into
Nizam (a single-binary DNS filter) would dilute that product. Spinning
them out as **Sezar** keeps both products honest:

- **Nizam** stays a focused DNS resolver + filter. It still does its
  share of crypto observability — DNSSEC validation, PQ-algorithm
  tagging on DNS responses — but only at the DNS layer.
- **Sezar** consumes those DNS-layer observations *plus* eBPF/agent
  feeds from every other crypto-bearing surface, normalises them into
  a single inventory, and answers the migration-readiness question.

## Module layout (planned)

| Module           | Purpose                                          | Data source                  |
|------------------|--------------------------------------------------|------------------------------|
| `sezar-net`      | TLS / SSH / IPsec ciphersuite observation        | eBPF + libpcap               |
| `sezar-cert`     | X.509 inventory, key sizes, signature algos      | CT logs, internal CA, host scan |
| `sezar-chain`    | Public-chain crypto: ECDSA/EdDSA/PQC adoption    | RPC / mempool sniffing       |
| `sezar-id`       | HSM / KMS / smart-card key inventory             | Vendor APIs                  |
| `sezar-core`     | Shared event schema + posture-rollup library     | (n/a — library)              |
| `sezar-server`   | Collector + REST API + dashboard backend         | (n/a — service)              |

V1 ships only `sezar-net` + `sezar-server` + a minimal UI. The rest are
stubs until later milestones.

## Architecture at a glance

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  sezar-net   │  │ sezar-cert   │  │ sezar-chain  │  │  sezar-id    │
│ (eBPF agent) │  │  (scanners)  │  │  (RPC/mp)    │  │ (HSM/KMS)    │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │                 │
       │     normalised  │   crypto_inventory_event v1       │
       │                 ▼                 ▼                 │
       │            ┌─────────────────────────────┐          │
       └───────────►│       sezar-server          │◄─────────┘
                    │  (axum collector + UI API)  │
                    └────────────┬────────────────┘
                                 │
                              ┌──┴──┐
                              │ DB  │  (Postgres for config,
                              │     │   columnar later for events)
                              └─────┘
                                 │
                          ┌──────┴──────┐
                          │  Sezar UI   │  posture dashboard,
                          │  (React)    │  migration roadmap
                          └─────────────┘
```

## Quickstart (single-host Docker)

The shortest path from clone to a live collector backed by
Postgres:

```bash
git clone https://github.com/e2esolutions-tech/sezar
cd sezar
docker compose up -d              # builds sezar-server, starts postgres + collector
curl -fsS http://127.0.0.1:8090/healthz   # → ok
```

The stack brings up two services on a shared network:
`postgres` (durable event store, bound to `127.0.0.1:5433`) and
`sezar-server` (the axum collector, bound to `127.0.0.1:8090`).
sezar-server runs the bundled migrations on first boot and
points itself at the Postgres instance via `SEZAR_DATABASE_URL`.

Override the host-side port if `8090` is already taken on the
machine (a common conflict — `127.0.0.1:8090` ships as the
documented default):

```bash
SEZAR_HOST_PORT=8190 docker compose up -d
```

POST an event, then read it back:

```bash
curl -sS http://127.0.0.1:8090/v1/events \
  -H 'content-type: application/json' \
  -d '{"schema_version":1,"schema_minor":1,"source_module":"smoke",
       "observed_at":"2026-05-20T12:00:00Z",
       "asset":{"kind":"tls_session","identity":"smoke-1"},
       "primitives":[{"role":"kex","algorithm":"X25519MLKEM768","pq_resistant":true}],
       "posture":{"score":0,"rationale":"smoke"}}'
curl -sS http://127.0.0.1:8090/v1/posture
```

For a full V1 release-binary acceptance test that drives the
canonical 5-asset rollup against the new container (or the
release binaries directly), run `./scripts/acceptance.sh`.

## Repository layout

```
sezar/
├── Cargo.toml                # workspace
├── crates/
│   ├── sezar-core/           # event schema + rollup library (V1)
│   ├── sezar-server/         # axum collector + REST API     (V1)
│   ├── sezar-net/            # TLS/SSH eBPF agent            (V1)
│   ├── sezar-cert/           # cert inventory                (V2)
│   ├── sezar-chain/          # blockchain crypto monitor     (V3)
│   └── sezar-id/             # HSM/KMS inventory             (V4)
├── docs/
│   ├── architecture.md
│   ├── crypto-event-schema.md
│   ├── module-net.md
│   ├── module-chain.md
│   └── posture-rollup.md
├── web/                      # React + Vite UI
├── Dockerfile                # multi-stage build, single-host runtime
└── compose.yaml              # docker compose up brings sezar-server live
```

## License

MIT — same as Nizam, encourages downstream adoption + integrations.

## Related projects

- [CortexDNS](https://github.com/e2esolutions-tech/cortexdns) — DNS
  security platform. Will surface its DNSSEC observations through
  Sezar's collector once V1 lands.
- [Nizam](https://github.com/e2esolutions-tech/nizam) — DNS filter
  engine + DNSSEC validator. Same relationship as above.

## Getting involved

Roadmap + V1 backlog tracked in [ROADMAP.md](./ROADMAP.md) and the
GitHub Issues / Milestones for this repo. **No code yet** — the V1
crates contain only stubs. The smallest meaningful contribution today
is reading the architecture doc and filing an issue with feedback on
the event schema.
