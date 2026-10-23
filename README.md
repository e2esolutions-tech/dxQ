# dxQ — Crypto-Posture Observability

**The dimension of quantum readiness.**

> *Previously named **Sezar**; the published preprint and the
> archived study artifacts under `studies/` retain that name. Old
> repository URLs redirect.*

> **Status:** working reference implementation. The V1–V5 milestones
> (network, QKD, agility, certificate, blockchain and HSM/KMS agents,
> collector, dashboard, recommendations engine) are complete, with
> ~190 tests, Docker Compose and systemd deployment, and `.deb`/`.rpm`
> packaging. QKD observation runs against an ETSI GS QKD 014 emulator;
> the eBPF live path and hardware HSM backends are validated
> operator-side (see the per-module runbooks under `docs/`).

dxQ maps every cryptographic asset in your environment — TLS handshakes
on the wire, certificates on disk, signing keys in your HSM, **and even
crypto holdings on public blockchains** — to a single posture: how
quantum-resistant is each asset, and where do you have to migrate first?

For a one-page overview of what was built and measured, see
[SUMMARY.md](SUMMARY.md).

It is **not** a DNS resolver, certificate authority, or blockchain node.
It is an observability layer that sits next to all of these and answers:

```
> Which of my crypto assets are still ECDSA / RSA-2048 / SHA-1?
> Which have already migrated to Dilithium / SPHINCS+ / hash-based?
> When the NIST 2030 deadline lands, what do I touch first?
```

## Why a separate product?

dxQ started life as backlog items on
[CortexDNS](https://github.com/e2esolutions-tech/cortexdns):

- *PQC algorithm tracking in DNSSEC visibility*
- *Network Crypto Posture roll-up*
- *Crypto-posture telemetry*

But these are not DNS problems. They span TLS termination, X.509 PKI,
SSH, IPsec, hardware tokens, and on-chain assets. Stuffing them into
Nizam (a single-binary DNS filter) would dilute that product. Spinning
them out as **dxQ** keeps both products honest:

- **Nizam** stays a focused DNS resolver + filter. It still does its
  share of crypto observability — DNSSEC validation, PQ-algorithm
  tagging on DNS responses — but only at the DNS layer.
- **dxQ** consumes those DNS-layer observations *plus* eBPF/agent
  feeds from every other crypto-bearing surface, normalises them into
  a single inventory, and answers the migration-readiness question.

## Module layout

| Module           | Purpose                                          | Data source                  |
|------------------|--------------------------------------------------|------------------------------|
| `dxq-net`      | TLS / SSH / IPsec ciphersuite observation        | eBPF + libpcap               |
| `dxq-cert`     | X.509 inventory, key sizes, signature algos      | CT logs, internal CA, host scan |
| `dxq-chain`    | Public-chain crypto: ECDSA/EdDSA/PQC adoption    | RPC / mempool sniffing       |
| `dxq-id`       | HSM / KMS / smart-card key inventory             | Vendor APIs                  |
| `dxq-qkd`      | QKD link/KME state observation                   | ETSI GS QKD 014 REST         |
| `dxq-agility`  | Crypto-agility scan + PQ-migration advice        | Semgrep over source/pkg      |
| `dxq-core`     | Shared event schema + posture-rollup library     | (n/a — library)              |
| `dxq-server`   | Collector + REST API + dashboard backend         | (n/a — service)              |
| `dxq-desktop`  | Native (egui) desktop monitor over the REST API  | (n/a — client)               |

All of the above have shipped across the V1-V5 milestones; see
[ROADMAP.md](./ROADMAP.md) for what landed in each.

## Architecture at a glance

```
┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│  dxq-net   │  │ dxq-cert   │  │ dxq-chain  │  │  dxq-id    │
│ (eBPF agent) │  │  (scanners)  │  │  (RPC/mp)    │  │ (HSM/KMS)    │
└──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘
       │                 │                 │                 │
       │     normalised  │   crypto_inventory_event v1       │
       │                 ▼                 ▼                 │
       │            ┌─────────────────────────────┐          │
       └───────────►│       dxq-server          │◄─────────┘
                    │  (axum collector + UI API)  │
                    └────────────┬────────────────┘
                                 │
                              ┌──┴──┐
                              │ DB  │  (Postgres for config,
                              │     │   columnar later for events)
                              └─────┘
                                 │
                          ┌──────┴──────┐
                          │  dxQ UI   │  posture dashboard,
                          │  (React)    │  migration roadmap
                          └─────────────┘
```

## Quickstart (single-host Docker)

The shortest path from clone to a live collector backed by
Postgres:

```bash
git clone https://github.com/e2esolutions-tech/dxQ
cd dxQ
docker compose up -d              # builds dxq-server, starts postgres + collector
curl -fsS http://127.0.0.1:8090/healthz   # → ok
```

The stack brings up two services on a shared network:
`postgres` (durable event store, bound to `127.0.0.1:5433`) and
`dxq-server` (the axum collector, bound to `127.0.0.1:8090`).
dxq-server runs the bundled migrations on first boot and
points itself at the Postgres instance via `DXQ_DATABASE_URL`.

Override the host-side port if `8090` is already taken on the
machine (a common conflict — `127.0.0.1:8090` ships as the
documented default):

```bash
DXQ_HOST_PORT=8190 docker compose up -d
```

POST an event, then read it back:

```bash
curl -sS http://127.0.0.1:8090/v1/events \
  -H 'content-type: application/json' \
  -d '{"schema_version":1,"schema_minor":2,"source_module":"smoke",
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
dxQ/
├── Cargo.toml                # workspace
├── crates/
│   ├── dxq-core/           # event schema + rollup library (V1)
│   ├── dxq-server/         # axum collector + REST API     (V1)
│   ├── dxq-net/            # TLS/SSH eBPF agent            (V1)
│   ├── dxq-cert/           # cert inventory                (V2)
│   ├── dxq-chain/          # blockchain crypto monitor     (V3)
│   └── dxq-id/             # HSM/KMS inventory             (V4)
├── docs/
│   ├── architecture.md
│   ├── crypto-event-schema.md
│   ├── module-net.md
│   ├── module-chain.md
│   └── posture-rollup.md
├── web/                      # React + Vite UI
├── Dockerfile                # multi-stage build, single-host runtime
└── compose.yaml              # docker compose up brings dxq-server live
```

## License

Released under the MIT License — see [`LICENSE`](LICENSE).
Copyright © 2026 E2E Solutions. The permissive licence is
deliberate: it encourages downstream adoption and
integrations, the same choice we made for Nizam.

## Security

Found a vulnerability? See [`SECURITY.md`](SECURITY.md) for how
to report it privately. Please don't open a public issue for
security problems.

## Related projects

- [CortexDNS](https://github.com/e2esolutions-tech/cortexdns) — DNS
  security platform. Will surface its DNSSEC observations through
  dxQ's collector once V1 lands.
- [Nizam](https://github.com/e2esolutions-tech/nizam) — DNS filter
  engine + DNSSEC validator. Same relationship as above.

## Getting involved

The roadmap lives in [ROADMAP.md](./ROADMAP.md) and work is tracked
in this repo's GitHub Issues / Milestones. See
[CONTRIBUTING.md](./CONTRIBUTING.md) for what helps most right now:
bug reports against current behaviour, hardening patches, and
real-world validation runs (live eBPF hosts, hardware HSMs, actual
QKD links).
