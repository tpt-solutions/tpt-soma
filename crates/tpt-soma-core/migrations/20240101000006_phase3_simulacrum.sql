-- Phase 3: Digital-twin / simulation schema (tpt-soma-simulacrum)
-- Relational backing for simulation runs, fitted parameter sets, calibration
-- targets, and emitted trajectories.

CREATE TABLE IF NOT EXISTS simulation_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    subject_id TEXT NOT NULL,
    model_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    status TEXT NOT NULL
);

CREATE INDEX idx_simulation_runs_subject ON simulation_runs (subject_id);

CREATE TABLE IF NOT EXISTS simulation_parameter_sets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES simulation_runs(id),
    param_name TEXT NOT NULL,
    param_value DOUBLE PRECISION NOT NULL
);

CREATE INDEX idx_simulation_params_run ON simulation_parameter_sets (run_id);

CREATE TABLE IF NOT EXISTS calibration_targets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES simulation_runs(id),
    target_name TEXT NOT NULL,
    target_value DOUBLE PRECISION NOT NULL
);

CREATE TABLE IF NOT EXISTS simulation_outputs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES simulation_runs(id),
    ts TIMESTAMPTZ NOT NULL DEFAULT now(),
    series_name TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL
);

CREATE INDEX idx_simulation_outputs_run ON simulation_outputs (run_id, series_name, ts);

-- Phase 3 data class (also seeded in tpt-soma-capability registry)
INSERT INTO data_class_registry (id, description, sensitivity) VALUES
    ('simulation_output', 'Digital-twin simulation outputs (trajectories, parameter sets)', 'Confidential')
ON CONFLICT (id) DO NOTHING;
