import { useEffect, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchPosture } from "../lib/api";
import { deadlineLabel, formatQ, qBgClass } from "../lib/posture";
import type { OrgPosture } from "../types/sezar";

export function PosturePage() {
  const [data, setData] = useState<OrgPosture | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchPosture()
      .then((r) => {
        if (!cancelled) setData(r);
      })
      .catch((e) => {
        if (!cancelled) setErr(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Org-level posture</h1>
        <p className="text-sm text-ink-600 mt-1">
          Deadline-adjusted quantum-risk score{" "}
          <code className="font-mono">q</code>, weighted across asset kinds.
          See <code className="font-mono">/v1/posture</code>.
        </p>
      </div>

      <LoadingError loading={data === null} error={err}>
        {data ? (
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="card p-6 col-span-1 md:col-span-2">
              <div className="text-xs uppercase tracking-wide text-ink-600">
                Org-level q
              </div>
              <div
                className={`mt-1 font-mono text-5xl font-bold ${qBgClass(
                  data.org_q,
                )} inline-block px-3 py-1 rounded`}
              >
                {formatQ(data.org_q)}
              </div>
              <div className="mt-4 text-sm text-ink-600">
                Computed under default weights (α=0.5, β=0.2, γ_max=0.3) with
                horizon = {data.horizon_years} years.
              </div>
              <div className="mt-2 text-sm">
                Deadline:{" "}
                <span className="font-mono">
                  {new Date(data.deadline).toISOString().slice(0, 10)}
                </span>{" "}
                <span className="text-ink-600">
                  ({deadlineLabel(data.deadline)})
                </span>
              </div>
            </div>
            <div className="card p-6 space-y-4">
              <div>
                <div className="text-xs uppercase tracking-wide text-ink-600">
                  Total assets
                </div>
                <div className="text-3xl font-bold">{data.assets}</div>
              </div>
              <div>
                <div className="text-xs uppercase tracking-wide text-ink-600">
                  BLOCKED count
                </div>
                <div className="text-3xl font-bold text-posture-critical">
                  {data.blocked_count}
                </div>
                <div className="text-xs text-ink-600 mt-1">
                  Agility ≤ Locked. Independent of q; surface action items
                  that need vendor or hardware programs.
                </div>
              </div>
            </div>
          </div>
        ) : null}
      </LoadingError>

      <div className="card p-6 text-sm text-ink-600">
        <strong className="text-ink-900">How to read this number.</strong>{" "}
        q is a deadline-adjusted prioritization signal, not absolute risk.
        Operators typically configure alert thresholds at q &gt; 0.6
        (must-migrate) and q &gt; 0.3 (plan-migration). Inter-org comparison
        requires shared deadline, weights, and corpora.
      </div>
    </div>
  );
}
