import { useMemo } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchInventory, fetchPosture } from "../lib/api";
import { deadlineLabel, formatQ, qBgClass, qColorClass } from "../lib/posture";
import { usePolling } from "../lib/usePolling";
import type { AssetKind, InventoryItem, OrgPosture } from "../types/sezar";

// 10 seconds matches the SEZ-5 scope; loud enough for operators
// to feel the dashboard "live" without putting noticeable load
// on the collector under default deployments.
const POSTURE_INTERVAL_MS = 10_000;
// The inventory roll-up shares the posture tab — same cadence
// is fine since the in-memory store's GET is constant-time.
const INVENTORY_INTERVAL_MS = 10_000;

interface Breakdown {
  kind: AssetKind;
  count: number;
  blocked: number;
  // Mean q across the assets in this kind. Cheap aggregate that
  // the in-memory store doesn't have to compute server-side.
  mean_q: number;
  max_q: number;
}

function breakdownByKind(items: InventoryItem[]): Breakdown[] {
  const acc = new Map<AssetKind, { qs: number[]; blocked: number }>();
  for (const it of items) {
    const slot = acc.get(it.asset_kind) ?? { qs: [], blocked: 0 };
    slot.qs.push(it.q);
    if (it.blocked) slot.blocked += 1;
    acc.set(it.asset_kind, slot);
  }
  const out: Breakdown[] = [];
  for (const [kind, { qs, blocked }] of acc) {
    const sum = qs.reduce((a, b) => a + b, 0);
    out.push({
      kind,
      count: qs.length,
      blocked,
      mean_q: sum / qs.length,
      max_q: Math.max(...qs),
    });
  }
  // Sort by mean q descending so the most-urgent kind is at the
  // top of the breakdown — same ordering convention as the
  // inventory table.
  out.sort((a, b) => b.mean_q - a.mean_q);
  return out;
}

