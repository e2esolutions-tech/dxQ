# Sezar V1 Punch List

Live engineering punch list for the V1 milestone (Q3 2026 target).
For the higher-level milestone view, see [ROADMAP.md](ROADMAP.md).
Update **before** the implementing PR, not after.

Status legend: `[ ]` not started · `[~]` in progress · `[x]` done ·
`[-]` deferred / out of V1.

## V1 critical path

### `sezar-core` — event schema
- [x] `crypto_inventory_event` schema v1 (commit `affd080`).
- [x] ts-rs + JsonSchema codegen wired into `cargo test --features
      ts-types` (regenerates `bindings/`, `web/src/types/sezar.ts`).
- [x] Schema v1.1 additive fields landed — `channel_protection`
      and `agility` blocks plus the `QkdLink` / `QkdKme` asset
      kinds. Backwards-compatible per the schema's
      `schema_minor` discipline; no major version bump needed.

### `sezar-server` — collector + REST
- [x] `axum` scaffolding + in-memory `DashMap` store, ready for
      a later Postgres swap behind the same `EventStore` surface.
- [x] `POST /v1/events` + `/v1/events/batch` with
      `schema_version` validation (rejects mismatched majors with
      a `422 schema_version_mismatch`).
- [x] `GET /v1/events?limit=N` — most-recent-first event log.
- [x] `GET /v1/inventory` — per-asset latest event plus computed
      $q$, sorted highest-urgency first. (Pagination still
      latent; the in-memory store returns the full list today.)
- [x] `GET /v1/posture` — org-level `org_q` + asset count +
      blocked count.
- [x] `GET /v1/blocked` and `GET /v1/qkd/links` — derived views
      off the same store.
- [x] Unit tests `worked_example_alpha_q_matches_paper` and
      `worked_example_delta_q_matches_paper` in
      `crates/sezar-server/src/posture.rs` — assert the
      implementation reproduces the paper §3.1 numerics within
      ±0.01.
- [x] `crates/sezar-server/tests/http_smoke.rs` —
      in-process integration test that exercises every V1 route
      against the router on an ephemeral port.
- [x] Postgres persistence (SEZ-2). `--database-url` /
      `SEZAR_DATABASE_URL` flips the in-process `EventStore`
      trait object from in-memory DashMap to a sqlx-backed
      `PgEventStore`. Two-table schema (`events` history +
      `assets` per-asset latest) under
      `crates/sezar-server/migrations/0001_init.sql`, run
      automatically on first boot. `docker compose up -d`
      brings up postgres + sezar-server with the collector
      pointing at `postgres://sezar:sezar@postgres:5432/sezar`.
      Three integration tests in
      `crates/sezar-server/tests/pg_smoke.rs` exercise the
      full HTTP loop, post-restart durability, and the
      out-of-order-ingest invariant against a disposable
      `postgres:16-alpine` testcontainer. Live-stack p99
      ingest at concurrency 16: **65 ms** (SEZ-2 budget was
      200 ms).
- [x] mTLS bootstrap (SEZ-6). All four acceptance criteria
      met:
      - Internal CA generated on first boot, persisted to disk
        (`/var/lib/sezar/ca/{crt,key}`, key at mode 0600),
        reloaded on every subsequent start.
      - `POST /v1/admin/bootstrap-tokens` — admin-gated by
        `X-Admin-Token` (configured via `--admin-token` or
        `SEZAR_ADMIN_TOKEN`), returns a single-use UUID token
        bound to a specific `agent_id` with operator-controlled
        TTL (1 h – 30 d, default 24 h).
      - `POST /v1/enrol` — agent redeems token, server mints a
        fresh ECDSA-P256 client cert (CN = agent_id, EKU =
        clientAuth) signed by the CA and returns the cert +
        matching private key + CA cert.
      - TLS termination via `--tls`: mints a CA-signed server
        cert at boot, splits routes across a bootstrap-only
        TLS listener (server cert, no client cert) and an mTLS
        main listener (client cert required, must chain to the
        internal CA). `/v1/events` rejection without a valid
        client cert happens at the TLS layer — handlers never
        see the request.
      - Agent-side disk-backed spool (`crates/sezar-net/src/
        spool.rs`): when `--spool-dir` is set alongside
        `--collector`, POST failures append to an NDJSON
        spool; the spool is drained at the start of every
        run. Two integration tests
        (`crates/sezar-net/tests/spool_smoke.rs`) exercise the
        outage-buffers-then-recovers and
        outage-keeps-rejecting-keeps-spool-full scenarios
        end-to-end. Closes the fourth criterion.
      Postgres encrypted-at-rest for the CA + tokens is the
      only deferred piece, gated on SEZ-2.

