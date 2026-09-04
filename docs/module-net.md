# `ree0xq-net` — Network Module Design

The first module to ship. Targets V1 (Q3 2026). Scope is narrow on
purpose: TLS only. SSH and IPsec are V1 backlog → V1.5 if we have
time.

## What it observes

A live TLS session on the wire — handshakes, not application bytes.

For each session it emits one [`crypto_inventory_event`](./crypto-event-schema.md):

- `asset.kind = tls_session`
- `asset.identity = sha256(client_random ‖ server_ip:port)`
- `asset.host = server SNI ?? server_ip`
- `primitives` lists the negotiated kex, sig (from the cert chain),
  symmetric, and hash. Server's chosen ciphersuite, not the client's
  offered list.
- `posture` rolled up from those four primitives.

## What it does NOT observe

- Application-layer bytes. The eBPF hooks fire on TLS handshake
  frames only.
- Outbound DNS (Nizam already does that — its DNSSEC observations
  forward into `ree0xq-server` separately).
- Certificate inventory. That's `ree0xq-cert`'s job in V2.

## How it works

1. Per host: a `ree0xq-net` agent binary runs as a systemd service.
   Requires `CAP_BPF` + `CAP_NET_ADMIN`.
2. Loads an eBPF program that hooks `sk_msg` for outgoing TLS
   `ClientHello` and incoming `ServerHello` frames. Filter is on
   destination port `443` (configurable).
3. The eBPF program parses the handshake bytes (TLS-record-layer
   decoder is small enough to live in eBPF — there's prior art in
   Falco / Tetragon / Cilium).
4. Extracts: client_random, ciphersuite list (offered + chosen), SNI,
   server cert chain. Pushes into a per-CPU ring buffer.
5. Userspace half drains the ring, dedupes against a recent-session
   cache (5-min TTL — implemented in
   [`ree0xq_net::dedup::DedupCache`](../crates/ree0xq-net/src/dedup.rs);
   default capacity 65 536 sessions, configurable per agent via
   `--dedup-ttl-secs` and `--dedup-capacity` on the CLI; pass
   `--dedup-ttl-secs 0` for forensic mode where every retransmit
   surfaces), maps cipher names → canonical algorithm names,
   computes posture, emits to `ree0xq-server` over HTTPS.

## eBPF library

Aya. Reasons:
- Pure Rust on both kernel and userspace sides; no LLVM-from-C
  toolchain in the build pipeline.
- Active upstream (Cilium adopted it for some agents).
- Type-checked map shapes between kernel and userspace.

Backup: redbpf. Gone if Aya works (it should).

## Performance budget

Per-session overhead in the kernel: target ≤2 µs (handshake parse
cost). In userspace: ≤500 µs (canonicalisation, posture rollup, HTTP
emission). Numbers measured against a baseline of 10 000 TLS sessions
per second per host.

If we can't hit ≤2 µs in eBPF for full ciphersuite-list parsing, we
fall back to "kernel emits raw bytes; userspace parses." Slower but
keeps the agent shippable. Decision is made when we have measurements,
not now.

## Failure modes + guard rails

- **Kernel doesn't support BTF / CO-RE:** agent refuses to start
  with a clear error. Minimum kernel: 5.10. Document this in the
  agent README.
- **Ring buffer overflow under spike:** drop oldest events,
  increment a Prometheus counter, log a warn. Don't block the kernel
  side.
- **Server unreachable:** agent buffers the last 5 minutes of events
  in memory + on disk (capped at 50 MB), retries with backoff.
  Ring + disk overflow drops oldest first; a Prometheus counter
  surfaces the gap.
- **Algorithm canonicalisation gap:** if we observe a ciphersuite
  the agent doesn't have a canonical name for, emit
  `algorithm: "raw:<hex>"` and a WARN. Better an event with an opaque
  name than no event.

## Test plan

- **Unit tests** for the userspace canonicaliser (a few hundred
  ciphersuite codes → algorithm names).
- **Integration tests** with a Rust TLS client/server pair (rustls)
  generating handshakes; agent attached; assert the resulting events
  match expected primitives.
- **Soak test:** the same scripted CortexDNS load test (dnsperf
  derivative), repurposed to drive 10 k TLS handshakes/sec for 60
  minutes. Watch agent CPU + drop counter.
- **Compatibility matrix:** rustls, OpenSSL 3.x, BoringSSL,
  Java 11/17/21, Go 1.21+. We must produce sane events for all.

## V1 acceptance criteria

`ree0xq-net` ships when:

- One agent on one host can run for 10 minutes against a real
  HTTP/HTTPS workload without dropping events or exceeding 2 % CPU.
- Every emitted event passes JSON-schema validation against
  `ree0xq-core`.
- Posture scores are correct for the canonical hand-picked test
  cases (X25519+ECDSA-P256+AES-GCM = 40, full-Kyber+ML-DSA-65 =
  100).
- Agent enrols with the server via mTLS bootstrap and survives the
  server being down for 5 minutes.

Anything beyond that — multi-host clusters, orchestrator deployment
templates, alerting hooks — is V2 conversation, not V1 acceptance.
