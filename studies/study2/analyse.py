#!/usr/bin/env python3
"""Analyse Study 2 captures and produce plots + a summary TSV.

Inputs (one set per scenario):
  captures/<scenario>.events.json  — sezar-server /v1/events output
  captures/<scenario>.replay.json  — original scenario file
  captures/<scenario>.replay_start_ns  — wall-clock ns at replay start

Outputs:
  plots/<scenario>-timeline.png    — link_health over replay-relative time
  plots/<scenario>-timeline.pdf    — same, vector
  plots/study2-summary.tsv         — per-scenario per-transition latency
  plots/study2-summary.json        — same, machine-readable
"""

from __future__ import annotations

import json
import statistics
from datetime import datetime
from pathlib import Path

import matplotlib.pyplot as plt
import yaml

ROOT = Path(__file__).resolve().parent
CAPTURE_DIR = ROOT / "captures"
PLOT_DIR = ROOT / "plots"
PLOT_DIR.mkdir(exist_ok=True)


def parse_rfc3339(ts: str) -> datetime:
    # Python 3.11+ tolerates a fractional component and "Z" suffix.
    return datetime.fromisoformat(ts.replace("Z", "+00:00"))


def load_scenario(name: str):
    events = json.loads((CAPTURE_DIR / f"{name}.events.json").read_text())
    raw_replay = (CAPTURE_DIR / f"{name}.replay.json").read_text()
    # `run.sh` falls back to copying the YAML scenario file when jq
    # can't ingest it, so try JSON first then YAML.
    try:
        replay = json.loads(raw_replay)
    except json.JSONDecodeError:
        replay = yaml.safe_load(raw_replay)
    start_ns = int((CAPTURE_DIR / f"{name}.replay_start_ns").read_text().strip())
    return events["events"], replay, start_ns


def event_health(ev: dict) -> str | None:
    cp = ev.get("channel_protection")
    if not cp:
        return None
    return cp.get("link_health")


HEALTH_NUMERIC = {"ok": 2, "degraded": 1, "failed": 0}


def replay_relative_seconds(ev_ts: str, start_ns: int) -> float:
    # The collector stamps RFC3339; replay_start_ns is wall-clock ns.
    ev_wall = parse_rfc3339(ev_ts).timestamp()  # seconds since epoch (UTC)
    start_wall = start_ns / 1_000_000_000.0
    return ev_wall - start_wall


