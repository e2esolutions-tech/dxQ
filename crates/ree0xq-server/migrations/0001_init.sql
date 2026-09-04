-- V1 schema for the Postgres-backed ree0xq-server collector.
--
-- Two tables: an append-only event log (history) and a
-- (source_module, kind, identity)-keyed current-state table for
-- fast `/v1/inventory` and `/v1/posture` reads. Both store the
-- event body as JSONB so adding optional schema_minor fields
-- doesn't require a migration; the columns we filter or order
-- on are denormalised out of the JSONB for index-friendly
-- queries.

CREATE TABLE IF NOT EXISTS events (
    id              BIGSERIAL PRIMARY KEY,
    observed_at     TIMESTAMPTZ NOT NULL,
    source_module   TEXT        NOT NULL,
    asset_kind      TEXT        NOT NULL,
    asset_identity  TEXT        NOT NULL,
    body            JSONB       NOT NULL
);

-- Hot path: GET /v1/events?limit=N → ORDER BY observed_at DESC.
CREATE INDEX IF NOT EXISTS idx_events_observed_at_desc
    ON events (observed_at DESC);

-- Per-asset latest snapshot. The collector upserts here on
-- every ingest where the incoming event's observed_at is
-- at-or-after the row's current observed_at.
CREATE TABLE IF NOT EXISTS assets (
    source_module   TEXT        NOT NULL,
    asset_kind      TEXT        NOT NULL,
    asset_identity  TEXT        NOT NULL,
    observed_at     TIMESTAMPTZ NOT NULL,
    body            JSONB       NOT NULL,
    PRIMARY KEY (source_module, asset_kind, asset_identity)
);

-- Inventory queries that filter by kind / host or sort by
-- recency benefit from an observed_at index; PK already covers
-- the dedup-key lookup path.
CREATE INDEX IF NOT EXISTS idx_assets_kind
    ON assets (asset_kind);
CREATE INDEX IF NOT EXISTS idx_assets_observed_at_desc
    ON assets (observed_at DESC);
