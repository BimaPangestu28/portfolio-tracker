-- Tag a transaction to at most one goal; progress for current_kind='tagged'
-- goals is computed from the txns carrying their goal_id.
ALTER TABLE txn ADD COLUMN goal_id INTEGER REFERENCES goal(id);
CREATE INDEX idx_txn_goal ON txn(goal_id);

-- Optional target date for a goal (ISO 'YYYY-MM-DD'); drives required-monthly.
ALTER TABLE goal ADD COLUMN target_date TEXT;
