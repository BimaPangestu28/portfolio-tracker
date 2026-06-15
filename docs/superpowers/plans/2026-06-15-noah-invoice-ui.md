# Invoice UI (read + download PDF) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose existing invoice data over HTTP and a read-only `/invoices` page (list + detail + PDF download), re-rendering the PDF from stored rows via the existing assemble/render pipeline.

**Architecture:** New repo `invoices::get` + a pure `invoice::rebuild::data_from_row` helper reconstruct `InvoiceData` from a saved row; a new `api/invoices.rs` serves list/detail/pdf/clients on the protected router. Frontend gets schemas + hooks + an authenticated blob helper and an `InvoicesPage` with master-detail and PDF download. No migrations; creation stays in chat.

**Tech Stack:** Rust (axum + sqlx + Typst render), React 18 + React Query + Zod, vitest + Testing Library.

**Reference spec:** `docs/superpowers/specs/2026-06-15-noah-invoice-ui-design.md`

**Branch:** `feat/noah-invoice-ui` (already created off `main`; spec committed).

**Notes for the engineer:**
- Backend is a **bin-only crate**: NO `cargo test --lib`. Use `cargo test <name>` / `cargo check`. NO `cargo fmt`.
- Existing, reuse as-is: `repo::invoices::{InvoiceRow, list_all, insert, max_seq_for_prefix}` (InvoiceRow already derives `Serialize`), `repo::clients::{ClientRow, get, list}` (ClientRow derives `Serialize`; `get(db,id) -> anyhow::Result<ClientRow>`), `invoice::assemble::{assemble_invoice_data, ParsedItem}`, `invoice::config::from_env() -> Result<InvoiceConfig, String>`, `invoice::render::render_pdf(&InvoiceData) -> anyhow::Result<Vec<u8>>`.
- `line_items_json` stores `[{ "title": str, "body": str|null, "qty": int, "amount": int }]` (raw rupiah ints) — maps directly to `ParsedItem { title, body, qty, amount_idr }`.
- `AppError` has `NotFound` and `Other(anyhow::Error)` (see `backend/src/api/events.rs`).
- Frontend money: `import { formatIDR } from "../lib/format"` (already exists). Auth: JWT in `localStorage["pt-auth-token"]`, sent as `authorization: Bearer` by the `api` client (`frontend/src/api/client.ts`).

---

## Task 1: Backend — `invoices::get` + `rebuild::data_from_row`

**Files:**
- Modify: `backend/src/repo/invoices.rs`
- Create: `backend/src/invoice/rebuild.rs`
- Modify: `backend/src/invoice/mod.rs` (add `pub mod rebuild;`)

- [ ] **Step 1: Write the failing rebuild test**

Create `backend/src/invoice/rebuild.rs` containing ONLY a test module first (so it compiles and fails on the missing fn):
```rust
//! Reconstruct display-ready InvoiceData from a stored InvoiceRow for re-rendering.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::config::InvoiceConfig;
    use crate::invoice::model::{Issuer, Payment};
    use crate::repo::clients::ClientRow;
    use crate::repo::invoices::InvoiceRow;

    fn config() -> InvoiceConfig {
        InvoiceConfig {
            issuer: Issuer { name: "Bima".into(), company: "Catalyst".into(), website: "catalystlabs.id".into(), city: "Jakarta".into() },
            payment: Payment { bank: "BCA".into(), account_no: "123".into(), account_name: "Bima".into() },
            due_days: 14,
        }
    }

    #[test]
    fn rebuilds_data_and_preserves_stored_due_date() {
        let row = InvoiceRow {
            id: 1,
            number: "INV/2026/VI/001".into(),
            client_id: 1,
            issue_date: "2026-06-11".into(),
            due_date: "2026-06-30".into(), // 19 days, NOT config's 14
            subtotal: "Rp 12.000.000".into(),
            total: "Rp 12.000.000".into(),
            line_items_json: r#"[{"title":"Landing","body":null,"qty":1,"amount":12000000}]"#.into(),
            created_at: "2026-06-11T08:00:00Z".into(),
        };
        let client = ClientRow { id: 1, name: "PT AIS".into(), sub_name: None, website: None, created_at: String::new() };
        let data = data_from_row(&row, &client, config()).unwrap();
        assert_eq!(data.number, "INV/2026/VI/001");
        assert_eq!(data.total, "Rp 12.000.000");
        assert_eq!(data.issue_date, "11 Juni 2026");
        assert_eq!(data.due_date, "30 Juni 2026"); // preserved from the row, not 25 Juni
        assert_eq!(data.line_items.len(), 1);
    }
}
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd backend && cargo test rebuilds_data_and_preserves_stored_due_date`
Expected: compile error — `data_from_row` not found.

