CREATE TABLE plan_node (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_id          INTEGER REFERENCES plan_node(id) ON DELETE CASCADE,
  name               TEXT NOT NULL,
  target_pct         TEXT NOT NULL,
  tolerance_band_pct TEXT,
  bind_kind          TEXT NOT NULL,                       -- 'group' | 'category' | 'instrument'
  category_id        INTEGER REFERENCES category(id),
  instrument_id      INTEGER REFERENCES instrument(id),
  sort_order         INTEGER NOT NULL DEFAULT 0,
  color              TEXT
);
CREATE INDEX idx_plan_node_parent ON plan_node(parent_id);

-- Backfill: every existing category becomes a root node bound to that category,
-- carrying its target/tolerance/color/order. category.target_pct is now deprecated
-- (kept for rollback safety) and no longer read for targets.
INSERT INTO plan_node (parent_id, name, target_pct, tolerance_band_pct, bind_kind, category_id, instrument_id, sort_order, color)
SELECT NULL, name, target_pct, tolerance_band_pct, 'category', id, NULL, sort_order, color
FROM category;
