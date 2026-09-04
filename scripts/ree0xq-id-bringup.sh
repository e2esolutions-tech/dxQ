#!/usr/bin/env bash
# scripts/ree0xq-id-bringup.sh — SEZ-18 reproducer.
#
# Pre-flight + scan + tail for the hardware-bound ree0xq-id
# backends (PKCS#11 / YubiHSM / smart card). Mirrors the
# scripts/ree0xq-net-ebpf-bringup.sh shape: the script
# checks every host-side prerequisite, reports missing
# pieces with a remediation pointer, then drives the scan
# if everything's in place.
#
# Usage:
#   scripts/ree0xq-id-bringup.sh <vendor-pkcs11-lib> [<collector-url>]
# Env:
#   REE0XQ_HSM_PIN              # user PIN; required when the
#                              # token requires login
#   REE0XQ_ID_PREFLIGHT_ONLY=1  # check only, no build / scan
#   REE0XQ_ID_SKIP_BUILD=1      # reuse cached release binary

set -euo pipefail

LIBRARY="${1:-}"
COLLECTOR="${2:-}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -t 1 ]]; then
    R=$'\033[31m'; G=$'\033[32m'; Y=$'\033[33m'; B=$'\033[34m'; N=$'\033[0m'
else
    R=""; G=""; Y=""; B=""; N=""
fi
step()  { printf "%s[id-bringup]%s %s\n" "$B" "$N" "$*"; }
ok()    { printf "  %s✓%s %s\n" "$G" "$N" "$*"; }
warn()  { printf "  %s!%s %s\n" "$Y" "$N" "$*"; }
fail()  { printf "  %s✗%s %s\n" "$R" "$N" "$*" >&2; exit 1; }

[[ -z "$LIBRARY" ]] && fail "missing PKCS#11 library path. usage: $0 <vendor-pkcs11-lib> [collector-url]"

# ---- pre-flight ----

step "pre-flight checks"

# Library exists + readable.
if [[ ! -r "$LIBRARY" ]]; then
    fail "PKCS#11 library not readable: $LIBRARY"
fi
ok "library exists: $LIBRARY"

# Library actually a shared object (PE / Mach-O fail loudly).
if file "$LIBRARY" 2>/dev/null | grep -q "shared object"; then
    ok "library is an ELF shared object"
else
    warn "file(1) didn't tag '$LIBRARY' as a shared object; check the architecture matches the binary"
fi

# pkcs11-tool present (for diagnostic listing).
if command -v pkcs11-tool >/dev/null 2>&1; then
    ok "pkcs11-tool: $(command -v pkcs11-tool)"
else
    warn "pkcs11-tool not on PATH — install opensc or the vendor's tooling for diagnostics"
fi

# Token responds to LIST objects.
if command -v pkcs11-tool >/dev/null 2>&1; then
    if pkcs11-tool --module "$LIBRARY" --list-slots 2>/dev/null | grep -q "Slot"; then
        ok "library lists at least one slot"
    else
        warn "pkcs11-tool --list-slots returned nothing; the token may not be initialised"
    fi
fi

# PIN env (when login is required).
if [[ -n "${REE0XQ_HSM_PIN:-}" ]]; then
    ok "REE0XQ_HSM_PIN is set (length=${#REE0XQ_HSM_PIN})"
else
    warn "REE0XQ_HSM_PIN not set; the scan will see only public-only objects"
fi

# Collector reachability — best-effort, no failure when not given.
if [[ -n "$COLLECTOR" ]]; then
    if curl -sf --max-time 3 "${COLLECTOR%/}/healthz" >/dev/null 2>&1; then
        ok "collector reachable: $COLLECTOR"
    else
        # Tolerate non-/healthz collector URLs (e.g.
        # full /v1/events). Just announce.
        warn "collector pre-check failed against ${COLLECTOR%/}/healthz — POSTs will surface the error"
    fi
fi

if [[ "${REE0XQ_ID_PREFLIGHT_ONLY:-0}" -eq 1 ]]; then
    step "preflight-only mode — skipping build + scan"
    exit 0
fi

# ---- build ----

BIN="$ROOT/target/release/ree0xq-id"
if [[ "${REE0XQ_ID_SKIP_BUILD:-0}" -eq 1 && -x "$BIN" ]]; then
    step "skipping build (REE0XQ_ID_SKIP_BUILD=1); using $BIN"
else
    step "building ree0xq-id (release, --features pkcs11)…"
    cargo build --release -p ree0xq-id --features pkcs11
    ok "binary at $BIN"
fi

# ---- scan ----

step "running pkcs11-scan against $LIBRARY"
echo

ARGS=( "$BIN" "pkcs11-scan" "--library" "$LIBRARY" )
[[ -n "${REE0XQ_HSM_PIN:-}" ]] && ARGS+=( "--pin-env" "REE0XQ_HSM_PIN" )
[[ -n "$COLLECTOR" ]] && ARGS+=( "--collector" "$COLLECTOR" )

exec "${ARGS[@]}"
