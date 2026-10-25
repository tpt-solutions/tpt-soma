-- Clinical trial results (tpt-soma-clinica): persisted enrollment decisions and
-- adverse events, layered on clinical_trial_cohorts. subject_id is the same
-- pseudonymous identifier used elsewhere; no direct identifiers are stored.

CREATE TABLE IF NOT EXISTS trial_enrollments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trial_cohort_id UUID NOT NULL REFERENCES clinical_trial_cohorts (id),
    subject_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('enrolled', 'screen_failed')),
    failed_criteria JSONB NOT NULL DEFAULT '[]',
    screened_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (trial_cohort_id, subject_id)
);

CREATE INDEX IF NOT EXISTS idx_trial_enrollments_cohort
    ON trial_enrollments (trial_cohort_id, status);

CREATE TABLE IF NOT EXISTS trial_adverse_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    trial_cohort_id UUID NOT NULL REFERENCES clinical_trial_cohorts (id),
    subject_id TEXT NOT NULL,
    term TEXT NOT NULL,
    grade SMALLINT NOT NULL CHECK (grade BETWEEN 1 AND 5),
    serious BOOLEAN NOT NULL,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_trial_ae_cohort
    ON trial_adverse_events (trial_cohort_id);
