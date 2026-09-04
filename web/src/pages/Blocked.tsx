import { useEffect, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchBlocked } from "../lib/api";
import { formatQ, qBgClass } from "../lib/posture";
import type { InventoryItem } from "../types/ree0xq";

export function BlockedPage() {
  const [items, setItems] = useState<InventoryItem[] | null>(null);
  const [err, setErr] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchBlocked()
      .then((r) => {
        if (!cancelled) setItems(r.items);
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
        <h1 className="text-2xl font-bold tracking-tight">BLOCKED assets</h1>
        <p className="text-sm text-ink-600 mt-1">
          Assets whose agility level is{" "}
          <code className="font-mono">locked</code> or{" "}
          <code className="font-mono">frozen</code>. These require a vendor
          program (firmware refresh, FIPS revalidation) or hardware refresh,
          and so cannot be migrated by configuration alone.
        </p>
      </div>
      <LoadingError
        loading={items === null}
        error={err}
        empty={items !== null && items.length === 0}
        emptyMessage="No BLOCKED assets observed."
      >
        <div className="card overflow-hidden">
          <table className="table min-w-full divide-y divide-ink-200">
            <thead className="bg-ink-100">
              <tr>
                <th className="px-4 py-2">Asset</th>
                <th className="px-4 py-2">Kind</th>
                <th className="px-4 py-2">Primitives</th>
                <th className="px-4 py-2">q</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-ink-200 bg-white">
              {items?.map((it) => (
                <tr key={`${it.source_module}:${it.asset_kind}:${it.identity}`}>
                  <td className="px-4 py-2">
                    <div className="font-mono text-xs">{it.identity}</div>
                    {it.host ? (
                      <div className="text-xs text-ink-600">{it.host}</div>
                    ) : null}
                  </td>
                  <td className="px-4 py-2 font-mono text-xs">
                    {it.asset_kind}
                  </td>
                  <td className="px-4 py-2">
                    <div className="flex flex-wrap gap-1">
                      {it.primitives.map((p) => (
                        <span
                          key={p}
                          className="badge bg-ink-100 text-ink-800"
                        >
                          {p}
                        </span>
                      ))}
                    </div>
                  </td>
                  <td className={`px-4 py-2 font-mono ${qBgClass(it.q)}`}>
                    {formatQ(it.q)}
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
