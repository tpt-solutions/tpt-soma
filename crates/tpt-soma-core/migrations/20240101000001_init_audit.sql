CREATE TABLE audit_ledger (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor TEXT NOT NULL,
    resource_class TEXT NOT NULL,
    action TEXT NOT NULL,
    cohort_scope TEXT[] NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT now(),
    query_fingerprint TEXT NOT NULL,
    outcome TEXT NOT NULL,
    prev_row_hash TEXT,
    row_hash TEXT NOT NULL
);

CREATE INDEX idx_audit_timestamp ON audit_ledger (timestamp);
CREATE INDEX idx_audit_actor ON audit_ledger (actor);
