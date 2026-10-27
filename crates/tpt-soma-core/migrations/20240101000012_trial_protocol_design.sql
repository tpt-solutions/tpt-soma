-- Trial protocol design (tpt-soma-clinica::protocol): treatment arms and visit
-- schedule on the protocol, and the arm each enrolled subject was assigned.
-- Existing protocols keep empty arms and visits, and their enrollments keep a
-- NULL arm. Allocation is not randomization; see the protocol module docs.

ALTER TABLE clinical_trial_cohorts
    ADD COLUMN IF NOT EXISTS arms JSONB NOT NULL DEFAULT '[]';

ALTER TABLE clinical_trial_cohorts
    ADD COLUMN IF NOT EXISTS visits JSONB NOT NULL DEFAULT '[]';

ALTER TABLE trial_enrollments
    ADD COLUMN IF NOT EXISTS arm TEXT;
