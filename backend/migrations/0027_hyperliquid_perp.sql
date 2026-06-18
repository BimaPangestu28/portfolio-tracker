CREATE TABLE IF NOT EXISTS hl_position (
    coin            TEXT PRIMARY KEY,
    direction       TEXT NOT NULL,
    size            TEXT NOT NULL,
    entry_px        TEXT NOT NULL,
    mark_px         TEXT NOT NULL,
    unrealized_pnl  TEXT NOT NULL,
    leverage        TEXT NOT NULL,
    notional        TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS hl_trade (
    external_id   TEXT PRIMARY KEY,
    coin          TEXT NOT NULL,
    direction     TEXT NOT NULL,
    size          TEXT NOT NULL,
    entry_px      TEXT NOT NULL,
    exit_px       TEXT NOT NULL,
    realized_pnl  TEXT NOT NULL,
    fee           TEXT NOT NULL,
    opened_at     TEXT NOT NULL,
    closed_at     TEXT NOT NULL,
    leverage      INTEGER,
    confidence    INTEGER,
    timeframe     TEXT,
    profile       TEXT
);

CREATE INDEX IF NOT EXISTS idx_hl_trade_closed_at ON hl_trade(closed_at DESC);
