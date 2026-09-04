import { useMemo, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchRecommendations } from "../lib/api";
import { usePolling } from "../lib/usePolling";
import type { AssetKind, RecommendationCost } from "../types/ree0xq";

// 30 seconds matches the Inventory poll cadence — the
// recommendations roll up from the same per-asset latest
// map so a shorter interval would only add load without
// adding signal.
const RECS_INTERVAL_MS = 30_000;

const ALL_KINDS: (AssetKind | "all")[] = [
  "all",
  "tls_session",
  "x509_cert",
  "blockchain_key",
  "hsm_slot",
  "qkd_link",
  "qkd_kme",
];

const COST_RANK: Record<RecommendationCost, number> = {
  trivial: 0,
  low: 1,
  medium: 2,
  high: 3,
};

function costClass(c: RecommendationCost): string {
  switch (c) {
    case "trivial":
    case "low":
      return "bg-posture-good/15 text-posture-good";
    case "medium":
      return "bg-posture-plan/15 text-posture-plan";
    case "high":
      return "bg-posture-urgent/15 text-posture-urgent";
  }
}

export function RecommendationsPage() {
  const recs = usePolling(fetchRecommendations, RECS_INTERVAL_MS);
  const [kind, setKind] = useState<AssetKind | "all">("all");
  const [maxCost, setMaxCost] = useState<RecommendationCost | "all">("all");

  const filtered = useMemo(() => {
    if (!recs.data) return [];
    return recs.data.items.filter((it) => {
      if (kind !== "all" && it.asset_kind !== kind) return false;
      if (maxCost === "all") return true;
      const limit = COST_RANK[maxCost];
      return it.recommendations.some((r) => COST_RANK[r.cost] <= limit);
    });
  }, [recs.data, kind, maxCost]);

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">
            PQ migration recommendations
          </h1>
          <p className="text-sm text-ink-600 mt-1">
            Per-asset replacement options, ranked cheapest first. Drawn from
            the V5 recommendation engine over <code className="font-mono">/v1/inventory</code>.
          </p>
        </div>
        <button
          type="button"
          onClick={recs.refresh}
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
        <label className="text-sm">
          <span className="mr-2 text-ink-600">Max cost:</span>
          <select
            value={maxCost}
            onChange={(e) =>
              setMaxCost(e.target.value as RecommendationCost | "all")
            }
            className="border border-ink-200 rounded px-2 py-1 bg-white text-sm"
          >
            <option value="all">all</option>
            <option value="trivial">trivial</option>
            <option value="low">low</option>
            <option value="medium">medium</option>
            <option value="high">high</option>
          </select>
        </label>
        <span className="text-xs text-ink-600 ml-auto font-mono">
          {filtered.length}/{recs.data?.items.length ?? 0} shown
        </span>
      </div>

      <LoadingError
        loading={recs.loading}
        error={recs.error}
        empty={recs.data !== null && filtered.length === 0}
        emptyMessage={
          recs.data?.count === 0
            ? "No assets need replacement — every observed primitive is already PQ-safe."
            : "No recommendations match the current filter."
        }
      >
        <div className="space-y-3">
          {filtered.map((it) => (
            <div
              key={`${it.source_module}:${it.asset_kind}:${it.identity}`}
              className="card p-4"
            >
              <div className="flex flex-wrap items-baseline gap-2 mb-2">
                <span className="font-mono text-sm break-all">{it.identity}</span>
                <span className="badge bg-ink-100 text-ink-800">
                  {it.asset_kind}
                </span>
                <span className="text-xs text-ink-600 font-mono">
                  {it.source_module}
                </span>
              </div>
              <div className="text-xs text-ink-600 mb-3">
                current:{" "}
                {it.current_primitives.map((p) => (
                  <span
                    key={p}
                    className="inline-block mr-1 mb-1 badge bg-ink-100 text-ink-800"
                  >
                    {p}
                  </span>
                ))}
              </div>
              <div className="space-y-2">
                {it.recommendations.map((r, i) => (
                  <div
                    key={`${r.replaces}:${r.replacement}:${i}`}
                    className="border-l-2 border-ink-200 pl-3"
                  >
                    <div className="flex flex-wrap items-baseline gap-2">
                      <span className="text-sm">
                        <span className="text-ink-600">{r.replaces}</span>
                        <span className="mx-1 text-ink-400">→</span>
                        <span className="font-mono font-semibold">
                          {r.replacement}
                        </span>
                      </span>
                      <span
                        className={`badge ${costClass(r.cost)}`}
                        title={`migration cost: ${r.cost}`}
                      >
                        {r.cost}
                      </span>
                    </div>
                    <div className="text-xs text-ink-600 mt-1">
                      {r.rationale}
                    </div>
                    {r.caveats.length > 0 && (
                      <ul className="mt-1 text-xs text-ink-600 list-disc list-inside">
                        {r.caveats.map((c, idx) => (
                          <li key={idx}>{c}</li>
                        ))}
                      </ul>
                    )}
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </LoadingError>
    </div>
  );
}