- [ ] **Step 3: Implement the helper (prepend above the test module in `rebuild.rs`)**

```rust
use crate::invoice::assemble::{assemble_invoice_data, ParsedItem};
use crate::invoice::config::InvoiceConfig;
use crate::invoice::model::InvoiceData;
use crate::repo::clients::ClientRow;
use crate::repo::invoices::InvoiceRow;
use serde::Deserialize;

#[derive(Deserialize)]
struct StoredItem {
    title: String,
    #[serde(default)]
    body: Option<String>,
    qty: i64,
    amount: i64,
}

/// Rebuild display-ready `InvoiceData` from a saved row. Parses `line_items_json`
/// and preserves the stored `due_date` by deriving `due_days` from the stored
/// issue/due dates (so a re-rendered PDF matches what was originally issued).
pub fn data_from_row(
    row: &InvoiceRow,
    client: &ClientRow,
    mut config: InvoiceConfig,
) -> anyhow::Result<InvoiceData> {
    let stored: Vec<StoredItem> = serde_json::from_str(&row.line_items_json)?;
    let items: Vec<ParsedItem> = stored
        .into_iter()
        .map(|s| ParsedItem { title: s.title, body: s.body, qty: s.qty, amount_idr: s.amount })
        .collect();
    let issue = chrono::NaiveDate::parse_from_str(&row.issue_date, "%Y-%m-%d")?;
    let due = chrono::NaiveDate::parse_from_str(&row.due_date, "%Y-%m-%d")?;
    config.due_days = (due - issue).num_days();
    Ok(assemble_invoice_data(row.number.clone(), issue, &config, client, &items))
}
```
Then add `pub mod rebuild;` to `backend/src/invoice/mod.rs` (alongside the other `pub mod` lines).

Note: `data_from_row` takes `config` by value (no `Clone` needed on `InvoiceConfig`). The test constructs `InvoiceConfig`/`Issuer`/`Payment` directly — confirm those field names match `backend/src/invoice/{config,model}.rs`; if a field differs, fix the test literal to match (report it).

- [ ] **Step 4: Add `get` to `backend/src/repo/invoices.rs`**

After `list_all`:
```rust
/// Fetch a single invoice by id.
pub async fn get(db: &Db, id: i64) -> anyhow::Result<Option<InvoiceRow>> {
    let row = sqlx::query_as::<_, InvoiceRow>("SELECT * FROM invoice WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}
```

- [ ] **Step 5: Run the test, confirm PASS + compile**

Run: `cd backend && cargo test rebuilds_data_and_preserves_stored_due_date && cargo check`
Expected: test PASS; check clean (pre-existing upwork `next_cursor` warning ok).

- [ ] **Step 6: Commit**

```bash
git add backend/src/repo/invoices.rs backend/src/invoice/rebuild.rs backend/src/invoice/mod.rs
git commit -m "feat(invoice): repo get + rebuild InvoiceData from a stored row"
```

---

## Task 2: Backend — `api/invoices.rs` endpoints + routes

**Files:**
- Create: `backend/src/api/invoices.rs`
- Modify: `backend/src/api/mod.rs` (declare module, register routes, add protection test)

- [ ] **Step 1: Add a failing protection test**

In the `router_tests` module of `backend/src/api/mod.rs`:
```rust
    #[serial]
    #[tokio::test]
    async fn invoice_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-invoice");
        let app = router(test_state().await);
        let uris = ["/invoices", "/invoices/1", "/invoices/1/pdf", "/clients"];
        for uri in uris {
            let res = app.clone().oneshot(
                Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "GET {uri} should be protected");
        }
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd backend && cargo test invoice_routes_are_protected`
Expected: `404 != 401`.

