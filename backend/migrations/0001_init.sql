CREATE TABLE account (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    account_type TEXT NOT NULL,
    institution TEXT,
    native_currency TEXT NOT NULL,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE category (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    target_pct TEXT NOT NULL,
    tolerance_band_pct TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    color TEXT
);

CREATE TABLE instrument (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol TEXT NOT NULL,
    name TEXT NOT NULL,
    instrument_type TEXT NOT NULL,
    native_currency TEXT NOT NULL,
    category_id INTEGER REFERENCES category(id),
    price_source TEXT NOT NULL,
    decimals INTEGER NOT NULL DEFAULT 8,
    note TEXT
);

CREATE TABLE txn (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id INTEGER NOT NULL REFERENCES account(id),
    instrument_id INTEGER NOT NULL REFERENCES instrument(id),
    txn_type TEXT NOT NULL,
    executed_at TEXT NOT NULL,
    quantity TEXT NOT NULL,
    price_native TEXT NOT NULL,
    fee_native TEXT NOT NULL DEFAULT '0',
    currency TEXT NOT NULL,
    fx_to_idr TEXT NOT NULL,
    fx_to_usd TEXT NOT NULL,
    note TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_txn_instrument ON txn(instrument_id, executed_at);

CREATE TABLE price_quote (
    instrument_id INTEGER NOT NULL REFERENCES instrument(id),
    as_of TEXT NOT NULL,
    price_native TEXT NOT NULL,
    currency TEXT NOT NULL,
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    PRIMARY KEY (instrument_id, as_of, kind)
);

CREATE TABLE fx_rate (
    as_of TEXT NOT NULL,
    base TEXT NOT NULL,
    quote TEXT NOT NULL,
    rate TEXT NOT NULL,
    PRIMARY KEY (as_of, base, quote)
);

CREATE TABLE valuation_snapshot (
    as_of TEXT PRIMARY KEY,
    total_idr TEXT NOT NULL,
    total_usd TEXT NOT NULL,
    breakdown_json TEXT NOT NULL
);