### `sezar-net` — network observer (V1 critical path)
- [x] Phase 2.0 pcap-file replay landed (`0d255e9`): `sezar-net
      live --pcap <file>` parses Eth/IP/TCP, emits one event per
      ClientHello/ServerHello. Synthetic-frame integration test in
      `tests/live_pcap.rs` passes.
- [x] Phase 2.2 libpcap live-interface capture: `sezar-net live
      --iface <name>` behind `--features live-pcap`. Frame-handling
      path is shared with Phase 2.0; Ctrl-C drains in-flight
      packets cleanly. Build needs `libpcap-devel` /
      `libpcap-dev`; run needs `CAP_NET_RAW`.
- [x] End-to-end smoke from pcap-file source. New integration test
      `crates/sezar-net/tests/end_to_end_smoke.rs` spins
      `sezar-server` in-process on an ephemeral port, runs
      `observe_pcap` against the synthetic ClientHello fixture, POSTs
      each emitted event to `/v1/events`, then reads it back and
      checks that primitives (`X25519+ML-KEM-768`, `X25519`,
      `ML-DSA-65`, `AES-256-GCM`) survived the round trip.
- [ ] End-to-end smoke against a real handshake on `lo`. The
      libpcap path is in place but needs a host with
      `libpcap-devel` + `CAP_NET_RAW`. Suggested check:
      `sezar-net live --iface lo --filter "tcp port 443"
       --collector http://127.0.0.1:8091/v1/events`
      while a `curl https://...` runs in another terminal.
- [x] Phase 2.1 eBPF — `sezar-net live-ebpf` CLI subcommand
      added behind `--features live-interface`; full operator
      runbook at [`docs/sezar-net-ebpf.md`](docs/sezar-net-ebpf.md)
      covering pre-flight, build, attach, validation procedures
      for each SEZ-3 acceptance criterion, and a troubleshooting
      section. `scripts/sezar-net-ebpf-bringup.sh` is the
      one-command orchestrator (pre-flight checks → BPF object
      build → loader build → attach + tail). The kernel-side
      attach validation is operator-side because the dev / CI
      environment doesn't satisfy nightly + `bpf-linker` +
      `CAP_BPF` simultaneously; the runbook is the
      authoritative gate. SEZ-3 closed on this basis.
- [ ] `sezar-net pq-probe` CLI binary that wraps the Tranco-1k
      probe (`rustls + rustls-post-quantum`) used in Study 1.
      Currently lives under `studies/study1/` — promote into the
      crate as a published subcommand.

### Posture rollup library
- [x] Per-event scoring with FIPS 203/204/205 awareness, ECDSA
      penalty, RSA<2048 fail, X25519MLKEM768 reward — lives in
      `crates/sezar-server/src/posture.rs::q_for_event`.
- [x] Three-axis combination `q(asset, t)` per paper §3 (§5 in
      extended) with operator-tunable α / β / γ weights and the
      deadline-tension τ term.
- [x] `BLOCKED` flag whenever `G ≤ 0.20` (`is_blocked` helper +
      `GET /v1/blocked` view).

### React UI — posture dashboard
- [x] Org-level score chip — Posture page polls `/v1/posture`
      every 10 s and renders `org_q` + asset count + BLOCKED
      count alongside a deadline countdown.
- [x] Asset list with sort by `q`, filter by `BLOCKED` and
      asset kind — Inventory page polls `/v1/inventory` every
      30 s with a manual refresh button.
- [x] Per-asset detail view — row click on the inventory table
      opens a modal panel with the asset's primitives, source
      module, q, observed-at timestamp, and a pointer to
      `/v1/events?limit=N` for the full event JSON.
- [x] Breakdown by asset kind — Posture page also shows mean
      and max q per asset kind with a small horizontal bar
      chart, surfaced under the org-level chip.
- [x] Empty-state CTA — when `/v1/posture` reports zero
      assets, the page swaps to a "no agents reporting yet"
      panel with copy-to-clipboard install commands.
