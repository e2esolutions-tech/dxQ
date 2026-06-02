import { useMemo, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchDeadlines } from "../lib/api";
import { usePolling } from "../lib/usePolling";
import { safeHttpUrl } from "../lib/safeUrl";

const DEADLINES_INTERVAL_MS = 60_000;

// Group items by jurisdiction prefix so the operator can scan the
// table by regulator family rather than a flat date list.
function jurisdictionFamily(j: string): string {
  const dash = j.indexOf("-");
  return dash < 0 ? j : j.slice(0, dash);
}

function daysUntil(iso: string): number {
  const target = new Date(iso).getTime();
  const now = Date.now();
  return Math.round((target - now) / (1000 * 60 * 60 * 24));
}

function urgencyClass(days: number): string {
  if (days < 0) return "bg-ink-100 text-ink-600";
  if (days <= 365) return "bg-posture-urgent/15 text-posture-urgent";
  if (days <= 365 * 3) return "bg-posture-plan/15 text-posture-plan";
  return "bg-posture-good/15 text-posture-good";
}

export function DeadlinesPage() {
  const [jurisdiction, setJurisdiction] = useState<string>("all");
  const [horizonYears, setHorizonYears] = useState<string>("all");

  const opts = useMemo(() => {
    const o: { jurisdiction?: string; horizonDays?: number } = {};
    if (jurisdiction !== "all") o.jurisdiction = jurisdiction;
    if (horizonYears !== "all") o.horizonDays = Number(horizonYears) * 365;
    return o;
  }, [jurisdiction, horizonYears]);

  const deadlines = usePolling(
    () => fetchDeadlines(opts),
    DEADLINES_INTERVAL_MS,
    [opts.jurisdiction, opts.horizonDays],
  );

  const items = deadlines.data?.items ?? [];
  const families = useMemo(() => {
    const out = new Set<string>();
    for (const it of items) out.add(jurisdictionFamily(it.jurisdiction));
    return Array.from(out).sort();
  }, [items]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            Regulator deadlines
          </h1>
          <p className="text-sm text-ink-600 mt-1">
            Public PQ-mandate dates from US-NSA, US-NIST, EU-ANSSI, DE-BSI and
            UK-NCSC. Click any row to open the source document.
          </p>
        </div>
        <button
          type="button"
          onClick={deadlines.refresh}
          className="text-xs px-3 py-1 border border-ink-300 rounded hover:bg-ink-100"
        >
          refresh
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-4">
        <label className="text-sm">
          <span className="mr-2 text-ink-600">Jurisdiction:</span>
          <select
            value={jurisdiction}
            onChange={(e) => setJurisdiction(e.target.value)}
            className="border border-ink-200 rounded px-2 py-1 bg-white text-sm"
          >
            <option value="all">all</option>
            <option value="US">US</option>
            <option value="EU">EU</option>
            <option value="DE">DE</option>
            <option value="UK">UK</option>
          </select>
        </label>
        <label className="text-sm">
          <span className="mr-2 text-ink-600">Horizon:</span>
          <select
            value={horizonYears}
            onChange={(e) => setHorizonYears(e.target.value)}
            className="border border-ink-200 rounded px-2 py-1 bg-white text-sm"
          >
            <option value="all">all (forever)</option>
            <option value="1">≤ 1 year</option>
            <option value="3">≤ 3 years</option>
            <option value="5">≤ 5 years</option>
            <option value="10">≤ 10 years</option>
          </select>
        </label>
        <span className="text-xs text-ink-600 ml-auto font-mono">
          {items.length} entries
        </span>
      </div>

      <LoadingError
        loading={deadlines.loading}
        error={deadlines.error}
        empty={deadlines.data !== null && items.length === 0}
        emptyMessage="No deadlines match the filter."
      >
        <div className="space-y-6">
          {families.map((fam) => (
            <section key={fam}>
              <h2 className="text-sm font-semibold text-ink-800 mb-2">{fam}</h2>
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-xs text-ink-600 text-left border-b border-ink-200">
                      <th className="py-2 pr-3">Jurisdiction</th>
                      <th className="py-2 pr-3">Effective</th>
                      <th className="py-2 pr-3">Window</th>
                      <th className="py-2 pr-3">Asset class</th>
                      <th className="py-2 pr-3">Label</th>
                    </tr>
                  </thead>
                  <tbody>
                    {items
                      .filter((it) => jurisdictionFamily(it.jurisdiction) === fam)
                      .map((it, i) => {
                        const days = daysUntil(it.effective_date);
                        const window =
                          days < 0
                            ? `${Math.abs(days)} d ago`
                            : `in ${days} d`;
                        return (
                          <tr
                            key={`${it.jurisdiction}:${it.label}:${i}`}
                            className="border-b border-ink-100 hover:bg-ink-50"
                          >
                            <td className="py-2 pr-3 font-mono text-xs">
                              {it.jurisdiction}
                            </td>
                            <td className="py-2 pr-3 font-mono text-xs">
                              {it.effective_date.slice(0, 10)}
                            </td>
                            <td className="py-2 pr-3">
                              <span className={`badge ${urgencyClass(days)}`}>
                                {window}
                              </span>
                            </td>
                            <td className="py-2 pr-3 text-xs text-ink-600">
                              {it.asset_class}
                            </td>
                            <td className="py-2 pr-3">
                              <a
                                href={safeHttpUrl(it.source)}
                                target="_blank"
                                rel="noopener noreferrer"
                                className="underline hover:text-ink-900"
                              >
                                {it.label}
                              </a>
                            </td>
                          </tr>
                        );
                      })}
                  </tbody>
                </table>
              </div>
            </section>
          ))}
        </div>
      </LoadingError>
    </div>
  );
}
