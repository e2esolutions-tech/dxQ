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
- [ ] Extend schema with three-axis fields (`channel_protection`,
      `agility`) per the paper §6. Counts as a `schema_version` bump
      — run `.config/skills/schema-bump-check` first.

### `sezar-server` — collector + REST
- [~] `axum` + Postgres scaffolding present; needs:
  - [ ] `POST /v1/events` with JSON validation + dedup key
  - [ ] `GET /v1/inventory` with pagination
  - [ ] `GET /v1/posture` returning `org_q` + asset breakdown
  - [ ] Unit tests `worked_example_alpha_q_matches_paper` and
        `worked_example_delta_q_matches_paper` (paper §3.1 anchors)
- [ ] mTLS bootstrap (server CA + enrolment token) before any
      external integration.

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
- [~] Phase 2.1 eBPF TC classifier skeleton landed (`0d255e9`)
      behind `--features live-interface`. Userspace loader wired
      up; the kernel-side build dance (`bpf-linker` + nightly +
      `bpfel-unknown-none`) and an end-to-end attach-and-consume
      test against a real interface are still open.
- [ ] `sezar-net pq-probe` CLI binary that wraps the Tranco-1k
      probe (`rustls + rustls-post-quantum`) used in Study 1.
      Currently lives under `studies/study1/` — promote into the
      crate as a published subcommand.

### Posture rollup library
- [ ] Per-event scoring with FIPS 203/204/205 awareness, ECDSA
      penalty, RSA<2048 fail, X25519MLKEM768 reward.
- [ ] Three-axis combination `q(asset, t)` per paper §3 (§5 in
      extended), with operator-tunable weights.
- [ ] `BLOCKED` flag whenever `G ≤ 0.20` (locked / frozen).

### React UI — posture dashboard
- [ ] Org-level score chip + last-N-days trend.
- [ ] Asset list with sort by `q`, filter by `BLOCKED`, drill into
      `agility_evidence`.
- [ ] Per-asset detail view: A, C, G axes broken out, evidence
      links into source-module captures.

### Demo + acceptance test rig
- [x] `scripts/demo.sh` boots emulator + collector + server (per
      paper §5.4 / §8.4).
- [ ] Acceptance smoke script: one command runs the whole chain,
      asserts `org_q` is within ±0.01 of the worked-example anchor.
- [ ] Docker Compose single-host install — `docker compose up`
      brings everything live (V1 item 7 in ROADMAP).

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
- [ ] CONTRIBUTING.md — set the bar for contribution before any
      external collaborator lands.
- [ ] CHANGELOG.md — start after the first tagged release.
- [ ] GitHub Actions: `cargo check` + `cargo test` on PR
      (currently deferred per commit `69ac7e3` — token lacked
      `workflow` scope).

## Out of scope for V1

Per [ROADMAP.md](ROADMAP.md): no SSH/IPsec sniffing, no certificate
scanners, no blockchain monitoring, no HSM adapters, no k8s
deployment, no multi-tenant RBAC, no alert rules. Don't add to V1
without explicitly deferring something else.
