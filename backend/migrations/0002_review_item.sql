CREATE TABLE review_item (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,            -- 'image' | 'pdf'
    source_filename TEXT NOT NULL,
    source_path TEXT NOT NULL,
    doc_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending' | 'confirmed' | 'rejected'
    needs_attention INTEGER NOT NULL DEFAULT 0,
    payload_json TEXT NOT NULL,
    raw_llm_json TEXT NOT NULL,
    suggested_instrument_id INTEGER REFERENCES instrument(id),
    suggested_account_id INTEGER REFERENCES account(id),
    created_txn_id INTEGER REFERENCES txn(id),
    created_at TEXT NOT NULL,
    confirmed_at TEXT
);
CREATE INDEX idx_review_item_status ON review_item(status, batch_id);
