# Sezar — Project Summary

A one-page overview of what Sezar is, what was built, and what
it measured. For the full design see [README.md](README.md) and
[ARCHITECTURE.md](ARCHITECTURE.md); for the detailed work log see
[CHANGELOG.md](CHANGELOG.md).

## The problem

Operators preparing for the post-quantum migration are told to
ask one question of every cryptographic asset: *is it PQ-ready?*
That single bit is not enough. Two assets that both read
"classical" can differ by years in how hard they are to migrate,
and the bit says nothing about the channel their key material
travels over — where Quantum Key Distribution is being deployed
on exactly the high-assurance links that matter most.

## The idea

Sezar treats quantum-risk posture as a **three-axis** problem
instead of a one-axis label. Each asset is graded on:

- **A — Algorithmic resistance:** is the primitive itself
  quantum-resistant?
- **C — Channel protection:** is the key material delivered over
  a quantum-secure channel (QKD / hybrid PSK)?
- **G — Migration agility:** how quickly can the primitive be
  replaced — a config change, a library bump, or a hardware
  refresh?

The three axes fold into a single deadline-adjusted score whose
agility weight shrinks as the operator's migration deadline
approaches.

## What was built

An open reference platform that observes all three axes on
commodity hardware — a single Rust workspace:

- An extended `crypto_inventory_event` schema that every agent
  emits, so one event shape spans surfaces as different as
  eBPF TLS sniffing and Solidity source analysis.
- **Five observation agents** — network (eBPF / libpcap TLS),
  certificate (host scan, Certificate Transparency, Vault PKI),
  blockchain (Bitcoin / Ethereum / QRL), HSM-KMS (PKCS#11,
  AWS KMS, smart cards), and QKD (ETSI GS QKD 014 + a reusable
  Key-Management-Entity emulator).
- A posture rollup library, an axum collector with a REST API
  and Postgres backend, and a React dashboard.
- A V5 crypto-agility / PQ-migration recommendations engine
  (per-asset replacements, an org-level roadmap projector, a
  TLS-stack compatibility matrix, and a regulator-deadline
  tracker).

The V1–V5 milestones are complete; the workspace carries ~190
tests, and the platform ships with systemd units, Docker
Compose, and `.deb`/`.rpm` packaging.

## What it measured

Three reproducible studies, runnable on one Linux host with no
QKD hardware and no commercial scanner:

- **Study 1 (Axis A):** a TLS handshake survey of the
  Tranco-top-1k found **43.8%** of responsive hosts (317 of 724)
  negotiating a hybrid post-quantum key exchange when offered one.
- **Study 2 (Axis C):** a controlled ETSI 014 emulator study
  classified **13 of 13** induced KME / link state transitions
  correctly, including the per-session case that link-level
  telemetry alone cannot resolve.
- **Study 3 (Axis G):** a crypto-agility pilot reached **91%**
  agreement (10 of 11) with hand-graded ground truth on an
  eleven-project subset of a fifty-project corpus.

## Outputs

- **Open source** under the MIT License at
  <https://github.com/e2esolutions-tech/ree0xQ> — schema,
  implementation, ETSI 014 emulator and replay corpus, the
  Semgrep agility rule pack, and the hand-graded ground-truth
  corpus.
- **Reference paper** — *"Three Axes of Quantum Risk: A Unified
  Observability Model for PQC, QKD, and Crypto-Agility"* — under
  submission to a peer-reviewed venue.

## Authors

Aleaddin Özer (Chief System Engineer, E2E Solutions) designed
and built the platform, ran the three studies, and wrote the
paper. Murat Aydos (Hacettepe University) supervised the work as
doctoral advisor.
