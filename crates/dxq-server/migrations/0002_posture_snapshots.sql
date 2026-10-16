-- Posture-trend time series (SEZ-32).
--
-- One row per retained org-rollup snapshot. The collector inserts a
-- row on each GET /v1/posture read that clears the write-rate gate
-- (dxq-server::history::SnapshotThrottle). Durable so the trend
-- survives a server restart, unlike the in-memory backend's ring.
CREATE TABLE IF NOT EXISTS posture_snapshots (
    id              BIGSERIAL   PRIMARY KEY,
    observed_at     TIMESTAMPTZ NOT NULL,
    org_q           REAL        NOT NULL,
    asset_count     INTEGER     NOT NULL,
    blocked_count   INTEGER     NOT NULL
);

-- GET /v1/posture/history?since_secs=N → WHERE observed_at >= $1
-- ORDER BY observed_at.
CREATE INDEX IF NOT EXISTS idx_posture_snapshots_observed_at
    ON posture_snapshots (observed_at);