def plot_timeline(name: str, events: list, replay: dict, start_ns: int):
    """One plot per scenario: replay-relative time on x, link_health on y."""
    # Each event has observed_at + channel_protection.link_health.
    # The collector polls every 1s so we get a dense timeline.
    rows = []
    for ev in events:
        h = event_health(ev)
        if h is None:
            continue
        t = replay_relative_seconds(ev["observed_at"], start_ns)
        rows.append((t, h))
    # Sort ascending (events were returned newest-first).
    rows.sort(key=lambda r: r[0])

    fig, ax = plt.subplots(figsize=(7, 3.2))
    if not rows:
        ax.text(0.5, 0.5, f"no link_health observations captured for {name}",
                ha="center", va="center", transform=ax.transAxes)
    else:
        xs = [r[0] for r in rows]
        ys = [HEALTH_NUMERIC[r[1]] for r in rows]
        # Step plot is the right semantic for a categorical timeline.
        ax.step(xs, ys, where="post", linewidth=2, color="#1A73E8")
        ax.scatter(xs, ys, s=10, color="#1A73E8", zorder=3)

    # Mark replay control-op moments on the x axis.
    for ev in replay.get("events", []):
        ax.axvline(ev["at_seconds"], linestyle=":", color="#5F6368",
                   linewidth=0.7, alpha=0.7)
        if ev.get("label"):
            ax.text(ev["at_seconds"], 2.15, ev["label"], fontsize=7,
                    rotation=20, ha="left", va="bottom", color="#5F6368")

    ax.set_yticks([0, 1, 2])
    ax.set_yticklabels(["failed", "degraded", "ok"])
    ax.set_xlim(left=0)
    ax.set_xlabel("replay-relative time (s)")
    ax.set_ylabel("link_health")
    ax.set_title(f"Study 2 — {name}: {replay.get('description', '')}",
                 fontsize=10, loc="left")
    ax.grid(True, alpha=0.3)
    fig.tight_layout()
    fig.savefig(PLOT_DIR / f"{name}-timeline.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOT_DIR / f"{name}-timeline.pdf", bbox_inches="tight")
    plt.close(fig)


def compute_transitions(name: str, events: list, replay: dict, start_ns: int):
    """For each transition the operator induced via the replay, find the
    first event whose link_health reflects the expected new state, and
    record the wall-clock latency from control op to observed event."""
    # Build a temporal sorted view.
    samples = []
    for ev in events:
        h = event_health(ev)
        if h is None:
            continue
        t = replay_relative_seconds(ev["observed_at"], start_ns)
        samples.append((t, h))
    samples.sort(key=lambda r: r[0])

    transitions = []
    for ctrl in replay.get("events", []):
        op = ctrl["op"]["op"]
        # What health do we expect AFTER this op?
        expected = None
        if op == "force_failure":
            expected = "failed"
        elif op == "clear_failure":
            # depends on QBER at the time — typically "ok" or "degraded"
            expected = "ok"
        elif op == "set_qber":
            q = ctrl["op"]["qber"]
            if q >= 0.11:
                expected = "failed"
            elif q >= 0.05:
                expected = "degraded"
            else:
                expected = "ok"
        if expected is None:
            continue
        # First post-op sample matching the expected health.
        t_op = ctrl["at_seconds"]
        match = next(((t, h) for (t, h) in samples
                      if t >= t_op and h == expected), None)
        latency = (match[0] - t_op) if match else None
        transitions.append({
            "scenario": name,
            "at_seconds": t_op,
            "label": ctrl.get("label", ""),
            "op": op,
            "expected": expected,
            "observed_t": match[0] if match else None,
            "latency_s": latency,
        })
    return transitions


def main():
    summary = []
    scenario_names = sorted(p.stem.split(".")[0]
                            for p in CAPTURE_DIR.glob("*.events.json"))

    for name in scenario_names:
        events, replay, start_ns = load_scenario(name)
        plot_timeline(name, events, replay, start_ns)
        transitions = compute_transitions(name, events, replay, start_ns)
        summary.extend(transitions)
        print(f"[{name}] events={len(events)} "
              f"transitions={len(transitions)} "
              f"first_observed_at={events[0]['observed_at'] if events else 'n/a'}")

    # Per-scenario latency histogram.
    fig, ax = plt.subplots(figsize=(6, 3))
    latencies = [t["latency_s"] for t in summary if t["latency_s"] is not None]
    if latencies:
        ax.hist(latencies, bins=20, color="#1A73E8", edgecolor="white")
        med = statistics.median(latencies)
        p95 = statistics.quantiles(latencies, n=20)[-1] if len(latencies) >= 20 else max(latencies)
        ax.axvline(med, linestyle="--", color="#188038",
                   label=f"p50 = {med:.2f}s")
        ax.axvline(p95, linestyle="--", color="#D93025",
                   label=f"p95 ≈ {p95:.2f}s")
        ax.set_xlabel("observation latency (s)")
        ax.set_ylabel("control-op transitions")
        ax.set_title("Study 2 — collector observation latency over all scenarios",
                     fontsize=10, loc="left")
        ax.legend(loc="upper right", fontsize=9)
    fig.tight_layout()
    fig.savefig(PLOT_DIR / "study2-latency-hist.png", dpi=180, bbox_inches="tight")
    fig.savefig(PLOT_DIR / "study2-latency-hist.pdf", bbox_inches="tight")
    plt.close(fig)

    # TSV + JSON
    tsv = ["scenario\tat_s\top\texpected\tobserved_t\tlatency_s\tlabel"]
    for r in summary:
        tsv.append("\t".join([
            r["scenario"],
            str(r["at_seconds"]),
            r["op"],
            r["expected"],
            f"{r['observed_t']:.2f}" if r["observed_t"] is not None else "n/a",
            f"{r['latency_s']:.2f}" if r["latency_s"] is not None else "n/a",
            r["label"],
        ]))
    (PLOT_DIR / "study2-summary.tsv").write_text("\n".join(tsv) + "\n")
    (PLOT_DIR / "study2-summary.json").write_text(json.dumps(summary, indent=2))

    print()
    print("=" * 60)
    print(f"transitions={len(summary)} latencies_recorded={len(latencies)}")
    if latencies:
        print(f"  p50={statistics.median(latencies):.2f}s")
        print(f"  mean={statistics.mean(latencies):.2f}s")
        print(f"  max={max(latencies):.2f}s")
    print("plots:")
    for p in sorted(PLOT_DIR.glob("*.png")):
        print(f"  {p}")


if __name__ == "__main__":
    main()