- [ ] **Step 3: Create `backend/src/api/invoices.rs`**

```rust
use crate::error::AppError;
use crate::repo::clients::{self, ClientRow};
use crate::repo::invoices::{self, InvoiceRow};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    Json,
};

/// All invoices, newest first by id (list_all already orders).
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<InvoiceRow>>, AppError> {
    let rows = invoices::list_all(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

/// One invoice by id.
pub async fn get(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<InvoiceRow>, AppError> {
    let row = invoices::get(&s.db, id).await.map_err(AppError::Other)?.ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

/// Re-render the invoice PDF from its stored row.
pub async fn pdf(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Response, AppError> {
    let row = invoices::get(&s.db, id).await.map_err(AppError::Other)?.ok_or(AppError::NotFound)?;
    let client = clients::get(&s.db, row.client_id).await.map_err(AppError::Other)?;
    let config = crate::invoice::config::from_env().map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
    let data = crate::invoice::rebuild::data_from_row(&row, &client, config).map_err(AppError::Other)?;
    let bytes = crate::invoice::render::render_pdf(&data).map_err(AppError::Other)?;
    let filename = format!("{}.pdf", row.number.replace('/', "-"));
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
        ],
        bytes,
    )
        .into_response())
}

/// All clients.
pub async fn list_clients(State(s): State<AppState>) -> Result<Json<Vec<ClientRow>>, AppError> {
    let rows = clients::list(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
```

- [ ] **Step 4: Declare the module + register routes in `backend/src/api/mod.rs`**

Add the module declaration next to the other `mod <name>;` lines at the top of `api/mod.rs` (match the existing visibility, e.g. `mod invoices;`). Then, in the protected router (near the `/events`/`/goals` routes), add:
```rust
        .route("/invoices", get(invoices::list))
        .route("/invoices/:id", get(invoices::get))
        .route("/invoices/:id/pdf", get(invoices::pdf))
        .route("/clients", get(invoices::list_clients))
```
(`get` is already imported in `mod.rs`.)

- [ ] **Step 5: Run the test + compile**

Run: `cd backend && cargo test invoice_routes_are_protected && cargo check`
Expected: test PASS; check clean.

- [ ] **Step 6: Commit**

```bash
git add backend/src/api/invoices.rs backend/src/api/mod.rs
git commit -m "feat(api): read-only invoice + clients endpoints with PDF re-render"
```

---

## Task 3: Frontend — schemas, blob helper, hooks

**Files:**
- Modify: `frontend/src/api/schemas.ts`
- Modify: `frontend/src/api/client.ts`
- Modify: `frontend/src/api/hooks.ts`
- Create: `frontend/src/api/invoice-schemas.test.ts`

- [ ] **Step 1: Write the failing schema test**

Create `frontend/src/api/invoice-schemas.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { InvoiceSchema, ClientSchema, InvoiceLineItemSchema } from "./schemas";

describe("invoice schemas", () => {
  it("parses an invoice row", () => {
    const inv = InvoiceSchema.parse({
      id: 1, number: "INV/2026/VI/001", client_id: 3, issue_date: "2026-06-11", due_date: "2026-06-25",
      subtotal: "Rp 12.000.000", total: "Rp 12.000.000",
      line_items_json: '[{"title":"Landing","body":null,"qty":1,"amount":12000000}]',
      created_at: "2026-06-11T08:00:00Z",
    });
    expect(inv.number).toBe("INV/2026/VI/001");
    const items = InvoiceLineItemSchema.array().parse(JSON.parse(inv.line_items_json));
    expect(items[0].amount).toBe(12000000);
  });

  it("parses a client row", () => {
    const c = ClientSchema.parse({ id: 3, name: "PT AIS", sub_name: null, website: null, created_at: "2026-06-01T00:00:00Z" });
    expect(c.name).toBe("PT AIS");
  });
});
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/api/invoice-schemas.test.ts`
Expected: FAIL — schemas not exported.

