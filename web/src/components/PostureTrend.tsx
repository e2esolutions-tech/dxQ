import { useMemo } from "react";

import { fetchPostureHistory } from "../lib/api";
import { qColorClass } from "../lib/posture";
import { usePolling } from "../lib/usePolling";
import type { PostureHistoryResponse } from "../types/ree0xq";

const TREND_INTERVAL_MS = 15_000;

/** Inline-SVG org_q trend line (SEZ-32). No chart library — keeps
 *  the bundle under its 300 KB budget. Draws the retained history
 *  window as a sparkline with a BLOCKED-count overlay. */
export function PostureTrend() {
  const hist = usePolling<PostureHistoryResponse>(
    () => fetchPostureHistory(),
    TREND_INTERVAL_MS,
  );

  const snaps = hist.data?.snapshots ?? [];

  const geom = useMemo(() => {
    if (snaps.length < 2) return null;
    const W = 640;
    const H = 140;
    const pad = { l: 8, r: 8, t: 10, b: 10 };
    const xs = snaps.map((s) => new Date(s.at).getTime());
    const t0 = xs[0];
    const tN = xs[xs.length - 1];
    const span = Math.max(1, tN - t0);
    // org_q is in [0,1]; fix the y-scale so the line is comparable
    // across polls rather than auto-fitting to the local range.
    const x = (t: number) =>
      pad.l + ((t - t0) / span) * (W - pad.l - pad.r);
    const y = (q: number) => pad.t + (1 - q) * (H - pad.t - pad.b);
    const path = snaps
      .map((s, i) => `${i === 0 ? "M" : "L"} ${x(xs[i]).toFixed(1)} ${y(s.org_q).toFixed(1)}`)
      .join(" ");
    const maxBlocked = Math.max(1, ...snaps.map((s) => s.blocked_count));
    const blockedPath = snaps
      .map(
        (s, i) =>
          `${i === 0 ? "M" : "L"} ${x(xs[i]).toFixed(1)} ${(
            pad.t +
            (1 - s.blocked_count / maxBlocked) * (H - pad.t - pad.b)
          ).toFixed(1)}`,
      )
      .join(" ");
    return { W, H, path, blockedPath, last: snaps[snaps.length - 1], maxBlocked };
  }, [snaps]);

  return (
    <div className="card p-6">
      <div className="flex items-center justify-between">
        <div className="text-xs uppercase tracking-wide text-ink-600">
          Org q — trend
        </div>
        <div className="text-xs text-ink-600 font-mono">
          {snaps.length} point{snaps.length === 1 ? "" : "s"}
        </div>
      </div>
      {geom ? (
        <>
          <svg
            viewBox={`0 0 ${geom.W} ${geom.H}`}
            className="w-full mt-3"
            preserveAspectRatio="none"
            role="img"
            aria-label="Organization q score over time"
          >
            {[0.25, 0.5, 0.75].map((g) => (
              <line
                key={g}
                x1={0}
                x2={geom.W}
                y1={10 + (1 - g) * (geom.H - 20)}
                y2={10 + (1 - g) * (geom.H - 20)}
                stroke="currentColor"
                className="text-ink-200"
                strokeWidth={1}
                strokeDasharray="3 3"
              />
            ))}
            <path
              d={geom.blockedPath}
              fill="none"
              stroke="currentColor"
              className="text-posture-urgent/40"
              strokeWidth={1.5}
            />
            <path
              d={geom.path}
              fill="none"
              stroke="currentColor"
              className={qColorClass(geom.last.org_q)}
              strokeWidth={2}
            />
          </svg>
          <div className="mt-2 flex gap-4 text-xs text-ink-600">
            <span>
              <span className="inline-block w-3 border-t-2 border-current align-middle" />{" "}
              org_q (0–1, top = low risk)
            </span>
            <span className="text-posture-urgent/70">
              <span className="inline-block w-3 border-t-2 border-current align-middle" />{" "}
              BLOCKED (0–{geom.maxBlocked})
            </span>
          </div>
        </>
      ) : (
        <p className="mt-3 text-sm text-ink-600">
          Collecting trend… at least two posture polls are needed to
          draw a line. History is in-memory for the process lifetime.
        </p>
      )}
    </div>
  );
}
