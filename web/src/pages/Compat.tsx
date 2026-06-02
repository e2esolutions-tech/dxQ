import { useMemo, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchCompat } from "../lib/api";
import { usePolling } from "../lib/usePolling";
import { safeHttpUrl } from "../lib/safeUrl";
import type { SupportStatus } from "../types/sezar";

const COMPAT_INTERVAL_MS = 5 * 60_000;

function statusClass(s: SupportStatus): string {
  switch (s) {
    case "supported":
      return "bg-posture-good/15 text-posture-good";
    case "experimental":
      return "bg-posture-plan/15 text-posture-plan";
    case "not-implemented":
      return "bg-posture-urgent/15 text-posture-urgent";
    case "unknown":
      return "bg-ink-100 text-ink-600";
  }
}

const STACKS = [
  "all",
  "openssl-3.x",
  "boringssl",
  "rustls-post-quantum",
  "go-crypto-tls",
  "bouncycastle",
  "nss",
];

export function CompatPage() {
  const [stack, setStack] = useState<string>("all");

  const compat = usePolling(
    () => fetchCompat(stack === "all" ? undefined : { stack }),
    COMPAT_INTERVAL_MS,
    [stack],
  );

  const items = compat.data?.items ?? [];

  // Pivot to a stack × algorithm matrix so the operator can read
  // it left-to-right as a compatibility grid rather than a flat
  // list.
  const grid = useMemo(() => {
    const algos = Array.from(new Set(items.map((it) => it.algorithm))).sort();
    const stacks = Array.from(new Set(items.map((it) => it.stack))).sort();
    const cells = new Map<string, (typeof items)[number]>();
    for (const it of items) cells.set(`${it.stack}|${it.algorithm}`, it);
    return { algos, stacks, cells };
  }, [items]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            TLS-stack compatibility matrix
          </h1>
          <p className="text-sm text-ink-600 mt-1">
            Which TLS stacks support which PQ algorithms today. Click a cell to
            open the upstream source.
          </p>
        </div>
        <button
          type="button"
          onClick={compat.refresh}
          className="text-xs px-3 py-1 border border-ink-300 rounded hover:bg-ink-100"
        >
          refresh
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-4">
        <label className="text-sm">
          <span className="mr-2 text-ink-600">Stack:</span>
          <select
            value={stack}
            onChange={(e) => setStack(e.target.value)}
            className="border border-ink-200 rounded px-2 py-1 bg-white text-sm"
          >
            {STACKS.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </label>
        <span className="text-xs text-ink-600 ml-auto font-mono">
          {items.length} entries
        </span>
      </div>

      <LoadingError
        loading={compat.loading}
        error={compat.error}
        empty={compat.data !== null && items.length === 0}
        emptyMessage="No matrix entries match the filter."
      >
        <div className="overflow-x-auto">
          <table className="w-full text-sm border-collapse">
            <thead>
              <tr className="text-xs text-ink-600 text-left border-b border-ink-200">
                <th className="py-2 pr-3 sticky left-0 bg-white">Stack</th>
                {grid.algos.map((a) => (
                  <th key={a} className="py-2 pr-3 font-mono whitespace-nowrap">
                    {a}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {grid.stacks.map((s) => (
                <tr key={s} className="border-b border-ink-100 hover:bg-ink-50">
                  <td className="py-2 pr-3 font-mono text-xs sticky left-0 bg-white">
                    {s}
                  </td>
                  {grid.algos.map((a) => {
                    const cell = grid.cells.get(`${s}|${a}`);
                    if (!cell)
                      return (
                        <td key={a} className="py-2 pr-3 text-ink-400 text-xs">
                          —
                        </td>
                      );
                    const content = (
                      <span
                        className={`badge ${statusClass(cell.status)}`}
                        title={
                          cell.min_version
                            ? `min ${cell.min_version}`
                            : undefined
                        }
                      >
                        {cell.status}
                        {cell.min_version ? ` · ${cell.min_version}` : ""}
                      </span>
                    );
                    return (
                      <td key={a} className="py-2 pr-3">
                        {safeHttpUrl(cell.source) ? (
                          <a
                            href={safeHttpUrl(cell.source)}
                            target="_blank"
                            rel="noopener noreferrer"
                            className="underline hover:text-ink-900"
                          >
                            {content}
                          </a>
                        ) : (
                          content
                        )}
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </LoadingError>
    </div>
  );
}