- [ ] **Step 3: Add schemas to `frontend/src/api/schemas.ts`**

Append:
```ts
export const ClientSchema = z.object({
  id: z.number(),
  name: z.string(),
  sub_name: z.string().nullable(),
  website: z.string().nullable(),
  created_at: z.string(),
});
export type Client = z.infer<typeof ClientSchema>;

export const InvoiceSchema = z.object({
  id: z.number(),
  number: z.string(),
  client_id: z.number(),
  issue_date: z.string(),
  due_date: z.string(),
  subtotal: z.string(),
  total: z.string(),
  line_items_json: z.string(),
  created_at: z.string(),
});
export type Invoice = z.infer<typeof InvoiceSchema>;

export const InvoiceLineItemSchema = z.object({
  title: z.string(),
  body: z.string().nullable().optional(),
  qty: z.number(),
  amount: z.number(),
});
export type InvoiceLineItem = z.infer<typeof InvoiceLineItemSchema>;
```

- [ ] **Step 4: Add `getBlob` to `frontend/src/api/client.ts`**

The file has `const BASE`, `authHeader()`, and exports `const api = { ... }`. Add a method to the `api` object:
```ts
  getBlob: async (path: string): Promise<Blob> => {
    const res = await fetch(`${BASE}${path}`, { headers: { ...authHeader() } });
    if (res.status === 401) {
      localStorage.removeItem("pt-auth-token");
      window.dispatchEvent(new Event("pt-unauthorized"));
    }
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.blob();
  },
```
(Place it inside the `api` object literal, e.g. right after `del:`. Note `TOKEN_KEY` is a module const = `"pt-auth-token"`; use that const if in scope, else the literal as shown.)

- [ ] **Step 5: Add hooks to `frontend/src/api/hooks.ts`**

Add `InvoiceSchema`, `ClientSchema` to the import-from-`./schemas` line, then append after the existing query hooks:
```ts
export const useInvoices = () =>
  useQuery({ queryKey: ["invoices"], queryFn: () => api.get("/invoices", z.array(InvoiceSchema)) });

export const useClients = () =>
  useQuery({ queryKey: ["clients"], queryFn: () => api.get("/clients", z.array(ClientSchema)) });

export const useInvoice = (id: number | null) =>
  useQuery({ queryKey: ["invoice", id], enabled: id != null, queryFn: () => api.get(`/invoices/${id}`, InvoiceSchema) });
```

- [ ] **Step 6: Test + type-check**

Run: `cd frontend && npx vitest run src/api/invoice-schemas.test.ts && npx tsc --noEmit`
Expected: test PASS; tsc clean.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/client.ts frontend/src/api/hooks.ts frontend/src/api/invoice-schemas.test.ts
git commit -m "feat(web): invoice/client schemas, blob helper, query hooks"
```

---

## Task 4: Frontend — InvoicesPage + route + nav

**Files:**
- Create: `frontend/src/pages/InvoicesPage.tsx`
- Create: `frontend/src/pages/InvoicesPage.test.tsx`
- Modify: `frontend/src/App.tsx` (route), `frontend/src/components/AppShell.tsx` (nav item)

- [ ] **Step 1: Write the failing page test**

Create `frontend/src/pages/InvoicesPage.test.tsx`:
```tsx
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import InvoicesPage from "./InvoicesPage";
import * as hooks from "../api/hooks";
import { api } from "../api/client";

vi.mock("../api/hooks");
vi.mock("../api/client", () => ({ api: { getBlob: vi.fn() } }));

const invoice = {
  id: 1, number: "INV/2026/VI/001", client_id: 3, issue_date: "2026-06-11", due_date: "2026-06-25",
  subtotal: "Rp 12.000.000", total: "Rp 12.000.000",
  line_items_json: '[{"title":"Landing","body":null,"qty":1,"amount":12000000}]',
  created_at: "2026-06-11T08:00:00Z",
};

