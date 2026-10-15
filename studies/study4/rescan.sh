#!/usr/bin/env bash
# Study 4 — capture one adoption epoch.
#
# Reuses the Study 1 probe pair (classical ssl baseline + the
# rustls-post-quantum PQ-capable probe shipped as `ree0xq-net
# pq-probe`) against the current Tranco top-1k, stores the raw
# NDJSON under epochs/<label>/, diffs it against the frozen 6G8PX
# baseline, and writes summary.json for the time series.
#
# Usage:  ./rescan.sh <epoch-label> <tranco-hosts.txt>
#   e.g.  ./rescan.sh 2026-Q4 ../study1/tranco/tranco-6G8PX-1k.txt
#
# Ethics: single TLS handshake per host, 5 s timeouts, 1 Hz rate
# cap (inherited from ree0xq-net pq-probe defaults). Same footprint
# as Study 1.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$ROOT/../.." && pwd)"

LABEL="${1:?usage: rescan.sh <epoch-label> <hosts.txt>}"
HOSTS="${2:?usage: rescan.sh <epoch-label> <hosts.txt>}"
EPOCH_DIR="$ROOT/epochs/$LABEL"
mkdir -p "$EPOCH_DIR"

echo "[study4] building ree0xq-net pq-probe…"
cargo build --release -q -p ree0xq-net --manifest-path "$REPO/Cargo.toml"

echo "[study4] PQ-capable probe over $(wc -l < "$HOSTS") hosts → pq-scan.ndjson"
"$REPO/target/release/ree0xq-net" pq-probe --hosts "$HOSTS" \
  > "$EPOCH_DIR/pq-scan.ndjson" 2> "$EPOCH_DIR/pq-scan.log" || true

echo "[study4] diffing against the frozen 6G8PX baseline…"
DATE="$(date -u +%Y-%m-%d)"
python3 "$ROOT/diff_epoch.py" "$EPOCH_DIR/pq-scan.ndjson" --json \
  | python3 -c "import json,sys; d=json.load(sys.stdin); d['epoch']='$LABEL'; d['date']='$DATE'; print(json.dumps(d, indent=2))" \
  > "$EPOCH_DIR/summary.json"

python3 "$ROOT/diff_epoch.py" "$EPOCH_DIR/pq-scan.ndjson"
echo "[study4] wrote $EPOCH_DIR/summary.json"
echo "[study4] rebuild the curve: python3 build_timeseries.py && python3 plot_curve.py"
