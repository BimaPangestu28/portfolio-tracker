-- DCA planner settings: a single persisted row (id = 1).
CREATE TABLE IF NOT EXISTS dca_setting (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    monthly_budget TEXT    NOT NULL DEFAULT '0',
    frequency      TEXT    NOT NULL DEFAULT 'monthly' CHECK (frequency IN ('monthly', 'weekly')),
    anchor_day     INTEGER NOT NULL DEFAULT 1,
    rounding_step  TEXT    NOT NULL DEFAULT '10000',
    updated_at     TEXT    NOT NULL
);
