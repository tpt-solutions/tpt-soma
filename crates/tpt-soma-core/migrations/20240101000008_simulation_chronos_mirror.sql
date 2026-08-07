-- Phase 4 (deferred item §2.2): mirror simulation outputs into Keystone's
-- Chronos time-series extension so digital-twin trajectories are queryable
-- through the same longitudinal-time-series path as CGM / organ-function
-- trajectories.
--
-- `simulation_series` is the Chronos-style store for a run's emitted series.
-- The original relational `simulation_outputs` table (migration 06) remains the
-- authoritative, audit-friendly record; this table is the query-friendly
-- time-series mirror populated by `simulacrum::storage::mirror_outputs_to_chronos`.

CREATE TABLE IF NOT EXISTS simulation_series (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL,
    ts TIMESTAMPTZ NOT NULL,
    series_name TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_simulation_series_run
    ON simulation_series (run_id, series_name, ts);
