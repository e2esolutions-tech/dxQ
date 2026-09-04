#!/usr/bin/env python3
"""scripts/loadtest.py — V1 throughput probe against POST /v1/events.

A small, dependency-free load generator: fans out N concurrent
POSTs to the collector, records per-request latency, and prints
rate plus latency percentiles when the run is done. Used by the
SEZ-8 load-test pass and by anyone who wants to know how a fresh
ree0xq-server build holds up before promoting it.

Usage:

    scripts/loadtest.py                 # 1000 events @ concurrency 16
    scripts/loadtest.py --n 5000 --concurrency 32
    scripts/loadtest.py --url http://127.0.0.1:8190/v1/events

Exit code is 0 when every POST returned 2xx, 1 otherwise. Python
stdlib only; no extra dependencies to install.
"""

from __future__ import annotations

import argparse
import concurrent.futures as cf
import json
import sys
import time
import urllib.request


def post_event(url: str, idx: int, timeout: float) -> tuple[int, float]:
    """Issue one POST. Returns (status_code, latency_seconds).

    A status of -1 means a transport-level failure (timeout, DNS,
    refused connection); the latency in that case is still useful
    for diagnosing where the run gave up.
    """
    body = json.dumps(
        {
            "schema_version": 1,
            "schema_minor": 1,
            "source_module": "loadtest",
            "observed_at": "2026-05-20T12:00:00Z",
            "asset": {
                "kind": "tls_session",
                "identity": f"loadtest-{idx:08d}",
                "host": "loadtest.example",
            },
            "primitives": [
                {
                    "role": "kex",
                    "algorithm": "X25519MLKEM768",
                    "pq_resistant": True,
                }
            ],
            "posture": {"score": 0, "rationale": "loadtest"},
        }
    ).encode("utf-8")
    req = urllib.request.Request(
        url,
        data=body,
        headers={"content-type": "application/json"},
        method="POST",
    )
    started = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            resp.read()
            return resp.status, time.perf_counter() - started
    except urllib.error.HTTPError as e:
        return e.code, time.perf_counter() - started
    except Exception:
        return -1, time.perf_counter() - started


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument(
        "--url",
        default="http://127.0.0.1:8090/v1/events",
        help="POST target (default: http://127.0.0.1:8090/v1/events)",
    )
    p.add_argument("--n", type=int, default=1000, help="event count (default: 1000)")
    p.add_argument(
        "--concurrency", type=int, default=16, help="worker count (default: 16)"
    )
    p.add_argument(
        "--timeout",
        type=float,
        default=5.0,
        help="per-request timeout seconds (default: 5)",
    )
    args = p.parse_args()

    print(
        f"[loadtest] POSTing {args.n} events to {args.url} "
        f"with concurrency {args.concurrency}",
        file=sys.stderr,
    )
    started = time.perf_counter()
    with cf.ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        results = list(
            pool.map(lambda i: post_event(args.url, i, args.timeout), range(args.n))
        )
    elapsed = time.perf_counter() - started

    statuses = [r[0] for r in results]
    successes = sum(1 for s in statuses if 200 <= s < 300)
    failures = len(statuses) - successes
    rate = len(statuses) / elapsed if elapsed > 0 else float("inf")

    print(
        f"[loadtest] sent={len(statuses)} 2xx={successes} "
        f"fail={failures} elapsed={elapsed:.2f}s rate={rate:.1f} req/s"
    )

    # Compute latency percentiles over successful requests only —
    # mixing in transport failures' partial latencies would
    # understate p99 by reporting the "failed-fast" tail.
    ok_latencies_ms = sorted(
        [r[1] * 1000.0 for r in results if 200 <= r[0] < 300]
    )
    if ok_latencies_ms:
        n = len(ok_latencies_ms)

        def pct(p: float) -> float:
            return ok_latencies_ms[min(n - 1, int(n * p))]

        print(
            f"[loadtest] latency ms (2xx only, n={n}): "
            f"p50={pct(0.50):.1f} p90={pct(0.90):.1f} "
            f"p99={pct(0.99):.1f} max={ok_latencies_ms[-1]:.1f}"
        )

    if failures:
        # Show the failure mix so the operator knows whether to
        # blame transport (-1), rate limits (429), validation
        # (422), or something else.
        from collections import Counter

        mix = Counter(s for s in statuses if not (200 <= s < 300))
        rendered = " ".join(f"{code}:{n}" for code, n in mix.most_common())
        print(f"[loadtest] failure mix: {rendered}", file=sys.stderr)

    return 0 if failures == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
