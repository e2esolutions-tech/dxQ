#!/usr/bin/env python3
"""Analyse Study 1 captures and produce distribution plots.

Inputs:
  captures/scan.json          — Python ssl probe (classical baseline)
  captures/pq-scan.ndjson     — rustls + rustls-post-quantum probe
                                (advertises X25519MLKEM768; one JSON per line)

Outputs:
  plots/study1-distribution.{png,pdf}   — TLS 1.3 cipher + cert-sig dist
  plots/study1-pq-kex.{png,pdf}         — PQ vs classical kex split
  plots/study1-summary.json             — machine-readable
"""

from __future__ import annotations

import json
from collections import Counter
from pathlib import Path

import matplotlib.pyplot as plt

ROOT = Path(__file__).resolve().parent
CAPTURE_DIR = ROOT / "captures"
PLOTS = ROOT / "plots"
PLOTS.mkdir(exist_ok=True)


def main():
    rows = json.loads((CAPTURE_DIR / "scan.json").read_text())
    pq_rows = [
        json.loads(line)
        for line in (CAPTURE_DIR / "pq-scan.ndjson").read_text().splitlines()
        if line.strip()
    ]

    n = len(rows)
    n_ok = sum(1 for r in rows if r["ok"])
    print(f"classical probe: probes={n} ok={n_ok}")

    versions = Counter(r["version"] for r in rows if r["ok"])
    ciphers = Counter(r["cipher"] for r in rows if r["ok"])
    cert_algs = Counter(r["cert_sig_algo"] for r in rows if r["ok"])

    print("\nTLS versions:")
    for v, c in versions.most_common():
        print(f"  {v}: {c}")
    print("\nCiphersuites (classical probe):")
    for v, c in ciphers.most_common():
        print(f"  {v}: {c}")
    print("\nCert signature algorithms (classical probe):")
    for v, c in cert_algs.most_common():
        print(f"  {v}: {c}")

    # --- PQ probe analysis ---
    pq_ok = [r for r in pq_rows if r["ok"]]
    pq_count = sum(1 for r in pq_ok if r["kex_pq"])
    pq_groups = Counter(r["kex_group"] for r in pq_ok)
    pq_adopters = sorted(r["host"] for r in pq_ok if r["kex_pq"])
    pq_holdouts = sorted(r["host"] for r in pq_ok if not r["kex_pq"])

    print(f"\npq probe: probes={len(pq_rows)} ok={len(pq_ok)} "
          f"pq_kex_negotiated={pq_count}/{len(pq_ok)} ({pq_count/len(pq_ok):.0%})")
    print("\nNegotiated kex groups (PQ-capable probe):")
    for v, c in pq_groups.most_common():
        print(f"  {v}: {c}")

    # --- plots ---
    # Plot 1: classical cipher + cert-sig distribution
    fig, axes = plt.subplots(1, 2, figsize=(11, 4))
    cs_items = ciphers.most_common()
    axes[0].barh([c[0] for c in cs_items], [c[1] for c in cs_items], color="#1A73E8")
    axes[0].set_xlabel("hosts")
    axes[0].set_title("Negotiated TLS 1.3 ciphersuite (Study 1, n=30)",
                      fontsize=10, loc="left")
    axes[0].invert_yaxis()

    ca_items = cert_algs.most_common()
    axes[1].barh([c[0] for c in ca_items], [c[1] for c in ca_items], color="#188038")
    axes[1].set_xlabel("hosts")
    axes[1].set_title("Leaf certificate signature algorithm (Study 1, n=30)",
                      fontsize=10, loc="left")
    axes[1].invert_yaxis()
    fig.tight_layout()
    fig.savefig(PLOTS / "study1-distribution.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOTS / "study1-distribution.pdf", bbox_inches="tight")
    plt.close(fig)

    # Plot 2: PQ vs classical kex split
    fig, axes = plt.subplots(1, 2, figsize=(11, 4))
    # Left: stacked bar — PQ adoption headline
    labels = ["this corpus (n=30)"]
    pq_n = pq_count
    cl_n = len(pq_ok) - pq_count
    axes[0].barh(labels, [pq_n], color="#188038", label=f"PQ-KEM ({pq_n})")
    axes[0].barh(labels, [cl_n], left=[pq_n], color="#94a3b8",
                 label=f"classical only ({cl_n})")
    axes[0].set_xlim(0, len(pq_ok))
    axes[0].set_xlabel("hosts")
    axes[0].set_title("PQ-KEM negotiation when client advertises X25519MLKEM768\n"
                      f"(rustls + rustls-post-quantum; {pq_n}/{len(pq_ok)} = {pq_n/len(pq_ok):.0%})",
                      fontsize=10, loc="left")
    axes[0].legend(loc="lower right", fontsize=9)

    # Right: per-kex-group bars
    grp_items = pq_groups.most_common()
    colors = ["#188038" if g.startswith(("X25519MLKEM", "MLKEM", "SecP256r1MLKEM", "P384MLKEM"))
              else "#94a3b8"
              for (g, _) in grp_items]
    axes[1].barh([g for g, _ in grp_items], [n for _, n in grp_items], color=colors)
    axes[1].set_xlabel("hosts")
    axes[1].set_title("Negotiated key-exchange group (PQ-capable client)",
                      fontsize=10, loc="left")
    axes[1].invert_yaxis()
    fig.tight_layout()
    fig.savefig(PLOTS / "study1-pq-kex.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOTS / "study1-pq-kex.pdf", bbox_inches="tight")
    plt.close(fig)

    summary = {
        "n_classical": n,
        "n_ok_classical": n_ok,
        "tls_versions": dict(versions),
        "ciphersuites": dict(ciphers),
        "cert_signature_algorithms": dict(cert_algs),
        "n_pq": len(pq_rows),
        "n_ok_pq": len(pq_ok),
        "pq_kex_negotiated": pq_count,
        "pq_kex_rate": pq_count / len(pq_ok) if pq_ok else None,
        "kex_groups": dict(pq_groups),
        "pq_adopters": pq_adopters,
        "pq_holdouts": pq_holdouts,
    }
    (PLOTS / "study1-summary.json").write_text(json.dumps(summary, indent=2))
    print(f"\nplots: {PLOTS}")


if __name__ == "__main__":
    main()
