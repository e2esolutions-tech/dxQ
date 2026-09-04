# Security Policy

ree0xQ is a crypto-posture observability platform. It reads
cryptographic metadata off the wire and out of inventories;
it does not terminate TLS, hold private keys, or enforce
policy. Even so, a discovery tool that ships parsers for
untrusted network and file input has a real attack surface,
and we want to hear about problems.

## Status

ree0xQ is **pre-alpha**. The V1 surface (the `ree0xq-net`
observer, the `ree0xq-server` collector, and the schema in
`ree0xq-core`) is the part most worth scrutinising. Treat
anything outside V1 as a moving target.

## Reporting a vulnerability

Please report security issues **privately** — do not open a
public GitHub issue for them.

- Preferred: GitHub's private vulnerability reporting on this
  repository (Security tab → "Report a vulnerability").
- Alternatively: e-mail <info@e2esolutions.tech> with
  "SECURITY" in the subject line.

Useful things to include, when you have them:

- The affected crate and code path (e.g. a parser in
  `ree0xq-net` or `ree0xq-cert`).
- A minimal input or reproducer — a pcap, a malformed
  certificate, a crafted event payload.
- What you observed (panic, hang, memory blow-up, incorrect
  classification, information disclosure) and what you
  expected.

We will acknowledge the report, work with you on a fix, and
credit you in the release notes if you would like to be
named. As a pre-alpha project we cannot commit to a fixed
response SLA, but security reports take priority over feature
work.

## Scope

In scope:

- Memory-safety or denial-of-service issues in the parsers
  that handle untrusted input (TLS handshake bytes, pcap
  frames, X.509 certificates, event payloads).
- Authentication or authorisation flaws in `ree0xq-server`
  (the mTLS enrolment path, the admin-token gate).
- Incorrect cryptographic classification that an attacker
  could exploit to hide a weak asset from the posture
  rollup.
- Secrets handling — anything that logs or persists a token,
  key, or credential it should not.

Out of scope:

- The disposable test fixtures and emulators under
  `studies/` and the QKD emulator, which are not meant for
  production exposure.
- Findings that require an already-privileged local attacker
  with no privilege boundary crossed.
- The third-party crates ree0xQ depends on — report those
  upstream (we track advisories via `cargo audit`).

## Disclosure

We favour coordinated disclosure: report privately, give us a
reasonable window to ship a fix, and we will publish the
advisory together. If a report goes unanswered, you are free
to disclose on your own timeline — but we would much rather
hear from you first.
