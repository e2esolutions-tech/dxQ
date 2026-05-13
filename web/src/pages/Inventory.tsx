import { useEffect, useMemo, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchInventory } from "../lib/api";
import { formatQ, qBgClass } from "../lib/posture";
import type { AssetKind, InventoryItem } from "../types/sezar";

const ALL_KINDS: (AssetKind | "all")[] = [
  "all",
  "tls_session",
  "ssh_session",
  "ipsec_sa",
  "x509_cert",
  "blockchain_key",
  "hsm_slot",
  "dns_dnssec",
  "qkd_link",
  "qkd_kme",
];

export function InventoryPage() {
  const [items, setItems] = useState<InventoryItem[] | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [kind, setKind] = useState<AssetKind | "all">("all");
  const [onlyBlocked, setOnlyBlocked] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetchInventory()
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

  const filtered = useMemo(() => {
    if (!items) return [];
    return items.filter(
      (it) =>
        (kind === "all" || it.asset_kind === kind) &&
        (!onlyBlocked || it.blocked),
    );
  }, [items, kind, onlyBlocked]);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Inventory</h1>
        <p className="text-sm text-ink-600 mt-1">
          Per-asset latest observation, sorted by{" "}
          <code className="font-mono">q</code> descending (most urgent first).
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-4">
        <label className="text-sm">
          <span className="mr-2 text-ink-600">Kind:</span>
          <select
            value={kind}
            onChange={(e) => setKind(e.target.value as AssetKind | "all")}
            className="border border-ink-200 rounded px-2 py-1 bg-white text-sm"
          >
            {ALL_KINDS.map((k) => (
              <option key={k} value={k}>
                {k}
              </option>
            ))}
          </select>
        </label>
        <label className="text-sm flex items-center gap-2">
          <input
            type="checkbox"
            checked={onlyBlocked}
            onChange={(e) => setOnlyBlocked(e.target.checked)}
          />
          BLOCKED only
        </label>
      </div>

      <LoadingError
        loading={items === null}
        error={err}
        empty={items !== null && filtered.length === 0}
        emptyMessage="No assets match the current filter."
      >
        <div className="card overflow-hidden">
          <table className="table min-w-full divide-y divide-ink-200">
            <thead className="bg-ink-100">
              <tr>
                <th className="px-4 py-2">Asset</th>
                <th className="px-4 py-2">Kind</th>
                <th className="px-4 py-2">Source</th>
                <th className="px-4 py-2">Primitives</th>
                <th className="px-4 py-2">q</th>
                <th className="px-4 py-2">Flag</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-ink-200 bg-white">
              {filtered.map((it) => (
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
                  <td className="px-4 py-2 font-mono text-xs text-ink-600">
                    {it.source_module}
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
                  <td className="px-4 py-2">
                    {it.blocked ? (
                      <span className="badge bg-posture-blocked text-white">
                        BLOCKED
                      </span>
                    ) : (
                      <span className="text-xs text-ink-400">—</span>
                    )}
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
