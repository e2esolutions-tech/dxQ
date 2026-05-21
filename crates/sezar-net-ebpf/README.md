# sezar-net-ebpf — Phase 2.1 kernel-side eBPF program

A TC classifier that captures TLS handshake bytes off the wire and
pushes them to a ring buffer for the userspace loader in
[`sezar-net`](../sezar-net) to parse.

This crate is **not part of the parent Cargo workspace** because it
compiles to the BPF target (`bpfel-unknown-none`) on a nightly
toolchain. `cargo build --workspace` from the repo root does not
touch it.

## What it observes

For every packet whose TCP payload starts with a TLS Plaintext
record header (`0x16 0x03 0x0X`) and an inner handshake msg_type
`0x01` (ClientHello) or `0x02` (ServerHello), the program copies up
to 1 KiB of the handshake bytes plus the TCP 4-tuple into a
[`RingBuf`] entry.

## Build

Prerequisites:

```bash
rustup toolchain install nightly
rustup target add bpfel-unknown-none --toolchain nightly
cargo install bpf-linker
```

Build (run from this directory; the `rust-toolchain.toml` here
pins nightly, the `.cargo/config.toml` selects the BPF target):

```bash
cargo build -Z build-std=core --release
```

The resulting object lands at
`../../target/bpfel-unknown-none/release/sezar-net-ebpf`.

## Load and attach

The userspace loader lives behind the `live-interface` feature in
the parent `sezar-net` crate; see
`crates/sezar-net/src/live_iface.rs` and `crates/sezar-net`'s
`README.md` for the runtime side. Briefly:

```bash
# From repo root, after building the eBPF object:
cargo build -p sezar-net --features live-interface
sudo ./target/debug/sezar-net live-iface --interface eth0 \
    --ebpf-object target/bpfel-unknown-none/release/sezar-net-ebpf
```

The loader attaches the program to `eth0`'s TC ingress, consumes
ring-buffer entries, parses each handshake with
`crate::tls::parse_handshake`, and emits one
`crypto_inventory_event` per ClientHello/ServerHello — to stdout as
NDJSON, or POSTed to a downstream collector with `--collector`.

## Why TC ingress

We considered:

- **Socket filters** — per-socket attachment, ergonomically a worse
  fit for long-running passive observability.
- **kprobe `tcp_recvmsg`** — sees post-reassembly buffers; the
  per-packet hook is a better fit for the small ClientHello /
  ServerHello messages.
- **uprobe on `SSL_read`** — OpenSSL-specific, per-process, and
  yields post-decryption bytes (more than we need for posture
  observation).

TC ingress: sees every packet on the interface, parses cheaply, no
per-app instrumentation.

## Capability requirements at run time

`CAP_BPF` + `CAP_NET_ADMIN` (loading the program + attaching to TC),
or simply `CAP_SYS_ADMIN` on older kernels. Modern systemd units
typically grant both via `AmbientCapabilities=`.

## Status

**Phase 2.1, scaffolded + bring-up runbook in place.** The
kernel-side source is complete, the userspace loader
(`crates/sezar-net/src/live_iface.rs`) is wired behind the
`live-interface` feature, and `sezar-net live-ebpf` is the CLI
entry point. End-to-end attach + ring-buffer consumption is
operator-driven: the dev / CI environment doesn't have the
nightly + `bpf-linker` + `CAP_BPF` combination, so SEZ-3's
sustained-load and multi-stack acceptance criteria are
documented as a host-side reproducer instead of a CI test.

For the full bring-up runbook (pre-flight, build, attach,
validation, troubleshooting) see
[`docs/sezar-net-ebpf.md`](../../docs/sezar-net-ebpf.md). For a
one-shot orchestrator that walks the pre-flight + build +
attach sequence, see
[`scripts/sezar-net-ebpf-bringup.sh`](../../scripts/sezar-net-ebpf-bringup.sh).

See [methodology.md](../../docs/paper/methodology.md) for the
deployment-time effort estimate.
