// Thin fetch wrapper for the ree0xq-server REST API.
//
// All calls are relative to the current origin so Vite's dev-server
// proxy (see vite.config.ts) routes them to 127.0.0.1:8090. In
// production the dashboard is typically served by ree0xq-server itself,
// keeping URLs the same.

import type {
  CompatResponse,
  DeadlinesResponse,
  EventsResponse,
  InventoryResponse,
  Milestone,
  OrgPosture,
  QkdLinksResponse,
  RecommendationsResponse,
  RoadmapProjection,
} from "../types/ree0xq";

async function getJson<T>(path: string): Promise<T> {
  const r = await fetch(path, {
    method: "GET",
    headers: { Accept: "application/json" },
  });
  if (!r.ok) {
    throw new Error(`GET ${path} failed: ${r.status} ${r.statusText}`);
  }
  return (await r.json()) as T;
}

export async function fetchInventory(): Promise<InventoryResponse> {
  return getJson<InventoryResponse>("/v1/inventory");
}

export async function fetchBlocked(): Promise<InventoryResponse> {
  return getJson<InventoryResponse>("/v1/blocked");
}

export async function fetchPosture(): Promise<OrgPosture> {
  return getJson<OrgPosture>("/v1/posture");
}

export async function fetchQkdLinks(): Promise<QkdLinksResponse> {
  return getJson<QkdLinksResponse>("/v1/qkd/links");
}

export async function fetchEvents(limit = 100): Promise<EventsResponse> {
  return getJson<EventsResponse>(`/v1/events?limit=${limit}`);
}

export async function fetchRecommendations(): Promise<RecommendationsResponse> {
  return getJson<RecommendationsResponse>("/v1/recommendations");
}

export async function fetchDeadlines(opts?: {
  jurisdiction?: string;
  horizonDays?: number;
}): Promise<DeadlinesResponse> {
  const qs = new URLSearchParams();
  if (opts?.jurisdiction) qs.set("jurisdiction", opts.jurisdiction);
  if (opts?.horizonDays != null)
    qs.set("horizon_days", String(opts.horizonDays));
  const path =
    qs.toString().length > 0
      ? `/v1/agility/deadlines?${qs}`
      : "/v1/agility/deadlines";
  return getJson<DeadlinesResponse>(path);
}

export async function fetchCompat(opts?: {
  stack?: string;
  algorithm?: string;
}): Promise<CompatResponse> {
  const qs = new URLSearchParams();
  if (opts?.stack) qs.set("stack", opts.stack);
  if (opts?.algorithm) qs.set("algorithm", opts.algorithm);
  const path =
    qs.toString().length > 0
      ? `/v1/agility/compat?${qs}`
      : "/v1/agility/compat";
  return getJson<CompatResponse>(path);
}

export async function postRoadmap(
  milestones: Milestone[],
): Promise<RoadmapProjection> {
  const r = await fetch("/v1/agility/roadmap", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Accept: "application/json",
    },
    body: JSON.stringify({ milestones }),
  });
  if (!r.ok) {
    throw new Error(
      `POST /v1/agility/roadmap failed: ${r.status} ${r.statusText}`,
    );
  }
  return (await r.json()) as RoadmapProjection;
}
