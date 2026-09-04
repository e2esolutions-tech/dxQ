# OSS-50 Crypto-Agility Corpus, v1

Hand-graded ground truth for the paper's Study 3.

Each row in [`oss-50-v1.csv`](./oss-50-v1.csv) names one widely
deployed open-source server project at a pinned upstream reference
(tag or branch). The `expected_level` column is the **reviewer
ground truth**: the agility level the project should be classified as
under the §2.3 rubric (`negotiated` / `configurable` / `pinned` /
`locked` / `frozen`).

## Why these projects

We aimed for:

- **Coverage across categories** — HTTP/reverse-proxy, mail (SMTP +
  IMAP/POP3), databases, message brokers, DNS, file sharing, VPN /
  secure-shell, certificate authority, observability, security/KMS,
  messaging, time, CI/CD.
- **Operationally significant** — all 50 are in widespread enterprise
  or hyperscale production deployment (cross-checked against
  CNCF/Apache/Linux Foundation registries and large vendor SBOMs).
- **Open licenses** — every project is OSI-licensed; the corpus
  contains no proprietary code.

## Schema

```
project,category,repo_url,pinned_ref,expected_level,reviewer_notes
```

| Column            | Meaning                                                                                   |
|-------------------|-------------------------------------------------------------------------------------------|
| `project`         | Short identifier used in dashboards and the paper.                                        |
| `category`        | One of: `http_server`, `mail_server`, `database`, `message_broker`, `vpn_secure_shell`, `dns_server`, `file_server`, `ci_cd`, `observability`, `security_kms`, `certificate_authority`, `messaging`, `time`. |
| `repo_url`        | Canonical upstream URL.                                                                   |
| `pinned_ref`      | Git tag (preferred) or branch the v1 grade applies to.                                    |
| `expected_level`  | Hand-grade per §2.3 rubric. v1 grades are author-supplied; v2 will fold reviewer dissent. |
| `reviewer_notes`  | One-sentence justification, citing the agility surface the rubric matched.                |

## v1 grading methodology

For each project the reviewers:

1. Read the project's configuration documentation for *any* directive
   that selects a TLS protocol, cipher list, signature scheme, key
   exchange group, or pre-shared symmetric algorithm.
2. Cross-referenced against the `ree0xq-agility/rules/v1` rule pack to
   confirm at least one rule would fire on a clean install.
3. Looked at the project's source for hard-coded algorithm names that
   are *not* covered by configuration — these downgrade the level
   under conservative-min aggregation (`pinned` would beat
   `configurable`).
4. Recorded the final level + a one-sentence rationale.

Projects that are intentionally non-agile by protocol design — most
notably **Wireguard** (Curve25519 + ChaCha20-Poly1305 + BLAKE2s
hard-coded by spec) — are graded `pinned` rather than `configurable`,
to reflect that operators cannot swap their algorithms without
switching to a different VPN protocol entirely.

## Disagreement protocol

When two reviewers disagree on a grade, they:

1. Re-read the project's configuration documentation together.
2. Run `ree0xq-agility scan --target <repo> --rules rules/v1` and
   inspect the emitted evidence.
3. Apply conservative-min: the lower level wins (the asset is only as
   agile as its hardest-to-change cryptographic surface).
4. Record the dissenting view in `reviewer_notes` so reviewers in v2
   can revisit.

## How to update

To revise a grade, edit the row directly. Cohen's $\kappa$ between
reviewers is reported in the paper based on the v1 snapshot frozen at
release; subsequent revisions ship as v1.1, v1.2 with a changelog at
the bottom of this README.

## Reproducing the scanner output

For each pinned ref:

```bash
git clone "$REPO_URL" target/
git -C target/ checkout "$PINNED_REF"
ree0xq-agility scan \
  --target target/ \
  --rules ../rules/v1 \
  > corpus/results/$PROJECT.events.json
```

The runner script at `corpus/run.sh` automates the loop over every
row. Outputs land in `corpus/results/`.

## Version history

- **v1** (2026-05-13) — Initial corpus, 50 projects, single-reviewer
  grades from the first author.
