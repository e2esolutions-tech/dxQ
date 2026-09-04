import { useEffect, useState } from "react";

import { LoadingError } from "../components/LoadingError";
import { fetchInventory, postRoadmap } from "../lib/api";
import type { Milestone, RoadmapProjection } from "../types/ree0xq";

// One operator-editable milestone in the local form state.
interface DraftMilestone {
  label: string;
  date: string;
  asset_ids: string;
  target_primitives: string;
}

function toMilestone(d: DraftMilestone): Milestone {
  return {
    label: d.label,
    date: new Date(`${d.date}T00:00:00Z`).toISOString(),
    asset_ids: d.asset_ids
      .split(/[\n,]/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0),
    target_primitives: d.target_primitives
      .split(/[\n,]/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0),
  };
}

function emptyDraft(year: number): DraftMilestone {
  return {
    label: `Q1-${year}-pq-cut`,
    date: `${year}-03-31`,
    asset_ids: "",
    target_primitives: "ML-DSA-65, ML-KEM-768",
  };
}

export function RoadmapPage() {
  const [drafts, setDrafts] = useState<DraftMilestone[]>([
    emptyDraft(new Date().getUTCFullYear() + 1),
  ]);
  const [knownIds, setKnownIds] = useState<string[]>([]);
  const [projection, setProjection] = useState<RoadmapProjection | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<boolean>(false);

  // Surface real asset identities so the operator can paste them
  // into a milestone instead of typing from memory.
  useEffect(() => {
    let cancelled = false;
    fetchInventory()
      .then((r) => {
        if (cancelled) return;
        setKnownIds(r.items.map((it) => it.identity).sort());
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  function updateDraft(i: number, patch: Partial<DraftMilestone>) {
    setDrafts((prev) => prev.map((d, j) => (j === i ? { ...d, ...patch } : d)));
  }

  function addMilestone() {
    const year =
      drafts.length > 0
        ? new Date(drafts[drafts.length - 1].date).getUTCFullYear() + 1
        : new Date().getUTCFullYear() + 1;
    setDrafts((prev) => [...prev, emptyDraft(year)]);
  }

  function removeMilestone(i: number) {
    setDrafts((prev) => prev.filter((_, j) => j !== i));
  }

  async function project() {
    setBusy(true);
    setError(null);
    try {
      const milestones = drafts
        .map(toMilestone)
        .filter((m) => m.asset_ids.length > 0);
      if (milestones.length === 0) {
        setError("Add at least one milestone with one asset id.");
        return;
      }
      const r = await postRoadmap(milestones);
      setProjection(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">
          Migration roadmap projector
        </h1>
        <p className="text-sm text-ink-600 mt-1">
          Draft milestones below, project them against the live inventory, and
          read the per-milestone effect on org <code>q</code>.
        </p>
      </div>

      <div className="space-y-4">
        {drafts.map((d, i) => (
          <div key={i} className="card p-4 space-y-2">
            <div className="flex items-center justify-between">
              <h2 className="font-semibold text-sm">Milestone {i + 1}</h2>
              {drafts.length > 1 && (
                <button
                  type="button"
                  onClick={() => removeMilestone(i)}
                  className="text-xs text-posture-urgent hover:underline"
                >
                  remove
                </button>
              )}
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <label className="text-sm">
                <span className="text-ink-600 block mb-1">Label</span>
                <input
                  type="text"
                  value={d.label}
                  onChange={(e) =>
                    updateDraft(i, { label: e.target.value })
                  }
                  className="w-full border border-ink-200 rounded px-2 py-1 font-mono text-sm"
                />
              </label>
              <label className="text-sm">
                <span className="text-ink-600 block mb-1">Date</span>
                <input
                  type="date"
                  value={d.date}
                  onChange={(e) =>
                    updateDraft(i, { date: e.target.value })
                  }
                  className="border border-ink-200 rounded px-2 py-1 font-mono text-sm"
                />
              </label>
              <label className="text-sm md:col-span-2">
                <span className="text-ink-600 block mb-1">
                  Asset identities (comma or newline separated)
                </span>
                <textarea
                  value={d.asset_ids}
                  onChange={(e) =>
                    updateDraft(i, { asset_ids: e.target.value })
                  }
                  rows={3}
                  placeholder="paste asset identities here"
                  className="w-full border border-ink-200 rounded px-2 py-1 font-mono text-xs"
                />
              </label>
              <label className="text-sm md:col-span-2">
                <span className="text-ink-600 block mb-1">
                  Target primitives (comma separated)
                </span>
                <input
                  type="text"
                  value={d.target_primitives}
                  onChange={(e) =>
                    updateDraft(i, { target_primitives: e.target.value })
                  }
                  className="w-full border border-ink-200 rounded px-2 py-1 font-mono text-sm"
                />
              </label>
            </div>
          </div>
        ))}

        <div className="flex flex-wrap gap-3">
          <button
            type="button"
            onClick={addMilestone}
            className="text-sm px-3 py-1 border border-ink-300 rounded hover:bg-ink-100"
          >
            + add milestone
          </button>
          <button
            type="button"
            onClick={project}
            disabled={busy}
            className="text-sm px-3 py-1 bg-ink-900 text-white rounded hover:bg-ink-800 disabled:opacity-50"
          >
            {busy ? "projecting…" : "project against inventory"}
          </button>
          <span className="text-xs text-ink-600 self-center font-mono">
            {knownIds.length} live asset id(s) available
          </span>
        </div>
      </div>

      {error && (
        <div className="card border border-posture-urgent/40 bg-posture-urgent/10 p-3 text-sm text-posture-urgent">
          {error}
        </div>
      )}

      {projection && (
        <LoadingError loading={false} error={null} empty={false}>
          <div className="space-y-4">
            <div className="card p-4">
              <h2 className="font-semibold mb-2">Today</h2>
              <div className="flex gap-6 text-sm">
                <span>
                  <span className="text-ink-600">org q:</span>{" "}
                  <span className="font-mono">
                    {projection.today_org_q.toFixed(3)}
                  </span>
                </span>
                <span>
                  <span className="text-ink-600">blocked:</span>{" "}
                  <span className="font-mono">{projection.today_blocked}</span>
                </span>
                <span>
                  <span className="text-ink-600">assets:</span>{" "}
                  <span className="font-mono">{projection.total_assets}</span>
                </span>
              </div>
            </div>

            <div className="card p-4">
              <h2 className="font-semibold mb-2">Projection</h2>
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-xs text-ink-600 text-left border-b border-ink-200">
                      <th className="py-2 pr-3">Milestone</th>
                      <th className="py-2 pr-3">Date</th>
                      <th className="py-2 pr-3">q before</th>
                      <th className="py-2 pr-3">q after</th>
                      <th className="py-2 pr-3">Δq</th>
                      <th className="py-2 pr-3">Migrated</th>
                      <th className="py-2 pr-3">Classical left</th>
                    </tr>
                  </thead>
                  <tbody>
                    {projection.projections.map((p, i) => {
                      const delta = p.org_q_after - p.org_q_before;
                      return (
                        <tr
                          key={`${p.milestone}:${i}`}
                          className="border-b border-ink-100"
                        >
                          <td className="py-2 pr-3 font-mono">{p.milestone}</td>
                          <td className="py-2 pr-3 font-mono">
                            {p.date.slice(0, 10)}
                          </td>
                          <td className="py-2 pr-3 font-mono">
                            {p.org_q_before.toFixed(3)}
                          </td>
                          <td className="py-2 pr-3 font-mono">
                            {p.org_q_after.toFixed(3)}
                          </td>
                          <td
                            className={`py-2 pr-3 font-mono ${
                              delta < 0 ? "text-posture-good" : "text-ink-600"
                            }`}
                          >
                            {delta.toFixed(3)}
                          </td>
                          <td className="py-2 pr-3 font-mono">
                            {p.assets_migrated}
                          </td>
                          <td className="py-2 pr-3 font-mono">
                            {p.assets_remaining_classical}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          </div>
        </LoadingError>
      )}
    </div>
  );
}
