// ree0xQ wire types — manually aligned with ree0xq-core's
// `CryptoInventoryEvent` schema. Once the workspace turns on the
// `ts-types` feature on ree0xq-core, `ts-rs` will replace this file
// with generated source. Until then, treat this as the canonical
// TS view of the schema.

export type AssetKind =
  | "tls_session"
  | "ssh_session"
  | "ipsec_sa"
  | "x509_cert"
  | "blockchain_key"
  | "hsm_slot"
  | "dns_dnssec"
  | "qkd_link"
  | "qkd_kme";

export type PrimitiveRole = "kex" | "sig" | "auth" | "encrypt" | "hash";
export type NistLevel = "L1" | "L3" | "L5";

export interface Primitive {
  role: PrimitiveRole;
  algorithm: string;
  parameters?: Record<string, unknown>;
  pq_resistant?: boolean | null;
  nist_classification?: NistLevel | null;
}

export type ChannelState = "classical" | "qkd_hybrid_psk" | "qkd_only";
export type LinkHealth = "ok" | "degraded" | "failed";

export interface ChannelProtection {
  state: ChannelState;
  kme_endpoint?: string | null;
  key_id_observed?: string | null;
  psk_age_seconds?: number | null;
  link_qber?: number | null;
  link_key_rate_bps?: number | null;
  link_health: LinkHealth;
  degraded_reason?: string | null;
}

export type AgilityLevel =
  | "negotiated"
  | "configurable"
  | "pinned"
  | "locked"
  | "frozen";

export interface AgilityBlock {
  level: AgilityLevel;
  level_score: number;
  evidence: unknown[];
  scanner_version: string;
  rubric_version: string;
}

export interface Asset {
  kind: AssetKind;
  identity: string;
  host?: string | null;
}

export interface CryptoInventoryEvent {
  schema_version: number;
  schema_minor: number;
  source_module: string;
  observed_at: string;
  asset: Asset;
  primitives: Primitive[];
  channel_protection?: ChannelProtection | null;
  agility?: AgilityBlock | null;
  posture: {
    score: number;
    rationale: string;
    recommended_replacement?: string | null;
  };
}

// ----- server response shapes -----

export interface InventoryItem {
  source_module: string;
  asset_kind: AssetKind;
  identity: string;
  host?: string | null;
  q: number;
  blocked: boolean;
  primitives: string[];
  observed_at: string;
}

export interface InventoryResponse {
  count: number;
  items: InventoryItem[];
}

export interface OrgPosture {
  org_q: number;
  deadline: string;
  horizon_years: number;
  assets: number;
  blocked_count: number;
}

export interface QkdLinkSummary {
  identity: string;
  kme_endpoint?: string | null;
  link_health: string;
  link_qber?: number | null;
  link_key_rate_bps?: number | null;
  observed_at: string;
}

export interface QkdLinksResponse {
  count: number;
  links: QkdLinkSummary[];
}

export interface EventsResponse {
  count: number;
  events: CryptoInventoryEvent[];
}

// V5 — PQ migration recommendations.

export type RecommendationCost = "trivial" | "low" | "medium" | "high";

export interface Recommendation {
  replaces: string;
  replacement: string;
  rationale: string;
  cost: RecommendationCost;
  caveats: string[];
}

export interface RecommendationItem {
  source_module: string;
  asset_kind: AssetKind;
  identity: string;
  host?: string | null;
  current_primitives: string[];
  recommendations: Recommendation[];
}

export interface RecommendationsResponse {
  count: number;
  items: RecommendationItem[];
}

// V5.2 — TLS-stack ↔ algorithm compatibility matrix.

export type SupportStatus =
  | "supported"
  | "experimental"
  | "not-implemented"
  | "unknown";

export interface CompatEntry {
  stack: string;
  algorithm: string;
  status: SupportStatus;
  min_version?: string | null;
  source?: string | null;
}

export interface CompatResponse {
  count: number;
  items: CompatEntry[];
}

// V5.3 — regulator deadline tracker.

export interface DeadlineEntry {
  jurisdiction: string;
  label: string;
  effective_date: string;
  asset_class: string;
  source: string;
}

export interface DeadlinesResponse {
  count: number;
  items: DeadlineEntry[];
}

// V5.1 — org-level migration roadmap projector.

export interface Milestone {
  label: string;
  date: string;
  asset_ids: string[];
  target_primitives: string[];
}

export interface MilestoneProjection {
  milestone: string;
  date: string;
  org_q_before: number;
  org_q_after: number;
  blocked_before: number;
  blocked_after: number;
  assets_migrated: number;
  assets_remaining_classical: number;
}

export interface RoadmapProjection {
  today_org_q: number;
  today_blocked: number;
  total_assets: number;
  projections: MilestoneProjection[];
}
