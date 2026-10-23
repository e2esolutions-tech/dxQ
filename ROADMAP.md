# dxQ Roadmap

Five planned milestones. V1 is concrete and budgeted; V2–V5 are
sketches and will be re-scoped as we learn from V1 in production.

The first product hypothesis we're testing is: **"a unified
crypto-inventory event, sourced from heterogeneous modules, is more
valuable than each scanner emitting its own format."** If V1 in
the field validates that, the rest of the milestones become
mechanical. If it doesn't, V2 onward is junked.

## V1 — Network module MVP (shipped; original target Q3 2026)

The smallest thing that proves the unifying-event hypothesis.

| # | Item | Notes |
|---|------|-------|
| 1 | `crypto_inventory_event` schema v1 (`dxq-core`) | Stable JSON shape, serde derives, doc comments. |
| 2 | `dxq-server` collector (axum + Postgres) | POST `/v1/events`, GET `/v1/inventory`, GET `/v1/posture`. |
| 3 | `dxq-net` eBPF agent (TLS only) | Sniff `client_hello` / `server_hello`, emit ciphersuite + cert fingerprint per session. |
| 4 | Posture rollup library | Per-event scoring (FIPS 203/204/205 awareness, ECDSA penalty, RSA<2048 fail). |
| 5 | React UI: posture dashboard | Org-level score, breakdown by asset kind, top weak assets. |
| 6 | mTLS bootstrap (server CA + enrolment token) | Production-grade transport from day one. |
| 7 | Docker compose single-host install | `docker compose up` brings everything live. |
| 8 | Acceptance test rig | Cargo test workspace + scripted dashboard smoke (mirroring CortexDNS's `smoke-v2.7.sh` pattern). |

**Out of scope for V1:** SSH/IPsec sniffing, certificate scanners,
blockchain monitoring, HSM adapters, k8s deployment, multi-tenant
RBAC, alert rules. Don't add anything to this list without explicitly
deferring something else.

## V2 — Certificate inventory (shipped; original target Q4 2026)

`dxq-cert`. Three data sources:

- **CT logs** — pull every cert issued for the customer's domains
  (cribbed from crt.sh / Google's Argon log). Highest signal for
  "what's out there in the world claiming to be us."
- **Internal CA scan** — pull the customer's PKI inventory directly
  (Active Directory CS, HashiCorp Vault PKI, ACME servers).
- **Host scan** — walk well-known cert paths
  (`/etc/ssl`, Java keystores, Windows cert stores) on hosts where
  the agent runs. Closes the "shadow IT cert" gap.

Adds asset kinds: `x509_cert`. Existing dashboard generalises — no
schema bump.

## V3 — Blockchain crypto monitor (shipped; original target Q1 2027)

`dxq-chain`. Initial chains:

- **Bitcoin** (secp256k1 / ECDSA) — the loudest signal, biggest
  customer overlap (custodians, compliance, regulators).
- **Ethereum L1** (secp256k1 ECDSA + emerging EdDSA / SNARK
  primitives in L2 systems).
- **A representative PQ-native chain** (QRL or an Algorand snapshot)
  to prove the schema doesn't fall over on hash-based signatures.

The interesting product question here: which dimension of "Bitcoin
holding" do we treat as the *asset*? Per-address? Per-UTXO? Per-key?
V3 spec doc will pick one and justify; the choice cascades through
the posture rollup math.

Adds asset kind: `blockchain_key`.

## V4 — Identity / HSM module (shipped; original target Q2 2027)

`dxq-id`. Reads from:

- PKCS#11 (any HSM)
- AWS KMS / GCP KMS / Azure Key Vault
- YubiHSM 2 (its own pseudo-PKCS#11 path)
- Smart-card readers (PIV, OpenPGP cards) — opt-in.

Closes the loop on "what keys exist that aren't observable on the
wire" — the cold-storage and root-of-trust corner of the inventory.

Adds asset kind: `hsm_slot`.

## V5 — PQ-migration recommendations engine (shipped; original target Q3 2027)

By V4 we have the inventory. V5 turns it into action:

- **Per-asset replacement plan** — given an `x509_cert` signed with
  RSA-2048, what's the recommended replacement, and what does it
  break (client compat, performance, certificate chain depth)?
- **Org-level migration roadmap** — Gantt-style view: "if you migrate
  these 12 services in Q1 of next year, your posture score moves
  from 47 → 71".
- **Compatibility matrix** — known-good Dilithium / SPHINCS+ /
  Kyber TLS stacks (BoringSSL, OpenSSL, BouncyCastle). Lets the user
  filter recommendations by their actual stack.
- **NIST / regulator deadline tracking** — surface upcoming PQ
  mandate dates per jurisdiction; prioritise migrations against them.

V5 is where dxQ stops being "another scanner" and starts being a
tool the CISO actually opens before each board meeting.

## V6 — Adoption measurement + trend (DONE; 2026-10)

The research-driven follow-up to V1-V5: turn the one-shot Study 1
snapshot into a continuous adoption observatory, and extend the
network agent beyond TLS. All four bites shipped; schema is now v1.2.

- **SEZ-30 — SSH PQ-KEX observation** (`dxq-net ssh`) — DONE
  (28d725d + 2b936b9): parses the SSH2 banner + `SSH_MSG_KEXINIT`
  from pcap/live captures, extracts the negotiated / offered kex
  algorithms, flags the OpenSSH hybrid PQ suites
  (`sntrup761x25519-sha512` L5, `mlkem768x25519-sha256` L3), emits
  `crypto_inventory_event`. Schema bumped to v1.2: new asset kind
  `ssh_session` (minor bump, all consumers audited).
- **SEZ-31 — Longitudinal adoption rig** (`studies/study4`) — DONE
  (a544653): re-runnable Tranco scan reusing the Study 1 probe pair,
  diffed against the frozen 6G8PX baseline; quarterly cadence; emits
  a time-series CSV + adoption-curve plot. Powers the planned
  adoption-curve paper.
- **SEZ-32 — Posture history + trend surface** — DONE
  (0b223ba + 40f36b9): `org_q` snapshots persisted server-side
  (Postgres-durable + in-memory ring), `GET /v1/posture/history`,
  and an inline-SVG trend chart on the dashboard.
- **SEZ-33 — BLOCKED-asset alerting** — DONE (d6389c3): operator
  webhook fired on the transition into the BLOCKED set, with
  fire-and-forget delivery + backoff retry.

Out of V6: Kubernetes/cert-manager inventory, DNSSEC feed, CBOM
export — candidates for V7 once V6 data starts accumulating.

**Clients.** Alongside the React web dashboard, `dxq-desktop`
(SEZ-34) is a native, pure-Rust egui monitor over the read-only REST
API — a standalone crate (not a workspace member; needs GUI libraries
to link/run). mTLS client-cert collectors are a documented follow-up.

## Out of scope for the foreseeable future

- **Building our own HSM / CA / blockchain.** Other people make
  these; dxQ reads them.
- **Real-time enforcement** (blocking weak ciphers, refusing to sign
  with deprecated algos). That belongs in the firewall / PKI / chain
  validator, not in an observability product.
- **Vulnerability scanning beyond crypto.** No port scan, no CVE
  database, no malware. There are excellent dedicated tools for
  those; competing with them adds nothing.
- **AI-driven anomaly detection.** Tempting but not differentiated;
  defer until V5 and only if a customer specifically asks.

## How this list will evolve

V1 ships, we get one or two pilot customers, and we discover that:

- one of the V1 items is actually two items in disguise, or
- two of them collapse into one once the schema is real, or
- the dashboard needs something we didn't list, or
- a customer's specific compliance regime forces a feature off the
  "out of scope" list.

Whatever the surprise is, the rule is: **update this file before the
implementation PR**, not after. Roadmap drift is fine; roadmap
stealth-drift is not.
