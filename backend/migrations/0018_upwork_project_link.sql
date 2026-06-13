-- Maps each synced Upwork contract to the ClickUp List created for it. A row's
-- existence means "already synced" — the idempotency guarantee for contract sync.
CREATE TABLE upwork_project_link (
  contract_id TEXT PRIMARY KEY,
  clickup_list_id TEXT NOT NULL,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL
);
