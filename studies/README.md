# Sezar — Empirical Studies

This directory holds the three empirical studies cited by the paper's §5 (magazine) and §8 (extended), with every script, raw capture, analysis notebook, and plot under version control.

## Layout

```
studies/
├── README.md              ← this file
├── study1/                ← Axis A — TLS scan of public hosts
│   ├── hosts.txt          → host list (30 well-known sites)
│   ├── probe.py           → TLS probe (Python ssl + cryptography)
│   ├── analyse.py         → distribution analysis + plots
│   ├── captures/          → raw probe results (scan.json + events.ndjson)
│   └── plots/             → study1-distribution.{png,pdf} + summary JSON
├── study2/                ← Axis C — KME emulator replay scenarios
│   ├── run.sh             → boot emulator + collector + server, run all 5 scenarios
│   ├── analyse.py         → per-scenario timelines + observation-latency stats
│   ├── captures/          → events.json + replay metadata per scenario
│   └── plots/             → r{1..5}-timeline.{png,pdf} + latency-hist + summary
└── study3/                ← Axis G — agility scan of OSS corpus subset
    ├── subset.csv         → 11-project pilot subset (from oss-50-v1)
    ├── run.sh             → clone + scan + agreement TSV
    ├── analyse.py         → confusion matrix + per-category distribution
    ├── sources/           → cloned repos at pinned refs
    ├── results/           → per-project events + agreement.tsv
    └── plots/             → study3-agreement-matrix.{png,pdf} + summary
```

## One-shot reproduction

From the repo root:

```bash
# Build all the binaries once.
cargo build --workspace

# Study 1 — ~60s, 30 public-host probes at 1 Hz over two passes:
#   - classical baseline (Python ssl)
#   - PQ-capable (rustls + rustls-post-quantum)
python3 studies/study1/probe.py
./target/debug/sezar-net pq-probe \
  --hosts studies/study1/hosts.txt \
  --rate-delay-ms 600 \
  > studies/study1/captures/pq-scan.ndjson
python3 studies/study1/analyse.py

# Study 2 — ~4 minutes, 5 scenarios sequentially.
./studies/study2/run.sh
python3 studies/study2/analyse.py

# Study 3 — ~3 minutes, 11 GitHub shallow-clones + scan.
./studies/study3/run.sh
python3 studies/study3/analyse.py
```

Dependencies: `python3` ≥ 3.10 with `matplotlib`, `cryptography`, `pyyaml`, `numpy`; `cargo`; `git`; `jq`; `curl`. Everything else is in the workspace.

## Headline numbers (frozen on 2026-05-13)

### Study 1 — public-web TLS baseline (n = 30)
- 30/30 hosts negotiated TLS 1.3 in both probes
- AEAD: AES-256-GCM/SHA-384 = 20, AES-128-GCM/SHA-256 = 7–9, ChaCha20-Poly1305/SHA-256 = 1–3
- Cert sig: ECDSA = 16–18, RSA-PKCS1 = 12–14, ML-DSA/SLH-DSA = 0
- **17/30 (57%) negotiated `X25519MLKEM768`** when offered (PQ-capable rustls probe)
- PQ adopters: Cloudflare, Google, YouTube, Wikipedia, Twitter, Facebook, Instagram, Reddit, Apple, Python, Rust, Debian, IETF, ETSI, Anthropic, OpenAI, E2E Solutions
- Classical-only holdouts: GitHub, Microsoft, Amazon, LinkedIn, Netflix, Mozilla, Go, Ubuntu, ArchLinux, kernel.org, GNU, NIST, OpenSSL

### Study 2 — KME emulator (n = 5 scenarios, 13 transitions)
- 13/13 induced state changes correctly classified by `link_health`
- Observation latency p50 = 0.71 s (range 0.70–0.71 s) against 1 s poll interval
- R5 bifurcated-SAE confirms per-session `channel_protection` over KME-only telemetry

### Study 3 — OSS agility (n = 11 pilot)
- 10/11 agreement with hand-grade (91%)
- Cohen's κ = 0.62 (substantial agreement, Landis–Koch)
- Single dissent: chrony — bimodal between NTS-configurable and NTP-pinned

## Ethical notes

- **Study 1** probes well-known popular public sites where a single TLS handshake is operationally a non-event. ≤1 connection per host, 1 Hz rate cap, 5 s timeout, certificate validation disabled (we observe the chain, we do not authenticate against it). The host list is intentionally small and well-known; before scaling to Tranco-top-1k, follow §9.2 of the paper.
- **Study 2** runs entirely on loopback against the synthetic-key emulator. No external traffic.
- **Study 3** consumes only OSI-licensed public source. Hand grades and reviewer notes are recorded in `crates/sezar-agility/corpus/oss-50-v1.csv`.

## How to update the corpus / grades

The OSS-50 corpus and v1 grades live at
`crates/sezar-agility/corpus/oss-50-v1.csv`. Add new projects
(or revise grades) there; the per-row `expected_level` is the
hand-graded ground truth that `study3/run.sh` compares the
scanner output against. Bump `corpus_version` in the README
when a grade flips.

## How to extend

- **Scale Study 1 up.** Replace `hosts.txt` with the Tranco-top-1k snapshot and increase the rate cap with care; ethical envelope per §9.2 of the paper.
- **Add a PQ-capable probe variant.** Already shipped — `sezar-net pq-probe` (rustls 0.23 + rustls-post-quantum 0.2). The 30-host sample gives 17/30 (57%) X25519MLKEM768 negotiation; rerun on the Tranco-top-1k for an Internet-wide estimate.
- **Add more replay scenarios.** Drop a YAML file into `crates/sezar-qkd/scenarios-fast/` and the `study2/run.sh` runner will pick it up automatically.
- **Add more rules.** Drop a YAML rule into `crates/sezar-agility/rules/v1/` and re-run `study3/run.sh`. Cohen's κ is recomputed by `analyse.py`.
