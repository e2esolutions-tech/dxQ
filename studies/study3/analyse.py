#!/usr/bin/env python3
"""Analyse Study 3 results: agreement matrix + per-category breakdown.

Inputs:
  results/agreement.tsv  — produced by studies/study3/run.sh
  results/<project>.events.json  — sezar-agility output per project

Outputs:
  plots/study3-agreement-matrix.png/.pdf
  plots/study3-distribution.png/.pdf
  plots/study3-summary.json (machine-readable)
"""

from __future__ import annotations

import csv
import json
from collections import defaultdict
from pathlib import Path

import matplotlib.pyplot as plt
import numpy as np

ROOT = Path(__file__).resolve().parent
RESULTS = ROOT / "results"
PLOTS = ROOT / "plots"
PLOTS.mkdir(exist_ok=True)

LEVELS = ["negotiated", "configurable", "pinned", "locked", "frozen"]


def load_rows():
    rows = []
    with (RESULTS / "agreement.tsv").open() as fp:
        rdr = csv.DictReader(fp, delimiter="\t")
        for r in rdr:
            r["evidence_count"] = int(r["evidence_count"])
            rows.append(r)
    return rows


def confusion_matrix(rows):
    m = defaultdict(lambda: defaultdict(int))
    for r in rows:
        m[r["expected"]][r["scanner"]] += 1
    return m


def cohen_kappa(rows):
    """Cohen's kappa over the present level vocabulary."""
    levels = sorted({r["expected"] for r in rows} | {r["scanner"] for r in rows})
    n = len(rows)
    if n == 0 or not levels:
        return float("nan")
    po = sum(1 for r in rows if r["expected"] == r["scanner"]) / n
    pe = 0.0
    for lv in levels:
        p_e = sum(1 for r in rows if r["expected"] == lv) / n
        p_s = sum(1 for r in rows if r["scanner"] == lv) / n
        pe += p_e * p_s
    if pe >= 1.0:
        return float("nan")
    return (po - pe) / (1.0 - pe)


def plot_confusion(rows):
    m = confusion_matrix(rows)
    present_levels = [lv for lv in LEVELS if any(
        m[exp].get(lv, 0) > 0 or sum(m[exp].values()) > 0 for exp in m
    ) or any(lv == s and m[e].get(s, 0) > 0 for e in m for s in m[e])]
    if not present_levels:
        present_levels = ["configurable", "pinned"]
    # Build numpy matrix
    mat = np.zeros((len(present_levels), len(present_levels)), dtype=int)
    for i, exp in enumerate(present_levels):
        for j, sc in enumerate(present_levels):
            mat[i, j] = m[exp].get(sc, 0)

    fig, ax = plt.subplots(figsize=(5, 4))
    im = ax.imshow(mat, cmap="Blues", vmin=0)
    ax.set_xticks(range(len(present_levels)))
    ax.set_xticklabels(present_levels, rotation=20)
    ax.set_yticks(range(len(present_levels)))
    ax.set_yticklabels(present_levels)
    ax.set_xlabel("scanner level")
    ax.set_ylabel("hand-graded level")
    ax.set_title("Study 3 — confusion matrix\n"
                 f"(n={len(rows)} projects; agreement={sum(1 for r in rows if r['match']=='yes')}/{len(rows)})",
                 fontsize=10, loc="left")
    for i in range(len(present_levels)):
        for j in range(len(present_levels)):
            ax.text(j, i, str(mat[i, j]), ha="center", va="center",
                    color="white" if mat[i, j] > mat.max() / 2 else "black",
                    fontweight="bold")
    fig.colorbar(im, ax=ax, shrink=0.7)
    fig.tight_layout()
    fig.savefig(PLOTS / "study3-agreement-matrix.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOTS / "study3-agreement-matrix.pdf", bbox_inches="tight")
    plt.close(fig)


def plot_distribution(rows):
    by_cat_level = defaultdict(lambda: defaultdict(int))
    for r in rows:
        by_cat_level[r["category"]][r["scanner"]] += 1
    cats = sorted(by_cat_level.keys())
    present_levels = ["negotiated", "configurable", "pinned", "locked", "frozen"]
    fig, ax = plt.subplots(figsize=(7, 4))
    bottom = np.zeros(len(cats))
    colors = {
        "negotiated": "#16a34a",
        "configurable": "#84cc16",
        "pinned": "#eab308",
        "locked": "#ea580c",
        "frozen": "#b91c1c",
    }
    for lv in present_levels:
        heights = [by_cat_level[c].get(lv, 0) for c in cats]
        if sum(heights) == 0:
            continue
        ax.bar(cats, heights, bottom=bottom, label=lv, color=colors[lv])
        bottom = bottom + np.array(heights)
    ax.set_ylabel("projects")
    ax.set_xticklabels(cats, rotation=25, ha="right")
    ax.legend(loc="upper right", fontsize=9, title="scanner level")
    ax.set_title("Study 3 — scanner-reported agility level, by category",
                 fontsize=10, loc="left")
    fig.tight_layout()
    fig.savefig(PLOTS / "study3-distribution.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOTS / "study3-distribution.pdf", bbox_inches="tight")
    plt.close(fig)


def main():
    rows = load_rows()
    n = len(rows)
    agreed = sum(1 for r in rows if r["match"] == "yes")
    kappa = cohen_kappa(rows)

    plot_confusion(rows)
    plot_distribution(rows)

    summary = {
        "project_count": n,
        "agreed": agreed,
        "agreement_rate": agreed / n if n else None,
        "cohen_kappa": kappa,
        "disagreements": [
            {"project": r["project"], "expected": r["expected"],
             "scanner": r["scanner"], "evidence_count": r["evidence_count"]}
            for r in rows if r["match"] != "yes"
        ],
        "per_category": {},
    }
    by_cat = defaultdict(list)
    for r in rows:
        by_cat[r["category"]].append(r)
    for cat, group in by_cat.items():
        summary["per_category"][cat] = {
            "count": len(group),
            "agreed": sum(1 for r in group if r["match"] == "yes"),
        }
    (PLOTS / "study3-summary.json").write_text(json.dumps(summary, indent=2))

    print(f"projects={n} agreed={agreed} agreement_rate={agreed / n:.2f} kappa={kappa:.3f}")
    print("disagreements:")
    for d in summary["disagreements"]:
        print(f"  {d['project']}: expected={d['expected']} scanner={d['scanner']} evidence={d['evidence_count']}")
    print(f"plots: {PLOTS}")


if __name__ == "__main__":
    main()
