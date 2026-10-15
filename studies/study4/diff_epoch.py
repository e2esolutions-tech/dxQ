#!/usr/bin/env python3
"""Study 4 — diff one epoch's PQ-scan against the frozen Study 1
baseline.

Both inputs are NDJSON in the Study 1 `pq-scan` schema (one object
per host, keyed by `host`, with `ok` and `kex_pq`). The Study 1
baseline is `studies/study1/captures/pq-scan-tranco-1k.ndjson`
(snapshot 6G8PX, 2026-05-13) and is NEVER modified by this tool.

Per-host transitions reported (only over hosts responsive in BOTH
snapshots, so adoption is measured on a stable denominator):
  - adopted      : classical -> PQ
  - regressed    : PQ -> classical
  - stable_pq    : PQ in both
  - stable_cl    : classical in both

Also reported (informational, not part of the rate):
  - newly_ok / newly_down : hosts that became / stopped being
    responsive between the two snapshots.

Usage:
  diff_epoch.py <epoch.ndjson> [--baseline <ndjson>] [--json]
"""
from __future__ import annotations
import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
DEFAULT_BASELINE = (
    ROOT.parent / "study1" / "captures" / "pq-scan-tranco-1k.ndjson"
)


def load(p: Path) -> dict[str, dict]:
    out: dict[str, dict] = {}
    for line in p.read_text().splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        r = json.loads(line)
        out[r["host"]] = r
    return out


def is_pq(r: dict) -> bool:
    return bool(r.get("ok")) and bool(r.get("kex_pq"))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("epoch")
    ap.add_argument("--baseline", default=str(DEFAULT_BASELINE))
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    base = load(Path(args.baseline))
    cur = load(Path(args.epoch))

    both = sorted(set(base) & set(cur))
    both_ok = [h for h in both if base[h].get("ok") and cur[h].get("ok")]

    adopted, regressed, stable_pq, stable_cl = [], [], [], []
    for h in both_ok:
        b, c = is_pq(base[h]), is_pq(cur[h])
        if not b and c:
            adopted.append(h)
        elif b and not c:
            regressed.append(h)
        elif b and c:
            stable_pq.append(h)
        else:
            stable_cl.append(h)

    base_ok = {h for h in base if base[h].get("ok")}
    cur_ok = {h for h in cur if cur[h].get("ok")}
    newly_ok = sorted((cur_ok - base_ok) & set(cur))
    newly_down = sorted((base_ok - cur_ok) & set(base))

    cur_pq = sum(1 for h in cur if is_pq(cur[h]))
    cur_ok_n = len(cur_ok)

    result = {
        "baseline_hosts": len(base),
        "epoch_hosts": len(cur),
        "responsive_in_both": len(both_ok),
        "epoch_ok": cur_ok_n,
        "epoch_pq": cur_pq,
        "epoch_pq_pct": round(100 * cur_pq / cur_ok_n, 1) if cur_ok_n else 0.0,
        "transitions": {
            "adopted": len(adopted),
            "regressed": len(regressed),
            "stable_pq": len(stable_pq),
            "stable_classical": len(stable_cl),
        },
        "availability": {
            "newly_ok": len(newly_ok),
            "newly_down": len(newly_down),
        },
        "adopted_hosts": adopted,
        "regressed_hosts": regressed,
    }

    if args.json:
        print(json.dumps(result, indent=2))
    else:
        t = result["transitions"]
        print(f"responsive in both : {result['responsive_in_both']}")
        print(f"epoch PQ           : {cur_pq}/{cur_ok_n} = {result['epoch_pq_pct']}%")
        print(f"  adopted (cl->PQ) : {t['adopted']}  {adopted[:8]}{'...' if len(adopted)>8 else ''}")
        print(f"  regressed (PQ->cl): {t['regressed']}  {regressed[:8]}{'...' if len(regressed)>8 else ''}")
        print(f"  stable PQ        : {t['stable_pq']}")
        print(f"  stable classical : {t['stable_classical']}")
        print(f"availability       : +{len(newly_ok)} newly ok, -{len(newly_down)} down")


if __name__ == "__main__":
    main()
