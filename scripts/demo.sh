#!/usr/bin/env bash
# scripts/demo.sh — boot every piece of dxQ locally and seed enough
# events into the collector to make the dashboard interesting.
#
# What this does:
#   1. cargo build --workspace (debug) so every binary is on disk.
#   2. Start a KME emulator on :11071 with the steady-state scenario
#      and dxQ's `dxq-qkd` collector pointed at it, forwarding
#      events to `dxq-server` on :8090.
#   3. Start `dxq-server` on :8090.
#   4. Run `dxq-net from-zgrab` over the bundled fixture, forwarding
#      the resulting events to the same collector — gives the
#      dashboard a small but realistic asset list.
#   5. Print the dashboard URLs (assumes `npm run dev` is running
#      separately under web/).
#
# Stop with Ctrl-C; the trap cleans up all child processes.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "[demo] building workspace (debug)..."
cargo build --workspace --quiet

pids=()
cleanup() {
  echo
  echo "[demo] shutting down (${#pids[@]} child pids)..."
  for p in "${pids[@]}"; do
    kill "$p" 2>/dev/null || true
  done
  wait 2>/dev/null || true
}
trap cleanup INT TERM EXIT

echo "[demo] starting KME emulator on :11071 (QBER 1.8%, 12 kbps)..."
./target/debug/dxq-qkd-kme-emulator \
  --listen 127.0.0.1:11071 \
  --kme-id KME-A \
  --paired-kme KME-B \
  --qber 0.018 \
  --key-rate-bps 12000 \
  >/tmp/dxq-kme.log 2>&1 &
pids+=("$!")
sleep 0.3

echo "[demo] starting dxq-server on :8090..."
# Demo runs unprivileged out of the user's $HOME; point the CA
# at /tmp so first-boot generation is writable without sudo.
./target/debug/dxq-server \
  --listen 127.0.0.1:8090 \
  --deadline 2030-01-01T00:00:00Z \
  --horizon-years 5 \
  --ca-dir /tmp/dxq-demo-ca \
  >/tmp/dxq-server.log 2>&1 &
pids+=("$!")
sleep 0.4

echo "[demo] seeding KME observations via dxq-qkd collector..."
./target/debug/dxq-qkd \
  --kme http://127.0.0.1:11071/api/v1 \
  --collector http://127.0.0.1:8090/v1/events \
  --status-poll-interval 3 \
  >/tmp/dxq-qkd.log 2>&1 &
pids+=("$!")
sleep 1.0

echo "[demo] seeding TLS observations from bundled zgrab2 fixture..."
./target/debug/dxq-net from-zgrab \
  --input crates/dxq-net/tests/fixtures/zgrab-tls13-pq.json \
  --collector http://127.0.0.1:8090/v1/events \
  >/tmp/dxq-net.log 2>&1

# Synthetic agility-tagged asset so the BLOCKED list isn't empty.
echo "[demo] seeding one synthetic locked asset for the BLOCKED demo..."
curl -sS -X POST http://127.0.0.1:8090/v1/events \
  -H 'content-type: application/json' \
  -d '{
    "schema_version": 1,
    "schema_minor": 1,
    "source_module": "dxq-agility",
    "observed_at": "2026-05-13T08:00:00Z",
    "asset": {"kind": "tls_session", "identity": "fips-locked-appliance", "host": "appliance.tek.example"},
    "primitives": [
      {"role": "kex",     "algorithm": "X25519",      "pq_resistant": false},
      {"role": "sig",     "algorithm": "ECDSA-P256",  "pq_resistant": false},
      {"role": "encrypt", "algorithm": "AES-256-GCM", "pq_resistant": true},
      {"role": "hash",    "algorithm": "SHA-384",     "pq_resistant": true}
    ],
    "agility": {
      "level": "locked",
      "level_score": 0.20,
      "evidence": [{"type": "vendor_declaration", "statement": "FIPS 140-3 tested configuration enumerates only classical algorithms."}],
      "scanner_version": "demo",
      "rubric_version": "qra-rubric/v1.0"
    },
    "posture": {"score": 0, "rationale": "demo"}
  }' >/dev/null

echo
echo "[demo] up and seeded."
echo
echo "  Posture API:    http://127.0.0.1:8090/v1/posture"
echo "  Inventory API:  http://127.0.0.1:8090/v1/inventory"
echo "  Dashboard dev:  cd web && npm run dev  (proxies to 8090)"
echo
echo "Tailing the collector + emulator logs. Ctrl-C to stop everything."
tail -n +1 -F /tmp/dxq-server.log /tmp/dxq-qkd.log /tmp/dxq-kme.log /tmp/dxq-net.log
