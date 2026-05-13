#!/usr/bin/env bash
# studies/study2/run.sh
#
# Drive each compressed replay scenario against a fresh
# sezar-qkd-kme-emulator + sezar-qkd collector + sezar-server.
# For each scenario, capture every emitted event and the replay
# timeline so the analysis script can compute observation latency
# and classification correctness.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

SCENARIOS_DIR="$ROOT/crates/sezar-qkd/scenarios-fast"
OUT_DIR="$ROOT/studies/study2/captures"
mkdir -p "$OUT_DIR"

# Use distinct ports so concurrent runs don't collide.
PORT_SERVER=18190
PORT_KME=18171

echo "[study2] building workspace..."
cargo build --workspace --quiet

for scenario_file in "$SCENARIOS_DIR"/*.yaml; do
  name=$(basename "$scenario_file" .yaml)
  echo
  echo "================================================"
  echo "[$name] starting run"
  echo "================================================"

  # Boot a fresh server + emulator + collector for each scenario so
  # the captures aren't contaminated by leftover state.
  ./target/debug/sezar-server \
    --listen "127.0.0.1:$PORT_SERVER" \
    --deadline 2030-01-01T00:00:00Z \
    >/tmp/study2-server.log 2>&1 &
  SRV_PID=$!

  ./target/debug/sezar-qkd-kme-emulator \
    --listen "127.0.0.1:$PORT_KME" \
    --kme-id KME-A \
    --paired-kme KME-B \
    >/tmp/study2-kme.log 2>&1 &
  KME_PID=$!

  # Wait for both to bind.
  sleep 0.4

  # Run collector — short status interval so we get tight measurements.
  ./target/debug/sezar-qkd \
    --kme "http://127.0.0.1:$PORT_KME/api/v1" \
    --collector "http://127.0.0.1:$PORT_SERVER/v1/events" \
    --status-poll-interval 1 \
    --slave-sae-id SAE-STUDY2 \
    >/tmp/study2-qkd.log 2>&1 &
  QKD_PID=$!

  sleep 0.3

  # Run the replay scenario. Record replay start time so the analyser
  # can compute per-event latency offsets later.
  REPLAY_START_NS=$(date +%s%N)
  ./target/debug/sezar-qkd-replay \
    --emulator-control "http://127.0.0.1:$PORT_KME/control" \
    --replay "$scenario_file" \
    >/tmp/study2-replay.log 2>&1 || true

  # Let one more poll cycle pass so the final state is captured.
  sleep 2

  # Pull all events out of the collector for this run.
  curl -sS "http://127.0.0.1:$PORT_SERVER/v1/events?limit=10000" \
    > "$OUT_DIR/$name.events.json"
  curl -sS "http://127.0.0.1:$PORT_SERVER/v1/qkd/links" \
    > "$OUT_DIR/$name.links.json"

  # Record the replay timeline for the analyser.
  jq --arg start_ns "$REPLAY_START_NS" \
     '. + { replay_start_ns: $start_ns }' \
     "$scenario_file" 2>/dev/null > "$OUT_DIR/$name.replay.json" \
   || cp "$scenario_file" "$OUT_DIR/$name.replay.json"
  echo "$REPLAY_START_NS" > "$OUT_DIR/$name.replay_start_ns"

  echo "[$name] captured $(jq '.count' < "$OUT_DIR/$name.events.json") events"

  # Tear down.
  kill "$QKD_PID" "$KME_PID" "$SRV_PID" 2>/dev/null || true
  wait 2>/dev/null || true
  # Brief pause so ports release cleanly before next iteration.
  sleep 0.3
done

echo
echo "[study2] captures written to $OUT_DIR"
ls -la "$OUT_DIR"
