import { useEffect, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchQkdLinks } from "../lib/api";
import type { QkdLinkSummary } from "../types/sezar";

function healthClass(h: string): string {
  switch (h) {
    case "ok":
      return "bg-posture-good/15 text-posture-good";
    case "degraded":
      return "bg-posture-plan/15 text-posture-plan";
    case "failed":
      return "bg-posture-critical/20 text-posture-critical";
    default:
      return "bg-ink-100 text-ink-600";
  }
}

export function QkdPage() {
  const [items, setItems] = useState<QkdLinkSummary[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchQkdLinks()
      .then((r) => {
        if (!cancelled) setItems(r.links);
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
        <h1 className="text-2xl font-bold tracking-tight">QKD links</h1>
        <p className="text-sm text-ink-600 mt-1">
          ETSI GS QKD 014 KME observations collected by{" "}
          <code className="font-mono">sezar-qkd</code>.
        </p>
      </div>
      <LoadingError
        loading={items === null}
        error={err}
        empty={items !== null && items.length === 0}
        emptyMessage={
          "No QKD observations yet. Start the emulator and the collector — see web/README.md."
        }
      >
        <div className="card overflow-hidden">
          <table className="table min-w-full divide-y divide-ink-200">
            <thead className="bg-ink-100">
              <tr>
                <th className="px-4 py-2">KME / Link</th>
                <th className="px-4 py-2">Endpoint</th>
                <th className="px-4 py-2">Health</th>
                <th className="px-4 py-2">QBER</th>
                <th className="px-4 py-2">Key rate</th>
                <th className="px-4 py-2">Observed</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-ink-200 bg-white">
              {items?.map((it) => (
                <tr key={it.identity}>
                  <td className="px-4 py-2 font-mono text-xs">{it.identity}</td>
                  <td className="px-4 py-2 font-mono text-xs text-ink-600">
                    {it.kme_endpoint ?? "—"}
                  </td>
                  <td className="px-4 py-2">
                    <span className={`badge ${healthClass(it.link_health)}`}>
                      {it.link_health}
                    </span>
                  </td>
                  <td className="px-4 py-2 font-mono">
                    {it.link_qber == null
                      ? "—"
                      : `${(it.link_qber * 100).toFixed(2)}%`}
                  </td>
                  <td className="px-4 py-2 font-mono">
                    {it.link_key_rate_bps == null
                      ? "—"
                      : `${it.link_key_rate_bps.toLocaleString()} bps`}
                  </td>
                  <td className="px-4 py-2 font-mono text-xs text-ink-600">
                    {new Date(it.observed_at).toLocaleString()}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </LoadingError>
    </div>
  );
}
