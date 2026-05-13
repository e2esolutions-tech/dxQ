#!/usr/bin/env bash
# studies/study3/run.sh
#
# Clone a curated subset of the OSS-50 corpus, run sezar-agility
# against each, and write a TSV of (expected_level, scanner_level,
# match) for the analysis step.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

SUBSET_CSV="$ROOT/studies/study3/subset.csv"
SRC_DIR="$ROOT/studies/study3/sources"
RESULTS_DIR="$ROOT/studies/study3/results"
AGREEMENT_TSV="$RESULTS_DIR/agreement.tsv"
RULES_DIR="$ROOT/crates/sezar-agility/rules/v1"

mkdir -p "$SRC_DIR" "$RESULTS_DIR"

echo "[study3] building sezar-agility..."
cargo build -p sezar-agility --bin sezar-agility --quiet
SCANNER="$ROOT/target/debug/sezar-agility"

echo -e "project\tcategory\texpected\tscanner\tmatch\tevidence_count" > "$AGREEMENT_TSV"

tail -n +2 "$SUBSET_CSV" | while IFS=, read -r project category repo_url pinned_ref expected_level reviewer_notes; do
  echo
  echo "------ $project ($category) ------"
  target="$SRC_DIR/$project"
  if [[ ! -d "$target/.git" ]]; then
    echo "[$project] cloning $repo_url (depth 1, branch $pinned_ref)"
    if ! git clone --quiet --depth=1 --branch "$pinned_ref" "$repo_url" "$target" 2>/dev/null; then
      # Some repos default to `main` rather than `master`; retry without --branch.
      rm -rf "$target"
      git clone --quiet --depth=1 "$repo_url" "$target" || {
        echo "[$project] clone failed; skipping"
        continue
      }
    fi
  else
    echo "[$project] cache hit"
  fi

  out="$RESULTS_DIR/$project.events.json"
  echo "[$project] scanning -> $out"
  "$SCANNER" scan --target "$target" --rules "$RULES_DIR" > "$out" 2>/dev/null

  scanner_level=$(jq -r '.level // "unknown"' "$out")
  ev_count=$(jq -r '.evidence | length' "$out")
  match="no"
  if [[ "$scanner_level" == "$expected_level" ]]; then
    match="yes"
  fi
  printf "%s\t%s\t%s\t%s\t%s\t%s\n" "$project" "$category" "$expected_level" "$scanner_level" "$match" "$ev_count" >> "$AGREEMENT_TSV"
  echo "[$project] expected=$expected_level scanner=$scanner_level evidence=$ev_count match=$match"
done

echo
echo "=========== agreement TSV ==========="
cat "$AGREEMENT_TSV"

total=$(($(wc -l < "$AGREEMENT_TSV") - 1))
agreed=$(awk -F'\t' 'NR>1 && $5=="yes" {n++} END{print n+0}' "$AGREEMENT_TSV")
echo
echo "scanner ↔ ground-truth agreement: $agreed / $total"
