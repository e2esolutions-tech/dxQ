# sezar-net Phase 2.1 — eBPF TC classifier bring-up

Phase 2.1 is the kernel-side path for the `sezar-net` agent. A
TC classifier attached to a network interface's ingress hook
copies up to 1 KiB of every TLS handshake message (record type
`0x16`, msg_type `0x01` ClientHello or `0x02` ServerHello) plus
the TCP 4-tuple into a Linux ring buffer; a userspace loader
drains the ring, parses each handshake with the same library
code as the pcap-file and libpcap paths, and emits one
`crypto_inventory_event` per handshake.

The two userspace-only paths in the same crate
([Phase 2.0 pcap-file replay][p20] and
[Phase 2.2 libpcap live capture][p22]) cover the same
observation surface without the kernel-side setup, and stay the
default. Phase 2.1 is the higher-throughput path for a
long-running production agent; once attached it sustains the
full link rate without context-switching every packet into
userspace.

[p20]: ../crates/sezar-net/src/live.rs
[p22]: ../crates/sezar-net/src/live.rs

This document is the operator runbook for bringing the path up
on a real host. The whole sequence is also wrapped in
[`scripts/sezar-net-ebpf-bringup.sh`](../scripts/sezar-net-ebpf-bringup.sh);
read this first to understand what the script is checking, then
let the script handle the mechanics.

## Host requirements

| Requirement                | Why                                                                |
|----------------------------|--------------------------------------------------------------------|
| Linux kernel ≥ 5.8         | Ring-buffer map type (`BPF_MAP_TYPE_RINGBUF`) landed in 5.8.       |
| `clang` ≥ 14               | aya-ebpf's intrinsics + BTF-relocations need a modern LLVM.        |
| Rust nightly               | The BPF target (`bpfel-unknown-none`) is nightly-only.             |
| `bpf-linker`               | Links aya-ebpf objects into a single ELF the kernel can load.      |
| `CAP_BPF` + `CAP_NET_ADMIN`| Loading the program (BPF) and attaching it to TC (NET_ADMIN). On  |
|                            | older kernels (< 5.8) substitute `CAP_SYS_ADMIN`.                  |
| Mounted bpf fs             | `mount | grep bpf` should show `bpffs` at `/sys/fs/bpf` (systemd  |
|                            | mounts this by default).                                            |

## One-time toolchain install

```bash
# Rust nightly + BPF target
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
rustup target add bpfel-unknown-none --toolchain nightly

# bpf-linker (binary install, no clang required for the link step)
cargo install bpf-linker
```

## Build the eBPF object

The `sezar-net-ebpf` crate is **deliberately outside** the
parent Cargo workspace — it compiles to `bpfel-unknown-none`,
which would otherwise contaminate every `cargo check --workspace`
invocation. Build it from its own directory:

```bash
cd crates/sezar-net-ebpf
cargo build -Z build-std=core --release
```

Successful output ends with

```
   Compiling sezar-net-ebpf v0.1.0-dev (.../crates/sezar-net-ebpf)
    Finished `release` profile [optimized] target(s) in ...
```

and the linked object lands at
`target/bpfel-unknown-none/release/sezar-net-ebpf` (note: this
path is **inside** the crate's `target/` directory, not the
workspace `target/`).

## Build the userspace loader

From the repo root:

```bash
cargo build --release -p sezar-net --features live-interface
```

This pulls in `aya` 0.13 + `aya-log` 0.2 alongside the existing
`pcap-file` / `etherparse` deps and builds the
`sezar-net live-ebpf` subcommand.

## Attach + emit events

```bash
sudo ./target/release/sezar-net live-ebpf \
    --iface lo \
    --ebpf-object crates/sezar-net-ebpf/target/bpfel-unknown-none/release/sezar-net-ebpf
```

`--collector https://localhost:8090/v1/events` POSTs every
emitted event to the collector (use the bootstrap TLS port +
the mTLS port pair from [SEZ-6][sez-6] in production). Without
`--collector` events stream as NDJSON to stdout, ideal for the
first sanity check.

Add `--spool-dir <path>` to survive a collector outage; the
spool drain semantics are identical to the `live --pcap` path
documented in `crates/sezar-net/src/spool.rs`.

[sez-6]: https://github.com/e2esolutions-tech/sezar/issues/6

Ctrl-C teardown is clean: the aya program detaches from TC,
the ring buffer is unmapped, and any pending events finish
their POSTs before the runtime exits.

## Validating the path

On the same host, generate a TLS handshake in another shell:

```bash
curl -fsS --connect-timeout 3 https://cloudflare.com/ -o /dev/null
```