export function PosturePage() {
  const posture = usePolling<OrgPosture>(fetchPosture, POSTURE_INTERVAL_MS);
  const inventory = usePolling(
    async () => (await fetchInventory()).items,
    INVENTORY_INTERVAL_MS,
  );

  const breakdown = useMemo(
    () => (inventory.data ? breakdownByKind(inventory.data) : []),
    [inventory.data],
  );

  const isEmpty = !!posture.data && posture.data.assets === 0;

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            Org-level posture
          </h1>
          <p className="text-sm text-ink-600 mt-1">
            Deadline-adjusted quantum-risk score{" "}
            <code className="font-mono">q</code>, weighted across asset kinds.
            See <code className="font-mono">/v1/posture</code>.
          </p>
        </div>
        <div className="text-xs text-ink-600 font-mono">
          poll every {POSTURE_INTERVAL_MS / 1000}s
        </div>
      </div>

      <LoadingError loading={posture.loading} error={posture.error}>
        {posture.data && isEmpty ? (
          <EmptyStateCta />
        ) : posture.data ? (
          <>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="card p-6 col-span-1 md:col-span-2">
                <div className="text-xs uppercase tracking-wide text-ink-600">
                  Org-level q
                </div>
                <div
                  className={`mt-1 font-mono text-5xl font-bold ${qBgClass(
                    posture.data.org_q,
                  )} inline-block px-3 py-1 rounded`}
                >
                  {formatQ(posture.data.org_q)}
                </div>
                <div className="mt-4 text-sm text-ink-600">
                  Computed under default weights (α=0.5, β=0.2, γ_max=0.3) with
                  horizon = {posture.data.horizon_years} years.
                </div>
                <div className="mt-2 text-sm">
                  Deadline:{" "}
                  <span className="font-mono">
                    {new Date(posture.data.deadline).toISOString().slice(0, 10)}
                  </span>{" "}
                  <span className="text-ink-600">
                    ({deadlineLabel(posture.data.deadline)})
                  </span>
                </div>
              </div>
              <div className="card p-6 space-y-4">
                <div>
                  <div className="text-xs uppercase tracking-wide text-ink-600">
                    Total assets
                  </div>
                  <div className="text-3xl font-bold">
                    {posture.data.assets}
                  </div>
                </div>
                <div>
                  <div className="text-xs uppercase tracking-wide text-ink-600">
                    BLOCKED count
                  </div>
                  <div className="text-3xl font-bold text-posture-critical">
                    {posture.data.blocked_count}
                  </div>
                  <div className="text-xs text-ink-600 mt-1">
                    Agility ≤ Locked. Independent of q; surface action items
                    that need vendor or hardware programs.
                  </div>
                </div>
              </div>
            </div>

            <KindBreakdown
              breakdown={breakdown}
              loading={inventory.loading}
              error={inventory.error}
            />
          </>
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

function KindBreakdown({
  breakdown,
  loading,
  error,
}: {
  breakdown: Breakdown[];
  loading: boolean;
  error: string | null;
}) {
  return (
    <div className="card p-6">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold">Breakdown by asset kind</h2>
        <span className="text-xs text-ink-600 font-mono">
          mean q + max q per kind
        </span>
      </div>
      <LoadingError
        loading={loading}
        error={error}
        empty={breakdown.length === 0}
        emptyMessage="No assets reporting yet."
      >
        <div className="space-y-3">
          {breakdown.map((row) => (
            <div key={row.kind} className="space-y-1">
              <div className="flex items-center justify-between text-sm">
                <div className="flex items-center gap-3">
                  <span className="font-mono">{row.kind}</span>
                  <span className="text-xs text-ink-600">
                    n = {row.count}
                    {row.blocked > 0 ? (
                      <span className="text-posture-critical ml-2">
                        ({row.blocked} BLOCKED)
                      </span>
                    ) : null}
                  </span>
                </div>
                <div className="font-mono text-xs">
                  <span className={qColorClass(row.mean_q)}>
                    {formatQ(row.mean_q)}
                  </span>
                  <span className="text-ink-600"> mean · </span>
                  <span className={qColorClass(row.max_q)}>
                    {formatQ(row.max_q)}
                  </span>
                  <span className="text-ink-600"> max</span>
                </div>
              </div>
              <div className="h-2 bg-ink-100 rounded overflow-hidden flex">
                <div
                  className={`h-full ${qBgClass(row.mean_q).split(" ")[0]}`}
                  style={{ width: `${Math.min(100, row.mean_q * 100)}%` }}
                />
              </div>
            </div>
          ))}
        </div>
      </LoadingError>
    </div>
  );
}

function EmptyStateCta() {
  const cmd = "docker compose up -d";
  const cmd2 =
    "curl -sS http://127.0.0.1:8090/v1/admin/bootstrap-tokens \\\n" +
    "  -H 'X-Admin-Token: $SEZAR_ADMIN_TOKEN' \\\n" +
    "  -d '{\"agent_id\":\"sezar-net-01\"}'";
  return (
    <div className="card p-8 space-y-4">
      <div className="space-y-1">
        <h2 className="text-lg font-semibold">No agents reporting yet</h2>
        <p className="text-sm text-ink-600">
          Sezar's collector is up but no asset events have arrived. Start the
          stack and bootstrap an agent against it:
        </p>
      </div>
      <div className="space-y-3">
        <CmdBlock label="1. Run the collector" cmd={cmd} />
        <CmdBlock
          label="2. Issue a one-time bootstrap token"
          cmd={cmd2}
          multiline
        />
        <CmdBlock
          label="3. Drive a probe and POST events"
          cmd="sezar-net live --pcap fixture.pcap --collector http://127.0.0.1:8090/v1/events"
        />
      </div>
      <p className="text-xs text-ink-600">
        The dashboard polls <code className="font-mono">/v1/posture</code> every
        10s and will refresh automatically when events start arriving.
      </p>
    </div>
  );
}

function CmdBlock({
  label,
  cmd,
  multiline,
}: {
  label: string;
  cmd: string;
  multiline?: boolean;
}) {
  return (
    <div>
      <div className="text-xs uppercase tracking-wide text-ink-600 mb-1">
        {label}
      </div>
      <div className="flex items-start gap-2">
        <pre
          className={`flex-1 bg-ink-900 text-white font-mono text-xs p-3 rounded overflow-x-auto ${
            multiline ? "whitespace-pre" : "whitespace-pre-wrap"
          }`}
        >
          {cmd}
        </pre>
        <button
          type="button"
          onClick={() => void navigator.clipboard?.writeText(cmd)}
          className="text-xs px-2 py-1 border border-ink-300 rounded hover:bg-ink-100"
          title="Copy to clipboard"
        >
          copy
        </button>
      </div>
    </div>
  );
}
