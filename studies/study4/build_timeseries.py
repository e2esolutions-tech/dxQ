#!/usr/bin/env python3
"""Study 4 — assemble the adoption time series.

Reads the frozen Study 1 baseline as epoch "2026-Q2" plus every
`epochs/<label>/summary.json` produced by `rescan.sh`, and writes
`timeseries.csv`:

  epoch,date,n_ok,hybrid_count,hybrid_pct,ssh_n,ssh_pq_count

SSH columns are 0 until SEZ-30-based SSH epochs are captured.
"""
from __future__ import annotations
import csv
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent


def baseline_row() -> dict:
    s = json.loads(
        (ROOT.parent / "study1" / "plots" / "study1-tranco-summary.json").read_text()
    )
    pq = s.get("pq_capable", {})
    ok = pq.get("ok") or s["classical"]["ok"]
    hy = pq.get("pq_kex") or pq.get("hybrid") or 317
    return {
        "epoch": "2026-Q2",
        "date": s.get("tranco_list_date", "2026-05-13"),
        "n_ok": ok,
        "hybrid_count": hy,
        "hybrid_pct": round(100 * hy / ok, 1) if ok else 0.0,
        "ssh_n": 0,
        "ssh_pq_count": 0,
    }


def main() -> None:
    rows = [baseline_row()]
    for d in sorted((ROOT / "epochs").glob("*/summary.json")):
        s = json.loads(d.read_text())
        rows.append(
            {
                "epoch": s["epoch"],
                "date": s.get("date", ""),
                "n_ok": s.get("epoch_ok", 0),
                "hybrid_count": s.get("epoch_pq", 0),
                "hybrid_pct": s.get("epoch_pq_pct", 0.0),
                "ssh_n": s.get("ssh_n", 0),
                "ssh_pq_count": s.get("ssh_pq_count", 0),
            }
        )
    out = ROOT / "timeseries.csv"
    with out.open("w", newline="") as fh:
        w = csv.DictWriter(
            fh,
            fieldnames=["epoch", "date", "n_ok", "hybrid_count",
                        "hybrid_pct", "ssh_n", "ssh_pq_count"],
        )
        w.writeheader()
        w.writerows(rows)
    print(f"wrote {out} ({len(rows)} epochs)")
    for r in rows:
        print(f"  {r['epoch']} ({r['date']}): TLS {r['hybrid_pct']}% "
              f"({r['hybrid_count']}/{r['n_ok']})")


if __name__ == "__main__":
    main()
