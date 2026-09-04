import { useMemo, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchInventory } from "../lib/api";
import { formatQ, qBgClass, qColorClass } from "../lib/posture";
import { usePolling } from "../lib/usePolling";
import type { AssetKind, InventoryItem, InventoryResponse } from "../types/ree0xq";

// Inventory is "on-demand" per SEZ-5 scope, but a 30-second
// background poll keeps the table coherent for an operator
// who's left the tab open while the stack ingests events. The
// usePolling hook suspends when the tab is hidden, so this is
// cheap.
const INVENTORY_INTERVAL_MS = 30_000;

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
  const inv = usePolling<InventoryResponse>(fetchInventory, INVENTORY_INTERVAL_MS);
  const [kind, setKind] = useState<AssetKind | "all">("all");
  const [onlyBlocked, setOnlyBlocked] = useState(false);
  const [selected, setSelected] = useState<InventoryItem | null>(null);

  const filtered = useMemo(() => {
    if (!inv.data) return [];
    return inv.data.items.filter(
      (it) =>
        (kind === "all" || it.asset_kind === kind) &&
        (!onlyBlocked || it.blocked),
    );
  }, [inv.data, kind, onlyBlocked]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Inventory</h1>
          <p className="text-sm text-ink-600 mt-1">
            Per-asset latest observation, sorted by{" "}
            <code className="font-mono">q</code> descending (most urgent first).
            Click a row for the per-asset detail panel.
          </p>
        </div>
        <button
          type="button"
          onClick={inv.refresh}
          className="text-xs px-3 py-1 border border-ink-300 rounded hover:bg-ink-100"
        >
          refresh
        </button>
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
        <span className="text-xs text-ink-600 ml-auto font-mono">
          {filtered.length}/{inv.data?.items.length ?? 0} shown
        </span>
      </div>

      <LoadingError
        loading={inv.loading}
        error={inv.error}
        empty={inv.data !== null && filtered.length === 0}
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
                <tr
                  key={`${it.source_module}:${it.asset_kind}:${it.identity}`}
                  onClick={() => setSelected(it)}
                  className="cursor-pointer hover:bg-ink-100"
                >
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

      {selected ? (
        <AssetDetail item={selected} onClose={() => setSelected(null)} />
      ) : null}
    </div>
  );
}

function AssetDetail({
  item,
  onClose,
}: {
  item: InventoryItem;
  onClose: () => void;
}) {
  return (
    <div
      className="fixed inset-0 bg-ink-900/40 flex items-end md:items-center justify-center p-4 z-50"
      onClick={onClose}
    >
      <div
        className="bg-white rounded-lg shadow-xl max-w-2xl w-full p-6 space-y-4"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="text-xs uppercase tracking-wide text-ink-600">
              Asset detail
            </div>
            <h2 className="font-mono text-lg font-bold break-all">
              {item.identity}
            </h2>
            {item.host ? (
              <div className="text-sm text-ink-600 font-mono">{item.host}</div>
            ) : null}
          </div>
          <button
            type="button"
            onClick={onClose}
            className="text-ink-600 hover:text-ink-900 text-xl leading-none"
            aria-label="Close"
          >
            ×
          </button>
        </div>

        <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
          <Stat label="kind" value={item.asset_kind} mono />
          <Stat label="source" value={item.source_module} mono />
          <Stat
            label="q"
            value={formatQ(item.q)}
            mono
            className={qColorClass(item.q)}
          />
          <Stat
            label="blocked"
            value={item.blocked ? "yes" : "no"}
            className={item.blocked ? "text-posture-critical" : ""}
          />
        </div>

        <div>
          <div className="text-xs uppercase tracking-wide text-ink-600 mb-1">
            primitives
          </div>
          {item.primitives.length === 0 ? (
            <div className="text-sm text-ink-600">none observed</div>
          ) : (
            <div className="flex flex-wrap gap-1">
              {item.primitives.map((p) => (
                <span key={p} className="badge bg-ink-100 text-ink-800">
                  {p}
                </span>
              ))}
            </div>
          )}
        </div>

        <div>
          <div className="text-xs uppercase tracking-wide text-ink-600 mb-1">
            observed at
          </div>
          <div className="font-mono text-sm">{item.observed_at}</div>
        </div>

        <div className="text-xs text-ink-600 border-t pt-3">
          For the full event JSON, hit{" "}
          <code className="font-mono">
            GET /v1/events?limit=N
          </code>{" "}
          and filter on{" "}
          <code className="font-mono">asset.identity</code>.
        </div>
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  mono,
  className,
}: {
  label: string;
  value: string;
  mono?: boolean;
  className?: string;
}) {
  return (
    <div>
      <div className="text-xs uppercase tracking-wide text-ink-600">
        {label}
      </div>
      <div
        className={`text-sm ${mono ? "font-mono" : ""} ${className ?? ""}`.trim()}
      >
        {value}
      </div>
    </div>
  );
}
