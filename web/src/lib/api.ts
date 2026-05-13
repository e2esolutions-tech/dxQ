// Thin fetch wrapper for the sezar-server REST API.
//
// All calls are relative to the current origin so Vite's dev-server
// proxy (see vite.config.ts) routes them to 127.0.0.1:8090. In
// production the dashboard is typically served by sezar-server itself,
// keeping URLs the same.

import type {
  EventsResponse,
  InventoryResponse,
  OrgPosture,
  QkdLinksResponse,
} from "../types/sezar";

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