- [ ] Auth: OIDC (Keycloak optional) — scope item, deferred to
      a follow-up issue. The dashboard is unauthenticated
      today, fine for the localhost demo path but a gate for
      any externally-reachable deployment.
- [ ] Last-N-days trend chart — not in V1 acceptance; useful
      addition once the store has time-series data.

### Demo + acceptance test rig
- [x] `scripts/demo.sh` boots emulator + collector + server (per
      paper §5.4 / §8.4).
- [x] In-process end-to-end test
      `crates/sezar-net/tests/end_to_end_smoke.rs` — runs
      `observe_pcap` against the synthetic ClientHello fixture,
      POSTs through the collector, reads back via `/v1/events`,
      `/v1/inventory`, `/v1/posture` and asserts the primitives
      and the positive `org_q`.
- [x] Throughput probe `scripts/loadtest.py` — concurrent POSTs
      against `/v1/events`, reports rate + p50/p90/p99 latency
      and exit codes the failure count. First baseline against
      the in-memory store: ~812 req/s at concurrency 16, p50
      18 ms, p99 51 ms on a single Linux host. (Closes SEZ-8.)
- [x] Shell-level acceptance smoke against the release binaries.
      `scripts/acceptance.sh` builds in release, boots
      `sezar-server` on `127.0.0.1:8190`, seeds the canonical 5
      events (3 via `sezar-net from-zgrab`, 1 via `sezar-net
      live --pcap`, 1 hand-crafted FIPS-locked appliance via
      curl), reads `/v1/posture` + `/v1/inventory` + `/v1/blocked`,
      and asserts `assets == 5`, `blocked_count == 1`, `org_q > 0`,
      and the BLOCKED row points at the appliance. Exits 0 on
      pass, 1 on any assertion failure. CI-ready, runs
      unprivileged on `127.0.0.1`. (Recovers `org_q ≈ 0.613` —
      close to but not identical to §8.4's `0.627`; the gap is
      the missing QKD-protected asset and the τ shift from the
      paper's measurement date.)
- [x] Docker Compose single-host install — multi-stage `Dockerfile`
      compiles `sezar-server` in release and ships a slim Debian
      runtime under a non-root `sezar` user with `tini` as PID 1
      and a `curl /healthz` HEALTHCHECK; `compose.yaml` exposes the
      collector on `127.0.0.1:8090` (overridable via
      `SEZAR_HOST_PORT`). `docker compose up -d` is the V1
      quickstart now documented in `README.md`.

## Paper

- [x] Magazine draft v0.4 — Study 1 on Tranco-top-1k headline.
- [x] Extended draft v0.3 — methodology aligned to actual run,
      pilot vs Tranco-1k table in §8.1.3.
- [ ] Add ORCIDs to the frontmatter once provided by the authors.
- [ ] Decide on submission venue concretely — README says
      IEEE S&P Magazine; lock the formatting once decided.

## Studies

- [x] Study 1 — Axis A on Tranco-top-1k (snapshot 6G8PX,
      2026-05-13). Raw captures + `analyse_tranco.py` under
      `studies/study1/`.
- [ ] Study 1 quarterly re-run — same vantage, same probes, builds
      the X25519MLKEM768 adoption time series.
- [ ] Study 2 — Axis C via the ETSI GS QKD 014 emulator. The runner
      `studies/study2/run.sh` exists; results need committing.
- [ ] Study 3 — Axis G on the OSS-50 corpus. Currently 11-project
      pilot per paper §8.3.3; full corpus is the remaining scale-up.

## Repo hygiene

- [x] NOTES.md project brief committed.
- [x] `.config/settings.json` + `.config/skills/` (paper-build,
      ref-verify, schema-bump-check).
- [x] CONTRIBUTING.md — contribution conventions, conventional
      commits, no-AI-attribution rule, citation-verification
      pointer for paper changes.
- [x] CHANGELOG.md — initial Unreleased entry capturing the V1
      scaffolding state.
- [ ] GitHub Actions: `cargo check` + `cargo test` on PR
      (currently deferred per commit `69ac7e3` — token lacked
      `workflow` scope).

## Out of scope for V1

Per [ROADMAP.md](ROADMAP.md): no SSH/IPsec sniffing, no certificate
scanners, no blockchain monitoring, no HSM adapters, no k8s
deployment, no multi-tenant RBAC, no alert rules. Don't add to V1
without explicitly deferring something else.