describe("InvoicesPage", () => {
  beforeEach(() => {
    vi.mocked(hooks.useInvoices).mockReturnValue({ data: [invoice], isLoading: false, isError: false } as any);
    vi.mocked(hooks.useClients).mockReturnValue({ data: [{ id: 3, name: "PT AIS", sub_name: null, website: null, created_at: "" }], isLoading: false, isError: false } as any);
    vi.mocked(hooks.useInvoice).mockReturnValue({ data: invoice, isLoading: false, isError: false } as any);
    vi.mocked(api.getBlob).mockReset();
    vi.mocked(api.getBlob).mockResolvedValue(new Blob(["%PDF"], { type: "application/pdf" }));
  });

  it("lists invoices with the client name", () => {
    render(<InvoicesPage />);
    expect(screen.getByText("INV/2026/VI/001")).toBeInTheDocument();
    expect(screen.getAllByText("PT AIS").length).toBeGreaterThan(0);
  });

  it("shows detail and downloads the PDF", async () => {
    render(<InvoicesPage />);
    fireEvent.click(screen.getByText("INV/2026/VI/001"));
    fireEvent.click(screen.getByRole("button", { name: /download pdf/i }));
    await waitFor(() => expect(api.getBlob).toHaveBeenCalledWith("/invoices/1/pdf"));
  });
});
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/pages/InvoicesPage.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Create `frontend/src/pages/InvoicesPage.tsx`**

