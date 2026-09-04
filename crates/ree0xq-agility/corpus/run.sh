#!/usr/bin/env bash
# corpus/run.sh — iterate the OSS-50 corpus, clone each pinned ref,
# run `ree0xq-agility scan` against it, and persist the resulting
# event JSON under corpus/results/.
#
# Usage:
#   ./corpus/run.sh                       # full corpus
#   PROJECT_FILTER=nginx ./corpus/run.sh  # one project (substring match)
#
# Outputs:
#   corpus/results/<project>.events.json — scanner output
#   corpus/results/agreement.tsv         — expected vs scanner agility level
#
# Requires:
#   - cargo (builds ree0xq-agility on demand if not on PATH)
#   - git, jq

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORPUS_CSV="$ROOT/corpus/oss-50-v1.csv"
RULES_DIR="$ROOT/rules/v1"
RESULTS_DIR="$ROOT/corpus/results"
SRC_CACHE_DIR="$ROOT/corpus/sources"
AGREEMENT_TSV="$RESULTS_DIR/agreement.tsv"

mkdir -p "$RESULTS_DIR" "$SRC_CACHE_DIR"

# Find the scanner binary. Prefer a release build under target/; fall
# back to `cargo run` so first-time users don't need a separate build step.
SCANNER_BIN="$(realpath -m "$ROOT/../../target/release/ree0xq-agility" 2>/dev/null || echo "")"
if [[ ! -x "$SCANNER_BIN" ]]; then
  SCANNER_BIN="$(realpath -m "$ROOT/../../target/debug/ree0xq-agility" 2>/dev/null || echo "")"
fi
if [[ ! -x "$SCANNER_BIN" ]]; then
  echo "Building ree0xq-agility (debug)..." >&2
  (cd "$ROOT/../.." && cargo build -p ree0xq-agility --bin ree0xq-agility >/dev/null)
  SCANNER_BIN="$(realpath -m "$ROOT/../../target/debug/ree0xq-agility")"
fi

echo -e "project\tcategory\texpected\tscanner\tagree" > "$AGREEMENT_TSV"

processed=0
agreed=0

# Skip header line; CSV uses no embedded commas in v1.
tail -n +2 "$CORPUS_CSV" | while IFS=, read -r project category repo_url pinned_ref expected_level reviewer_notes; do
  if [[ -n "${PROJECT_FILTER:-}" && "$project" != *"$PROJECT_FILTER"* ]]; then
    continue
  fi

  src_dir="$SRC_CACHE_DIR/$project"
  if [[ ! -d "$src_dir/.git" ]]; then
    echo "[$project] cloning $repo_url at $pinned_ref" >&2
    git clone --quiet --depth=1 --branch "$pinned_ref" "$repo_url" "$src_dir" \
      || { echo "[$project] clone failed; skipping" >&2; continue; }
  else
    echo "[$project] cache hit at $src_dir" >&2
  fi

  out="$RESULTS_DIR/$project.events.json"
  echo "[$project] scanning -> $out" >&2
  "$SCANNER_BIN" scan --target "$src_dir" --rules "$RULES_DIR" > "$out"

  scanner_level="$(jq -r '.level // "unknown"' "$out")"
  agree="no"
  if [[ "$scanner_level" == "$expected_level" ]]; then
    agree="yes"
  fi
  printf "%s\t%s\t%s\t%s\t%s\n" "$project" "$category" "$expected_level" "$scanner_level" "$agree" >> "$AGREEMENT_TSV"

  processed=$((processed + 1))
  if [[ "$agree" == "yes" ]]; then
    agreed=$((agreed + 1))
  fi
done

# Summary (subshell scopes processed/agreed; recompute from the TSV).
total=$(($(wc -l < "$AGREEMENT_TSV") - 1))
agreed_n=$(awk -F'\t' 'NR>1 && $5=="yes" {n++} END{print n+0}' "$AGREEMENT_TSV")
echo ""
echo "===== Corpus run summary ====="
echo "processed: $total"
echo "scanner ↔ ground-truth agreement: $agreed_n / $total"
echo "TSV: $AGREEMENT_TSV"
