CREATE TABLE cashflow_category (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  monthly_budget TEXT,
  color TEXT
);
CREATE TABLE cashflow (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER REFERENCES account(id),
  occurred_on TEXT NOT NULL,
  direction TEXT NOT NULL,
  amount TEXT NOT NULL,
  currency TEXT NOT NULL,
  category_id INTEGER REFERENCES cashflow_category(id),
  note TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_cashflow_month ON cashflow(occurred_on);
