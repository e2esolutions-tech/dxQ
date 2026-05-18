#!/usr/bin/env python3
"""Tranco-1k Study 1 analyser — drop-in for n=1000.

Inputs:
  captures/scan-tranco-1k.json       — Python ssl baseline
  captures/pq-scan-tranco-1k.ndjson  — rustls + rustls-post-quantum

Outputs (plots/ side, alongside the n=30 pilot):
  plots/study1-tranco-distribution.{png,pdf}   — TLS 1.3 cipher + cert-sig
  plots/study1-tranco-pq-kex.{png,pdf}         — PQ vs classical split
  plots/study1-tranco-summary.json             — machine-readable
"""

from __future__ import annotations

import json
from collections import Counter
from pathlib import Path

import matplotlib.pyplot as plt

ROOT = Path(__file__).resolve().parent
CAPTURE = ROOT / "captures"
PLOTS = ROOT / "plots"
PLOTS.mkdir(exist_ok=True)


def main():
    classical_rows = json.loads((CAPTURE / "scan-tranco-1k.json").read_text())
    pq_rows = [
        json.loads(line)
        for line in (CAPTURE / "pq-scan-tranco-1k.ndjson").read_text().splitlines()
        if line.strip().startswith("{")
    ]

    n_cl = len(classical_rows)
    ok_cl = [r for r in classical_rows if r["ok"]]
    versions = Counter(r["version"] for r in ok_cl)
    ciphers = Counter(r["cipher"] for r in ok_cl)
    cert_algs = Counter(r["cert_sig_algo"] for r in ok_cl)

    n_pq = len(pq_rows)
    ok_pq = [r for r in pq_rows if r["ok"]]
    pq_count = sum(1 for r in ok_pq if r["kex_pq"])
    pq_groups = Counter(r["kex_group"] for r in ok_pq)

    print(f"classical probe: {len(ok_cl)}/{n_cl} ok ({len(ok_cl)/n_cl:.1%})")
    print("  TLS versions:")
    for v, c in versions.most_common():
        print(f"    {v}: {c}")
    print("  Ciphersuites:")
    for v, c in ciphers.most_common(5):
        print(f"    {v}: {c}")
    print("  Cert signatures:")
    for v, c in cert_algs.most_common(8):
        print(f"    {v}: {c}")

    print(f"\npq probe: {len(ok_pq)}/{n_pq} ok ({len(ok_pq)/n_pq:.1%})")
    print(f"  PQ-KEM negotiated: {pq_count}/{len(ok_pq)} = {pq_count/len(ok_pq):.1%}")
    print("  Kex groups:")
    for v, c in pq_groups.most_common(8):
        print(f"    {v}: {c}")

    # --- Plot 1: classical baseline distribution ---
    fig, axes = plt.subplots(1, 2, figsize=(12, 4.5))
    cs_top = ciphers.most_common(6)
    axes[0].barh([c[0] for c in cs_top], [c[1] for c in cs_top], color="#1A73E8")
    axes[0].set_xlabel("hosts")
    axes[0].set_title(
        f"Negotiated TLS 1.3 ciphersuite (Tranco-1k, n_ok={len(ok_cl)})",
        fontsize=10, loc="left",
    )
    axes[0].invert_yaxis()
    ca_top = cert_algs.most_common(8)
    axes[1].barh([c[0] for c in ca_top], [c[1] for c in ca_top], color="#188038")
    axes[1].set_xlabel("hosts")
    axes[1].set_title(
        f"Leaf certificate signature algorithm (Tranco-1k, n_ok={len(ok_cl)})",
        fontsize=10, loc="left",
    )
    axes[1].invert_yaxis()
    fig.tight_layout()
    fig.savefig(PLOTS / "study1-tranco-distribution.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOTS / "study1-tranco-distribution.pdf", bbox_inches="tight")
    plt.close(fig)

    # --- Plot 2: PQ vs classical kex split ---
    fig, axes = plt.subplots(1, 2, figsize=(12, 4.5))
    pq_n = pq_count
    cl_n = len(ok_pq) - pq_count
    axes[0].barh(["Tranco-1k"], [pq_n], color="#188038",
                 label=f"PQ-KEM ({pq_n})")
    axes[0].barh(["Tranco-1k"], [cl_n], left=[pq_n], color="#94a3b8",
                 label=f"classical only ({cl_n})")
    axes[0].set_xlim(0, len(ok_pq))
    axes[0].set_xlabel("hosts")
    axes[0].set_title(
        "PQ-KEM negotiation when client advertises X25519MLKEM768\n"
        f"(rustls + rustls-post-quantum; {pq_n}/{len(ok_pq)} = {pq_n/len(ok_pq):.1%})",
        fontsize=10, loc="left",
    )
    axes[0].legend(loc="lower right", fontsize=9)

    grp_top = pq_groups.most_common(6)
    colors = [
        "#188038"
        if g.startswith(("X25519MLKEM", "SecP256r1MLKEM", "P384MLKEM", "MLKEM"))
        else "#94a3b8"
        for g, _ in grp_top
    ]
    axes[1].barh([g for g, _ in grp_top], [n for _, n in grp_top], color=colors)
    axes[1].set_xlabel("hosts")
    axes[1].set_title("Negotiated key-exchange group (PQ-capable client)",
                      fontsize=10, loc="left")
    axes[1].invert_yaxis()
    fig.tight_layout()
    fig.savefig(PLOTS / "study1-tranco-pq-kex.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOTS / "study1-tranco-pq-kex.pdf", bbox_inches="tight")
    plt.close(fig)

    summary = {
        "tranco_list_id": "6G8PX",
        "tranco_list_date": "2026-05-13",
        "host_count": n_cl,
        "classical": {
            "ok": len(ok_cl),
            "tls_versions": dict(versions),
            "ciphersuites": dict(ciphers.most_common()),
            "cert_sig_algorithms": dict(cert_algs.most_common()),
        },
        "pq_capable": {
            "ok": len(ok_pq),
            "pq_kem_negotiated": pq_count,
            "pq_kem_rate": pq_count / len(ok_pq) if ok_pq else None,
            "kex_groups": dict(pq_groups.most_common()),
        },
    }
    (PLOTS / "study1-tranco-summary.json").write_text(json.dumps(summary, indent=2))
    print(f"\nplots: {PLOTS}")


if __name__ == "__main__":
    main()