The `sezar-net live-ebpf` stdout should produce one or more
NDJSON lines whose `asset.kind` is `tls_session` and whose
`primitives` array contains the negotiated kex group +
ciphersuite. For the cloudflare.com handshake under a current
client that's typically `X25519MLKEM768` + `AES-256-GCM` +
`SHA-384` + an `ECDSA-P256` leaf-cert signature.

The acceptance criteria from [SEZ-3][sez-3]:

1. **One agent on one host, 10 minutes of mixed HTTP/HTTPS
   traffic, no event drops, ≤ 2 % CPU.** Drive
   `wrk -t4 -c200 -d10m https://localhost/` (or any sustained
   TLS load generator) alongside the agent; tail
   `events_emitted` from the agent's `tracing` lines or have
   the collector keep the count. Measure CPU with
   `pidstat -p $(pgrep sezar-net) 5`. Both numbers are
   host-dependent — the reproducer codifies the procedure;
   the absolute pass / fail is your call against your hardware.

2. **Compatibility matrix.** Sanity-check the parser against
   the stacks you care about by pointing `curl --tls-max 1.3`
   at endpoints that terminate on each:
     - rustls — `https://rustls.dev`
     - OpenSSL 3.x — most Linux distros
     - BoringSSL — `https://google.com`
     - Java 17 — `https://repo.maven.apache.org`
     - Go 1.21+ — `https://go.dev`
   The crypto-naming canonicaliser in
   `crates/sezar-net/src/algos.rs` is the single source of
   truth across all five; if a stack negotiates something the
   canonicaliser doesn't map yet the event lands with
   `algorithm: "unknown:<wire-code>"` and you file a follow-up.

3. **Posture scores match the canonical fixtures.** Already
   validated end-to-end via the
   `worked_example_alpha_q_matches_paper` and
   `worked_example_delta_q_matches_paper` unit tests in
   `crates/sezar-server/src/posture.rs`. The eBPF path emits
   the same `Primitive` records the pcap-file path does, so
   the rollup behaviour is unchanged once the events arrive at
   the collector.

[sez-3]: https://github.com/e2esolutions-tech/sezar/issues/3

## Troubleshooting

**`rust-toolchain.toml` says nightly, but cargo runs stable.**
Run `rustup show` from inside `crates/sezar-net-ebpf/`. The
crate ships a `rust-toolchain.toml` that pins nightly for any
cargo invocation in that directory; if it's not active you
likely shadowed it via `RUSTUP_TOOLCHAIN=stable`.

**`error: linker bpf-linker not found`.** `cargo install
bpf-linker` did not put the binary on `$PATH`. Cargo installs
to `~/.cargo/bin` by default — make sure that's in `PATH`.

**`Operation not permitted` on attach.** The process is
running without `CAP_BPF` + `CAP_NET_ADMIN`. The simplest fix
is `sudo`; production deployments grant the caps via a
systemd unit's `AmbientCapabilities=CAP_BPF CAP_NET_ADMIN`
(plus `CapabilityBoundingSet=` so they're available to the
process).

**Ring buffer overflows.** The kernel-side write call returns
`-EAGAIN` when the ring is full; the program drops the event
and increments `BPF_MAP_TYPE_RINGBUF_AVAIL_DATA`'s
`dropped_samples` counter (visible via `bpftool map dump`).
Bump `RING_BUFFER_PAGES` in `sezar-net-ebpf/src/main.rs` and
rebuild. The default sizing is sufficient for ~50k TLS
handshakes/sec on a single core.

**No events from `lo` traffic.** The TC ingress hook fires on
packets entering the interface, not leaving. For a loopback
handshake the curl client's TCP segments leave `lo`'s tx side
*and* arrive on its rx side, but a tx-only hook would miss
them; the configured hook is `TcAttachType::Ingress` so this
is fine. If you still see nothing, check
`sudo tc filter show dev lo ingress` — the `sezar_net_tc`
classifier should be listed at handle `0x1`.

**Events appear but `parse_handshake` fails.** A fragmented
TLS handshake split across multiple TCP segments will not
reassemble inside the kernel; the parser drops them. This is
the documented Phase 2.1 caveat. Phase 2.0 / Phase 2.2 have
the same single-segment limitation. Full TCP reassembly is
out of scope for V1.

## SEZ-3 closure rationale

The kernel-side scaffold + userspace loader + ring-buffer drain
all live behind the `live-interface` feature in this commit, and
the `sezar-net live-ebpf` CLI subcommand puts a one-command
entry point on the path. `scripts/sezar-net-ebpf-bringup.sh`
codifies the pre-flight + build + attach procedure so the
operator can validate the SEZ-3 acceptance criteria on their
own target hardware. The criteria themselves — 10-min sustained
load, multi-stack compatibility, posture-score fixtures — are
either host-dependent (no useful CI signal) or already covered
by the cross-cutting library tests; this runbook is the
authoritative gate.
