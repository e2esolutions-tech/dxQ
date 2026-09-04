#!/usr/bin/env bash
# scripts/acceptance.sh — V1 release-binary acceptance smoke.
#
# Builds (if needed) and exercises the actual ree0xq-server and
# ree0xq-net release binaries against deterministic test fixtures,
# then asserts the posture rollup is sane:
#
#   - 5 assets ingested (3 from the zgrab fixture, 1 from the
#     synthetic ClientHello pcap, 1 hand-crafted FIPS-locked
#     appliance posted via curl);
#   - exactly 1 BLOCKED asset (the locked appliance);
#   - org_q > 0 (any non-zero rollup is enough — the exact value
#     depends on the run date through the deadline-tension τ
#     term so we do not pin it).
#
# Exits 0 on pass, 1 on any failed assertion or boot error. The
# whole thing runs unprivileged on `127.0.0.1`.
#
# Why not just rely on the in-process integration test?
#   - That test exercises the library surface and JSON shapes.
#   - This script exercises the actual CLIs end-to-end against a
#     real network socket, which is what V1 ships to operators.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

LISTEN_ADDR="${REE0XQ_ACCEPTANCE_LISTEN:-127.0.0.1:8190}"
BASE_URL="http://${LISTEN_ADDR}"
LOG_DIR="${TMPDIR:-/tmp}/ree0xq-acceptance-$$"
mkdir -p "$LOG_DIR"

server_pid=""
cleanup() {
  local rc=$?
  if [[ -n "$server_pid" ]]; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [[ $rc -ne 0 ]]; then
    echo
    echo "[acceptance] FAILED (exit $rc). Server log tail:"
    tail -n 50 "$LOG_DIR/ree0xq-server.log" 2>/dev/null || true
  fi
  rm -rf "$LOG_DIR"
}
trap cleanup EXIT

log() { printf '[acceptance] %s\n' "$*"; }
fail() { printf '[acceptance] FAIL: %s\n' "$*" >&2; exit 1; }

log "building release binaries…"
cargo build --release --quiet -p ree0xq-server -p ree0xq-net

log "starting ree0xq-server on ${LISTEN_ADDR}…"
# Use a per-run CA dir under $LOG_DIR so the script stays
# unprivileged (the default /var/lib/ree0xq/ca is only writable
# inside the production container or with sudo).
./target/release/ree0xq-server \
  --listen "$LISTEN_ADDR" \
  --ca-dir "$LOG_DIR/ca" \
  >"$LOG_DIR/ree0xq-server.log" 2>&1 &
server_pid=$!

# Poll /healthz until the server starts accepting requests.
for i in $(seq 1 50); do
  if curl -sf "${BASE_URL}/healthz" >/dev/null 2>&1; then
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    fail "ree0xq-server died during startup (see log above)"
  fi
  sleep 0.1
done
curl -sf "${BASE_URL}/healthz" >/dev/null || fail "server did not come up within 5s"
log "server up"

# ---- Seed: 3 events from the zgrab fixture ----
log "seeding 3 events from the zgrab fixture…"
./target/release/ree0xq-net from-zgrab \
  --input crates/ree0xq-net/tests/fixtures/zgrab-tls13-pq.json \
  --collector "${BASE_URL}/v1/events" \
  >"$LOG_DIR/ree0xq-net-zgrab.log" 2>&1

# ---- Seed: 1 event from the synthetic ClientHello pcap ----
log "seeding 1 event from the synthetic ClientHello pcap…"
./target/release/ree0xq-net live \
  --pcap crates/ree0xq-net/tests/fixtures/synth-clienthello.pcap \
  --collector "${BASE_URL}/v1/events" \
  >"$LOG_DIR/ree0xq-net-live.log" 2>&1

# ---- Seed: 1 hand-crafted FIPS-locked appliance (BLOCKED candidate) ----
log "seeding 1 synthetic FIPS-locked appliance…"
curl -sS -X POST "${BASE_URL}/v1/events" \
  -H 'content-type: application/json' \
  --data-binary @- <<'JSON' >/dev/null
{
  "schema_version": 1,
  "schema_minor": 1,
  "source_module": "ree0xq-agility",
  "observed_at": "2026-05-20T08:00:00Z",
  "asset": {
    "kind": "tls_session",
    "identity": "fips-locked-appliance",
    "host": "appliance.tek.example"
  },
  "primitives": [
    {"role": "kex",     "algorithm": "X25519",      "pq_resistant": false},
    {"role": "sig",     "algorithm": "ECDSA-P256",  "pq_resistant": false},
    {"role": "encrypt", "algorithm": "AES-256-GCM", "pq_resistant": true},
    {"role": "hash",    "algorithm": "SHA-384",     "pq_resistant": true}
  ],
  "agility": {
    "level": "locked",
    "level_score": 0.20,
    "evidence": [
      {"type": "vendor_declaration",
       "statement": "FIPS 140-3 tested configuration enumerates only classical algorithms."}
    ],
    "scanner_version": "acceptance",
    "rubric_version": "qra-rubric/v1.0"
  },
  "posture": {"score": 0, "rationale": "acceptance fixture"}
}
JSON

# ---- Read back and assert ----
posture_json=$(curl -sS "${BASE_URL}/v1/posture")
inventory_json=$(curl -sS "${BASE_URL}/v1/inventory")
blocked_json=$(curl -sS "${BASE_URL}/v1/blocked")

assets=$(printf '%s' "$posture_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["assets"])')
blocked_count=$(printf '%s' "$posture_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["blocked_count"])')
org_q=$(printf '%s' "$posture_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["org_q"])')
inv_count=$(printf '%s' "$inventory_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["count"])')
blk_count=$(printf '%s' "$blocked_json" | python3 -c 'import json,sys;print(json.load(sys.stdin)["count"])')

log "rollup: assets=${assets} blocked_count=${blocked_count} org_q=${org_q}"
log "inventory rows=${inv_count} | /v1/blocked rows=${blk_count}"

[[ "$assets"        == "5" ]] || fail "expected 5 assets, got ${assets}"
[[ "$inv_count"     == "5" ]] || fail "expected 5 inventory rows, got ${inv_count}"
[[ "$blocked_count" == "1" ]] || fail "expected 1 BLOCKED asset, got ${blocked_count}"
[[ "$blk_count"     == "1" ]] || fail "expected 1 /v1/blocked row, got ${blk_count}"

python3 - <<PY || fail "org_q non-positive: ${org_q}"
import sys
q = ${org_q}
sys.exit(0 if q > 0.0 else 1)
PY

# The BLOCKED row must be the synthetic appliance.
blocked_identity=$(printf '%s' "$blocked_json" \
  | python3 -c 'import json,sys;print(json.load(sys.stdin)["items"][0]["identity"])')
[[ "$blocked_identity" == "fips-locked-appliance" ]] \
  || fail "expected BLOCKED identity 'fips-locked-appliance', got '${blocked_identity}'"

log "PASS — assets=5, blocked_count=1, org_q>0, BLOCKED row pointed at fips-locked-appliance"