```tsx
import { useState } from "react";
import { FileText, Download } from "lucide-react";
import { useInvoices, useClients, useInvoice } from "../api/hooks";
import { InvoiceLineItemSchema } from "../api/schemas";
import { api } from "../api/client";
import { formatIDR } from "../lib/format";

function parseLineItems(json: string) {
  try {
    return InvoiceLineItemSchema.array().parse(JSON.parse(json));
  } catch {
    return [];
  }
}

export default function InvoicesPage() {
  const invoices = useInvoices();
  const clients = useClients();
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = useInvoice(selectedId);
  const [downloading, setDownloading] = useState(false);

  const clientName = (id: number) => clients.data?.find((c) => c.id === id)?.name ?? "—";

  async function handleDownload(id: number, number: string) {
    setDownloading(true);
    try {
      const blob = await api.getBlob(`/invoices/${id}/pdf`);
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${number.replace(/\//g, "-")}.pdf`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } finally {
      setDownloading(false);
    }
  }

  const rows = invoices.data ?? [];
  const detail = selected.data;
  const items = detail ? parseLineItems(detail.line_items_json) : [];

  return (
    <div className="flex col gap-5">
      <div>
        <h1 className="t-h1">Invoice</h1>
        <div className="t-sm t-muted">Daftar invoice & unduh PDF</div>
      </div>

      <div className="grid gap-5" style={{ gridTemplateColumns: "minmax(0,1.3fr) minmax(0,1fr)" }}>
        {/* List */}
        <div className="card">
          <div className="card-head"><div className="card-title">Semua invoice</div></div>
          <div className="card-pad flex col gap-1" style={{ paddingTop: 12 }}>
            {rows.length === 0 && <p className="text-sm text-muted-foreground">Belum ada invoice.</p>}
            {rows.map((inv) => (
              <button
                key={inv.id}
                className={`flex items-center gap-2 text-sm${selectedId === inv.id ? " active" : ""}`}
                style={{ justifyContent: "space-between", padding: "8px 6px", borderRadius: 8, textAlign: "left", background: selectedId === inv.id ? "hsl(var(--muted))" : "transparent" }}
                onClick={() => setSelectedId(inv.id)}
              >
                <span className="flex-1 truncate">{inv.number}</span>
                <span className="text-muted-foreground truncate" style={{ maxWidth: 120 }}>{clientName(inv.client_id)}</span>
                <span className="num">{inv.total}</span>
              </button>
            ))}
          </div>
        </div>

        {/* Detail */}
        <div className="card">
          <div className="card-head"><div className="card-title">Detail</div></div>
          <div className="card-pad flex col gap-2" style={{ paddingTop: 12 }}>
            {!detail && <p className="text-sm text-muted-foreground">Pilih invoice.</p>}
            {detail && (
              <>
                <div className="flex items-center justify-between">
                  <div>
                    <div style={{ fontWeight: 600 }}>{detail.number}</div>
                    <div className="t-xs t-muted">{clientName(detail.client_id)}</div>
                  </div>
                  <button className="btn btn-primary" disabled={downloading} onClick={() => handleDownload(detail.id, detail.number)}>
                    <Download size={15} /> Download PDF
                  </button>
                </div>
                <div className="t-xs t-muted">Terbit {detail.issue_date} · Jatuh tempo {detail.due_date}</div>
                <div className="flex col gap-1" style={{ marginTop: 8 }}>
                  {items.map((it, idx) => (
                    <div key={idx} className="flex items-center gap-2 text-sm">
                      <span className="flex-1 truncate">{it.title}</span>
                      <span className="text-muted-foreground">×{it.qty}</span>
                      <span className="num">{formatIDR(it.amount)}</span>
                    </div>
                  ))}
                </div>
                <div className="flex items-center justify-between text-sm" style={{ borderTop: "1px solid hsl(var(--border))", paddingTop: 8, marginTop: 4, fontWeight: 600 }}>
                  <span>Total</span>
                  <span className="num">{detail.total}</span>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
```
Note: `FileText` is imported here only if used; it is NOT used in the page body, so DROP the `FileText` import (keep only `Download`) to avoid an unused-import lint. (The nav uses `FileText` in `AppShell.tsx`, not here.)

- [ ] **Step 4: Run the page test, confirm PASS**

Run: `cd frontend && npx vitest run src/pages/InvoicesPage.test.tsx`
Expected: both tests pass.

- [ ] **Step 5: Add the route in `frontend/src/App.tsx`**

Add `import InvoicesPage from "./pages/InvoicesPage";` with the other page imports, then inside the AppShell route group (near the `portfolio`/`data` routes):
```tsx
        <Route path="invoices" element={<InvoicesPage />} />
```

- [ ] **Step 6: Add the nav item in `frontend/src/components/AppShell.tsx`**

Add `FileText` to the lucide-react import. In `NAV_GROUPS`, the "Keuangan" group, after the Data entry:
```tsx
      { to: "/invoices", label: "Invoice", icon: FileText },
```

- [ ] **Step 7: Full verification**

Run: `cd frontend && npx tsc --noEmit && npx vitest run && npm run build`
Expected: tsc clean; all tests pass; build succeeds.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/pages/InvoicesPage.tsx frontend/src/pages/InvoicesPage.test.tsx frontend/src/App.tsx frontend/src/components/AppShell.tsx
git commit -m "feat(web): InvoicesPage (list + detail + PDF download), route + nav"
```

---

## Final verification (end-to-end)

- [ ] **Backend:** `cd backend && cargo check && cargo test rebuilds_data_and_preserves_stored_due_date && cargo test invoice_routes_are_protected` — all pass.
- [ ] **Frontend:** `cd frontend && npx tsc --noEmit && npx vitest run && npm run build` — clean, all tests pass, build succeeds.
- [ ] **Manual:** open `/invoices`; list shows numbers + client names + totals; click a row → detail with line items + totals; "Download PDF" downloads a valid `<number>.pdf`. Confirm chat-created invoices appear.

---

## Self-review notes

- **Spec coverage:** repo `get` + rebuild helper (T1); `GET /invoices`, `/invoices/:id`, `/invoices/:id/pdf`, `/clients` + protection test (T2); schemas + `getBlob` + hooks (T3); `InvoicesPage` list+detail+download, route, "Invoice" nav in Keuangan (T4). All spec items covered.
- **Out-of-scope respected:** no create/edit/delete UI, no client management, no filter/pagination.
- **Type consistency:** `data_from_row(row, client, config_by_value)` used identically in T1 def and T2 `pdf` handler; `invoices::get -> Option<InvoiceRow>` returned and `.ok_or(NotFound)` in both `get`/`pdf`; FE `InvoiceSchema`/`ClientSchema`/`InvoiceLineItemSchema` defined in T3 and consumed in T4; `api.getBlob(path)` defined T3, called T4 with `/invoices/${id}/pdf` (matches the test assertion).
- **Fidelity note:** PDF re-render derives `due_days` from stored dates so the rendered due date matches the issued invoice even if the env config's `due_days` changed.
