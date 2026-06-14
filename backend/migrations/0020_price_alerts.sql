-- Fase 6: user-defined per-instrument price alerts (fire once).
CREATE TABLE price_alerts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  instrument_id INTEGER NOT NULL REFERENCES instrument(id),
  target_price TEXT NOT NULL,
  direction TEXT NOT NULL CHECK (direction IN ('above', 'below')),
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'triggered', 'cancelled')),
  created_at TEXT NOT NULL,
  triggered_at TEXT
);
CREATE INDEX idx_price_alerts_active ON price_alerts (status, instrument_id);
