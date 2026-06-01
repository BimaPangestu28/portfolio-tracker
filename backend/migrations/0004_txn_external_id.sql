ALTER TABLE txn ADD COLUMN source TEXT;
ALTER TABLE txn ADD COLUMN external_id TEXT;
CREATE UNIQUE INDEX idx_txn_source_ext ON txn(source, external_id) WHERE source IS NOT NULL AND external_id IS NOT NULL;
