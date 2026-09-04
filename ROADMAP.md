# ree0xQ Roadmap

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
| 1 | `crypto_inventory_event` schema v1 (`ree0xq-core`) | Stable JSON shape, serde derives, doc comments. |
| 2 | `ree0xq-server` collector (axum + Postgres) | POST `/v1/events`, GET `/v1/inventory`, GET `/v1/posture`. |
| 3 | `ree0xq-net` eBPF agent (TLS only) | Sniff `client_hello` / `server_hello`, emit ciphersuite + cert fingerprint per session. |
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

`ree0xq-cert`. Three data sources:

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

`ree0xq-chain`. Initial chains:

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

`ree0xq-id`. Reads from:

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

V5 is where ree0xQ stops being "another scanner" and starts being a
tool the CISO actually opens before each board meeting.

## Out of scope for the foreseeable future

- **Building our own HSM / CA / blockchain.** Other people make
  these; ree0xQ reads them.
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
