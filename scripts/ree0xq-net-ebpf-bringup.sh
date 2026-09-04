#!/usr/bin/env bash
# scripts/ree0xq-net-ebpf-bringup.sh — SEZ-3 reproducer.
#
# Walks an operator through the Phase 2.1 eBPF bring-up on the
# host this script runs on:
#
#   1. Pre-flight checks (kernel version, capabilities,
#      toolchain availability, bpf-linker on $PATH).
#   2. Build the kernel-side `ree0xq-net-ebpf` crate against the
#      nightly toolchain + bpfel-unknown-none target.
#   3. Build the userspace loader (`ree0xq-net` with the
#      `live-interface` feature) in release mode.
#   4. Attach the classifier to the requested interface and
#      tail the resulting NDJSON event stream until Ctrl-C.
#
# The script is intentionally a thin orchestrator over the
# operator-visible build commands — every step prints what it's
# about to run, and fails loudly with a documented remediation
# pointer when a pre-flight check fails. The exhaustive runbook
# (background, troubleshooting, acceptance gating) lives in
# docs/ree0xq-net-ebpf.md.
#
# Usage:
#   scripts/ree0xq-net-ebpf-bringup.sh           # attach to lo, NDJSON to stdout
#   scripts/ree0xq-net-ebpf-bringup.sh eth0      # attach to eth0
#   scripts/ree0xq-net-ebpf-bringup.sh eth0 https://collector.local/v1/events
#
# Environment overrides:
#   REE0XQ_EBPF_SKIP_BUILD=1   # reuse previously built objects
#   REE0XQ_EBPF_PREFLIGHT_ONLY=1
#                             # run checks only; don't build/attach

set -euo pipefail

IFACE="${1:-lo}"
COLLECTOR="${2:-}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# Colour codes are belt-and-braces — survive non-TTY pipes by
# falling back to plain text.
if [[ -t 1 ]]; then
    R=$'\033[31m'; G=$'\033[32m'; Y=$'\033[33m'; B=$'\033[34m'; N=$'\033[0m'
else
    R=""; G=""; Y=""; B=""; N=""
fi
step()  { printf "%s[ebpf-bringup]%s %s\n" "$B" "$N" "$*"; }
ok()    { printf "  %s✓%s %s\n" "$G" "$N" "$*"; }
warn()  { printf "  %s!%s %s\n" "$Y" "$N" "$*"; }
fail()  { printf "  %s✗%s %s\n" "$R" "$N" "$*" >&2; exit 1; }

# ----- Pre-flight ---------------------------------------------

step "pre-flight checks"

# Kernel version — ring buffer needs 5.8+
kver="$(uname -r)"
kmaj=$(echo "$kver" | cut -d. -f1)
kmin=$(echo "$kver" | cut -d. -f2)
if [[ "$kmaj" -lt 5 ]] || [[ "$kmaj" -eq 5 && "$kmin" -lt 8 ]]; then
    fail "kernel $kver — need ≥ 5.8 for BPF_MAP_TYPE_RINGBUF (Phase 2.0 pcap-file still works on older kernels)"
fi
ok "kernel $kver (≥ 5.8)"

# bpf filesystem mounted
if ! mount | grep -q "type bpf"; then
    warn "bpffs not mounted; aya will mount it on demand if it has CAP_SYS_ADMIN. To mount manually:"
    warn "    sudo mount -t bpf bpffs /sys/fs/bpf"
else
    ok "bpffs mounted"
fi

# Rust nightly + bpfel-unknown-none target
if ! rustup show 2>/dev/null | grep -q '^nightly'; then
    fail "rust nightly toolchain not installed (run: rustup toolchain install nightly)"
fi
ok "rust nightly available"

if ! rustup +nightly target list --installed 2>/dev/null | grep -q '^bpfel-unknown-none$'; then
    fail "bpfel-unknown-none target missing (run: rustup target add bpfel-unknown-none --toolchain nightly)"
fi
ok "bpfel-unknown-none target installed"

if ! command -v bpf-linker >/dev/null 2>&1; then
    fail "bpf-linker not on PATH (run: cargo install bpf-linker)"
fi
ok "bpf-linker: $(command -v bpf-linker)"

# Interface exists
if ! ip link show "$IFACE" >/dev/null 2>&1; then
    fail "interface $IFACE not found (run: ip link show)"
fi
ok "interface $IFACE present"

# Capabilities — either we're root or we have CAP_BPF + CAP_NET_ADMIN
if [[ $EUID -ne 0 ]]; then
    # We can only do a best-effort check via capsh, when available.
    if command -v capsh >/dev/null 2>&1; then
        caps="$(capsh --print 2>/dev/null | awk -F'= ' '/Current:/ {print $2}')"
        if [[ "$caps" != *"cap_bpf"* ]] || [[ "$caps" != *"cap_net_admin"* ]]; then
            warn "running as non-root and CAP_BPF / CAP_NET_ADMIN not visible in current set"
            warn "  attach will fail with EPERM; re-run with sudo or configure systemd AmbientCapabilities"
        else
            ok "non-root run with CAP_BPF + CAP_NET_ADMIN"
        fi
    else
        warn "running as non-root and capsh is not installed; attach may fail with EPERM"
    fi
else
    ok "running as root"
fi

if [[ "${REE0XQ_EBPF_PREFLIGHT_ONLY:-0}" -eq 1 ]]; then
    step "preflight-only mode — skipping build + attach"
    exit 0
fi

# ----- Build the eBPF object -----------------------------------

EBPF_OBJ="$ROOT/crates/ree0xq-net-ebpf/target/bpfel-unknown-none/release/ree0xq-net-ebpf"

if [[ "${REE0XQ_EBPF_SKIP_BUILD:-0}" -eq 1 && -f "$EBPF_OBJ" ]]; then
    step "skipping eBPF build (REE0XQ_EBPF_SKIP_BUILD=1); using existing $EBPF_OBJ"
else
    step "building kernel-side ree0xq-net-ebpf (nightly + bpfel-unknown-none)…"
    (
        cd crates/ree0xq-net-ebpf
        # The crate's rust-toolchain.toml pins nightly here.
        cargo build -Z build-std=core --release
    )
    [[ -f "$EBPF_OBJ" ]] || fail "build succeeded but $EBPF_OBJ is missing"
    ok "kernel object at $EBPF_OBJ"
fi

# ----- Build the userspace loader ------------------------------

LOADER="$ROOT/target/release/ree0xq-net"

if [[ "${REE0XQ_EBPF_SKIP_BUILD:-0}" -eq 1 && -x "$LOADER" ]]; then
    step "skipping loader build; using existing $LOADER"
else
    step "building userspace ree0xq-net (release, --features live-interface)…"
    cargo build --release -p ree0xq-net --features live-interface
    ok "loader at $LOADER"
fi

# ----- Attach + tail ------------------------------------------

step "attaching TC classifier to $IFACE"
echo
echo "  -- generate a TLS handshake from another shell to confirm events flow, e.g.:"
echo "       curl -fsS https://cloudflare.com/ -o /dev/null"
echo "  -- Ctrl-C here teardowns cleanly (detach + ring-buffer unmap)."
echo

ARGS=( "$LOADER" "live-ebpf" "--iface" "$IFACE" "--ebpf-object" "$EBPF_OBJ" )
if [[ -n "$COLLECTOR" ]]; then
    ARGS+=( "--collector" "$COLLECTOR" )
fi

if [[ $EUID -ne 0 ]]; then
    exec sudo "${ARGS[@]}"
else
    exec "${ARGS[@]}"
fi
