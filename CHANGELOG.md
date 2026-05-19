# Changelog

All notable changes to this project will be documented in this
file. Format follows the spirit of [Keep a Changelog]; this
project does not follow [Semantic Versioning] yet — V1 is still
under development and the tag scheme will land with the V1 cut.

[Keep a Changelog]: https://keepachangelog.com/en/1.1.0/
[Semantic Versioning]: https://semver.org/spec/v2.0.0.html

## [Unreleased]

Pre-alpha. Tracks toward the V1 cut (Q3 2026) per
[`ROADMAP.md`](ROADMAP.md).

### Added

- **Single-host Docker install.** Multi-stage `Dockerfile` and
  `compose.yaml` bring `sezar-server` up under a non-root user
  with `tini` as PID 1 and a `curl /healthz` HEALTHCHECK. Port is
  env-overridable (`SEZAR_HOST_PORT`); `docker compose up -d` is
  the documented quickstart.
- **Release-binary acceptance smoke.** `scripts/acceptance.sh`
  drives the release CLIs end-to-end against a local TCP socket,
  seeds five deterministic events through `sezar-net from-zgrab`
  + `sezar-net live --pcap` + a hand-crafted `agility: locked`
  curl POST, and asserts `assets == 5`, `blocked_count == 1`,
  `org_q > 0`, and that the BLOCKED row points at the synthetic
  appliance.
- **`sezar-net` Phase 2.2 — libpcap live-interface capture.**
  Behind the `live-pcap` Cargo feature: `sezar-net live --iface
  <name>` opens a network interface via libpcap and feeds the
  same frame-handling code as the pcap-file replay. Ctrl-C
  drains in-flight packets cleanly. Build needs
  `libpcap-devel` / `libpcap-dev`; runtime needs `CAP_NET_RAW`.
- **`sezar-net` end-to-end smoke** (`crates/sezar-net/tests/
  end_to_end_smoke.rs`). Spins `sezar-server`'s router in-process
  on an ephemeral port, drives `observe_pcap` against the
  synthetic ClientHello fixture, POSTs each emitted event to
  `/v1/events`, and asserts the primitives (`X25519+ML-KEM-768`,
  `X25519`, `ML-DSA-65`, `AES-256-GCM`) survive the round trip.
- **Study 1 — Tranco-top-1k scan artefacts.** Captures
  (`scan-tranco-1k.json`, `pq-scan-tranco-1k.ndjson`), the pinned
  Tranco list (`tranco-6G8PX-1k.{csv,txt}`), the regenerable
  plots, and the analyser script `studies/study1/analyse_tranco.py`.
  Reproducibility-friendly: re-running the analyser regenerates
  the figures the paper cites without a fresh scan.
- **Paper drafts** under `docs/paper/`: magazine v0.4 and
  extended v0.3 (2026-05-18). Magazine §5.1 + extended §8.1
  carry the Tranco-1k headline (n = 1,000, 724 responsive, 317
  PQ = 43.8%); the 30-host curated pilot survives as a
  sample-selection-bias contrast.
- **local tooling project setup.** `.config/settings.json`
  permission allowlist plus three skills (`paper-build`,
  `ref-verify`, `schema-bump-check`) that codify the load-bearing
  directives from `NOTES.md`.
- **TODO.md** punch list for the V1 critical path.
- **CONTRIBUTING.md** — contribution conventions including the
  no-AI-attribution rule and the citation-verification
  requirement.

### Changed

- **Paper tone.** Stripped style-tell phrasings (`We argue` openers,
  `Crucially,`, `Side-by-side,` framing, adjective-stacked
  self-description) across both magazine and extended drafts
  without changing technical claims, numbers, or citations.
- **`idq-deployments` citation.** Upgraded from the IACR ePrint
  preprint to the peer-reviewed EPJ Quantum Technology version
  (DOI `10.1140/epjqt/s40507-025-00350-5`); metadata
  cross-checked via the PMC open-access mirror.

### Existing surface (V1 scaffolding inherited from earlier work)

- **`sezar-core`** — `crypto_inventory_event` schema v1.1 with
  `channel_protection` and `agility` blocks plus the
  `QkdLink` / `QkdKme` asset kinds; ts-rs + JsonSchema codegen.
- **`sezar-server`** — `axum` collector backed by an in-memory
  `DashMap` store, the V1 REST surface
  (`/healthz`, `POST /v1/events`, `/v1/events/batch`,
  `GET /v1/events`, `/v1/inventory`, `/v1/posture`,
  `/v1/blocked`, `/v1/qkd/links`), the deadline-adjusted rollup
  library (`q_for_event`, `is_blocked`, `org_score`), and unit
  tests `worked_example_alpha/delta_q_matches_paper` that anchor
  the paper §3.1 numerics.
- **`sezar-net`** — TLS handshake byte parser, IANA-codepoint
  primitive mapping, zgrab2 JSON ingest adapter, Phase 2.0
  pcap-file replay, Phase 2.1 eBPF TC classifier skeleton
  (`live-interface` feature), and the PQ-capable probe used in
  Study 1.
