-- Device-state observations: one row per pathway per telemetry sample, from a
-- tpt-hands end-effector. Written only through Arrow Flight do_put, under the
-- `ingest` capability action and the `device_state` resource class (TM-02).
-- These rows carry no patient identifiers. tool_id names a device, not a person.

CREATE TABLE IF NOT EXISTS device_state_observations (
    id BIGSERIAL PRIMARY KEY,
    tool_id TEXT NOT NULL,
    timestamp_us BIGINT NOT NULL,
    structural_integrity DOUBLE PRECISION NOT NULL,
    pathway_id INTEGER NOT NULL,
    pathway_state TEXT NOT NULL
        CHECK (pathway_state IN ('intact', 'severed', 'hypertrophied')),
    healing_progress DOUBLE PRECISION NOT NULL,
    resistance_ohms DOUBLE PRECISION NOT NULL,
    max_bandwidth_mbps DOUBLE PRECISION NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_device_state_tool_time
    ON device_state_observations (tool_id, timestamp_us);
