-- Track when the Google sync loop last completed a successful pass, so the UI
-- can show "last synced" and a manual "sync now" can report freshness.
ALTER TABLE google_integration ADD COLUMN last_synced_at TEXT;
