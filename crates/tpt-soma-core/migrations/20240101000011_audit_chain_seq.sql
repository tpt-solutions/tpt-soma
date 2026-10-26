-- The hash chain is ordered by insertion, not by `timestamp`. `timestamp` holds
-- the time the request started, so requests that overlap can be appended in a
-- different order from their start times. `seq` records the append order.
ALTER TABLE audit_ledger ADD COLUMN seq BIGSERIAL;
CREATE UNIQUE INDEX audit_ledger_seq_idx ON audit_ledger (seq);
