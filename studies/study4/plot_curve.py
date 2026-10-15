#!/usr/bin/env python3
"""Study 4 — plot the PQ-KEX adoption curve from timeseries.csv.

Outputs plots/adoption-curve.{png,pdf}. Degrades gracefully when
only the baseline epoch exists (draws a single marked point).
"""
from __future__ import annotations
import csv
from pathlib import Path

import matplotlib.pyplot as plt

ROOT = Path(__file__).resolve().parent
PLOTS = ROOT / "plots"
PLOTS.mkdir(exist_ok=True)


def main() -> None:
    ts = ROOT / "timeseries.csv"
    if not ts.exists():
        raise SystemExit("run build_timeseries.py first")
    rows = list(csv.DictReader(ts.open()))
    epochs = [r["epoch"] for r in rows]
    tls = [float(r["hybrid_pct"]) for r in rows]
    ssh = [
        round(100 * int(r["ssh_pq_count"]) / int(r["ssh_n"]), 1)
        if int(r["ssh_n"]) else None
        for r in rows
    ]

    fig, ax = plt.subplots(figsize=(8, 4.5))
    ax.plot(epochs, tls, marker="o", lw=2, color="#1A73E8", label="TLS hybrid PQ-KEX")
    if any(v is not None for v in ssh):
        xs = [e for e, v in zip(epochs, ssh) if v is not None]
        ys = [v for v in ssh if v is not None]
        ax.plot(xs, ys, marker="s", lw=2, color="#188038", label="SSH PQ-KEX")
    ax.set_ylabel("PQ-KEX adoption (% of responsive hosts)")
    ax.set_xlabel("Epoch")
    ax.set_title("Post-quantum key-exchange adoption — Tranco top-1k")
    ax.grid(True, alpha=0.3)
    ax.set_ylim(0, max(100, max(tls) + 10))
    ax.legend()
    for x, y in zip(epochs, tls):
        ax.annotate(f"{y}%", (x, y), textcoords="offset points",
                    xytext=(0, 8), ha="center", fontsize=8)
    fig.tight_layout()
    for ext in ("png", "pdf"):
        fig.savefig(PLOTS / f"adoption-curve.{ext}", bbox_inches="tight")
    print(f"wrote {PLOTS}/adoption-curve.pdf")


if __name__ == "__main__":
    main()
