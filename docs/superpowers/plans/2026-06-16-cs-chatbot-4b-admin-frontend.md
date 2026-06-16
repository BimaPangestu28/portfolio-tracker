# CS Chatbot — Plan 4b: Admin Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Four authenticated SPA pages so the owner can operate the CS bot: a Knowledge-Base manager, a Pricing manager, an Orders manager, and a CS Inbox (conversations + transcripts + escalations).

**Architecture:** Mirror the established `BudgetPage` pattern (list-in-card + Dialog form + `useInvalidatingMutation` hooks) and the `src/api/{schemas,hooks,client}.ts` conventions. New Zod schemas validate every `/cs/admin/*` response (Plan 4a contract); React Query hooks own server state; four pages under `/cs/admin/*` routes with a new "Admin (CS)" nav group. No new dependencies.

**Tech Stack:** React, TypeScript (strict), React Query, Zod, Tailwind/Radix, vitest. Depends on Plan 4a endpoints.

> **Work in the worktree** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`, frontend dir `frontend/`. `npm run build` runs `tsc -b` (strict). Tests: `npx vitest run <path>`.

> **Endpoint contract (Plan 4a):**
> - `GET /cs/admin/docs` → `KbDoc[]`; `POST /cs/admin/docs {title,source?,body}` → `KbDoc`; `PATCH /cs/admin/docs/:id {title,source?,body}` → null; `DELETE /cs/admin/docs/:id`; `POST /cs/admin/kb/reindex` → `{embedded:number}`
> - `GET /cs/admin/products` → `Product[]`; `POST /cs/admin/products {name,description?,price?,currency?,availability?}` → `{id}`; `PATCH /cs/admin/products/:id {...}` → null; `POST /cs/admin/products/:id/active {active}` → null; `DELETE /cs/admin/products/:id`
> - `GET /cs/admin/orders` → `Order[]`; `POST /cs/admin/orders {external_ref,customer_name?,customer_contact?,status,details_json?}` → null; `DELETE /cs/admin/orders/:id`
> - `GET /cs/admin/conversations` → `CsConversation[]`; `GET /cs/admin/conversations/:id/messages` → `CsMessage[]`; `POST /cs/admin/conversations/:id/resolve` → null
> - `GET /cs/admin/escalations` → `CsEscalation[]`; `POST /cs/admin/escalations/:id/handle` → null

---

## File Structure

- Modify: `frontend/src/api/schemas.ts` — add CS admin schemas.
- Modify: `frontend/src/api/hooks.ts` — add CS admin query + mutation hooks.
- Create: `frontend/src/pages/CsPricingPage.tsx`, `CsDocsPage.tsx`, `CsOrdersPage.tsx`, `CsInboxPage.tsx`.
- Modify: `frontend/src/App.tsx` — 4 routes.
- Modify: `frontend/src/components/AppShell.tsx` — "Admin (CS)" nav group.
- Create: `frontend/src/api/hooks.cs.test.tsx` — hook tests.

---

## Task 1: Schemas

**Files:** Modify `frontend/src/api/schemas.ts`

- [ ] **Step 1: Add the schemas** (append, matching the file's `z.object` + `z.infer` export style)

```ts
export const KbDocSchema = z.object({
  id: z.number(),
  title: z.string(),
  source: z.string().nullable(),
  body: z.string(),
  updated_at: z.string(),
});
export type KbDoc = z.infer<typeof KbDocSchema>;

export const CsProductSchema = z.object({
  id: z.number(),
  name: z.string(),
  description: z.string().nullable(),
  price: z.number().nullable(),
  currency: z.string().nullable(),
  availability: z.string().nullable(),
  active: z.number(), // SQLite 0/1
  updated_at: z.string(),
});
export type CsProduct = z.infer<typeof CsProductSchema>;

export const CsOrderSchema = z.object({
  id: z.number(),
  external_ref: z.string(),
  customer_name: z.string().nullable(),
  customer_contact: z.string().nullable(),
  status: z.string(),
  details_json: z.string().nullable(),
  updated_at: z.string(),
});
export type CsOrder = z.infer<typeof CsOrderSchema>;

export const CsConversationSchema = z.object({
  id: z.number(),
  channel: z.string(),
  visitor_name: z.string().nullable(),
  visitor_email: z.string().nullable(),
  visitor_phone: z.string().nullable(),
  session_token: z.string(),
  status: z.string(),
  created_at: z.string(),
  last_msg_at: z.string(),
});
export type CsConversation = z.infer<typeof CsConversationSchema>;

export const CsMessageSchema = z.object({
  id: z.number(),
  conversation_id: z.number(),
  role: z.string(),
  content: z.string(),
  created_at: z.string(),
});
export type CsMessage = z.infer<typeof CsMessageSchema>;

export const CsEscalationSchema = z.object({
  id: z.number(),
  conversation_id: z.number(),
  reason: z.string(),
  summary: z.string(),
  status: z.string(),
  created_at: z.string(),
  handled_at: z.string().nullable(),
});
export type CsEscalation = z.infer<typeof CsEscalationSchema>;
```

- [ ] **Step 2: Typecheck + commit**

Run: `cd frontend && npx tsc -b 2>&1 | tail -8` (no errors).

```bash
git add frontend/src/api/schemas.ts
git commit -m "feat(cs-admin): zod schemas for CS admin entities"
```

---

## Task 2: Hooks

**Files:** Modify `frontend/src/api/hooks.ts`

- [ ] **Step 1: Add the hooks** (append; reuse the existing `useInvalidatingMutation` helper + `api` + `z`. Confirm those imports exist at the top of the file; the research shows they do.)

```ts
import {
  KbDocSchema, CsProductSchema, CsOrderSchema,
  CsConversationSchema, CsMessageSchema, CsEscalationSchema,
} from "./schemas";

// ---- Knowledge base ----
export const useKbDocs = () =>
  useQuery({ queryKey: ["cs-docs"], queryFn: () => api.get("/cs/admin/docs", z.array(KbDocSchema)) });

export const useCreateDoc = () =>
  useInvalidatingMutation(
    (b: { title: string; source?: string | null; body: string }) => api.post("/cs/admin/docs", KbDocSchema, b),
    ["cs-docs"],
  );
export const useUpdateDoc = () =>
  useInvalidatingMutation(
    (a: { id: number; body: { title: string; source?: string | null; body: string } }) =>
      api.patch(`/cs/admin/docs/${a.id}`, z.unknown(), a.body),
    ["cs-docs"],
  );
export const useDeleteDoc = () =>
  useInvalidatingMutation((id: number) => api.del(`/cs/admin/docs/${id}`), ["cs-docs"]);
export const useReindexKb = () =>
  useInvalidatingMutation(() => api.post("/cs/admin/kb/reindex", z.object({ embedded: z.number() }), {}), ["cs-docs"]);

// ---- Pricing ----
export const useCsProducts = () =>
  useQuery({ queryKey: ["cs-products"], queryFn: () => api.get("/cs/admin/products", z.array(CsProductSchema)) });

type ProductBody = { name: string; description?: string | null; price?: number | null; currency?: string | null; availability?: string | null };
export const useCreateProduct = () =>
  useInvalidatingMutation((b: ProductBody) => api.post("/cs/admin/products", z.object({ id: z.number() }), b), ["cs-products"]);
export const useUpdateProduct = () =>
  useInvalidatingMutation((a: { id: number; body: ProductBody }) => api.patch(`/cs/admin/products/${a.id}`, z.unknown(), a.body), ["cs-products"]);
export const useSetProductActive = () =>
  useInvalidatingMutation((a: { id: number; active: boolean }) => api.post(`/cs/admin/products/${a.id}/active`, z.unknown(), { active: a.active }), ["cs-products"]);
export const useDeleteProduct = () =>
  useInvalidatingMutation((id: number) => api.del(`/cs/admin/products/${id}`), ["cs-products"]);

// ---- Orders ----
export const useCsOrders = () =>
  useQuery({ queryKey: ["cs-orders"], queryFn: () => api.get("/cs/admin/orders", z.array(CsOrderSchema)) });
type OrderBody = { external_ref: string; customer_name?: string | null; customer_contact?: string | null; status: string; details_json?: string | null };
export const useUpsertOrder = () =>
  useInvalidatingMutation((b: OrderBody) => api.post("/cs/admin/orders", z.unknown(), b), ["cs-orders"]);
export const useDeleteOrder = () =>
  useInvalidatingMutation((id: number) => api.del(`/cs/admin/orders/${id}`), ["cs-orders"]);

// ---- Inbox / escalations ----
export const useCsConversations = () =>
  useQuery({ queryKey: ["cs-conversations"], queryFn: () => api.get("/cs/admin/conversations", z.array(CsConversationSchema)) });
export const useCsTranscript = (id: number | null) =>
  useQuery({
    queryKey: ["cs-transcript", id],
    queryFn: () => api.get(`/cs/admin/conversations/${id}/messages`, z.array(CsMessageSchema)),
    enabled: id != null,
  });
export const useResolveConversation = () =>
  useInvalidatingMutation((id: number) => api.post(`/cs/admin/conversations/${id}/resolve`, z.unknown(), {}), ["cs-conversations"]);
export const useCsEscalations = () =>
  useQuery({ queryKey: ["cs-escalations"], queryFn: () => api.get("/cs/admin/escalations", z.array(CsEscalationSchema)) });
export const useHandleEscalation = () =>
  useInvalidatingMutation((id: number) => api.post(`/cs/admin/escalations/${id}/handle`, z.unknown(), {}), ["cs-escalations", "cs-conversations"]);
```

> **Implementer note:** confirm the existing import block already brings in `useQuery`, `api`, `z`, and `useInvalidatingMutation` (it does per the codebase). Add the schema import to the existing schema-import line rather than duplicating. If `api.del` doesn't return a parseable value, the mutation's generic is fine (it returns `unknown`).

- [ ] **Step 2: Typecheck + commit**

Run: `cd frontend && npx tsc -b 2>&1 | tail -8`

```bash
git add frontend/src/api/hooks.ts
git commit -m "feat(cs-admin): react-query hooks for CS admin endpoints"
```

---

## Task 3: Hook tests

**Files:** Create `frontend/src/api/hooks.cs.test.tsx`

- [ ] **Step 1: Write the tests** (mirror the existing `hooks.test.tsx` wrapper; mock `fetch`)

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { useCsProducts, useCsEscalations } from "./hooks";

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

afterEach(() => vi.restoreAllMocks());

function stubFetch(body: unknown) {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => body } as Response));
}

test("useCsProducts fetches and validates the product list", async () => {
  stubFetch([{ id: 1, name: "Paket A", description: null, price: 150000, currency: "IDR", availability: "ready", active: 1, updated_at: "2026-06-16T00:00:00Z" }]);
  const { result } = renderHook(() => useCsProducts(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.[0].name).toBe("Paket A");
});

test("useCsEscalations validates the escalation list", async () => {
  stubFetch([{ id: 7, conversation_id: 3, reason: "cannot_answer", summary: "needs quote", status: "open", created_at: "2026-06-16T00:00:00Z", handled_at: null }]);
  const { result } = renderHook(() => useCsEscalations(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.[0].reason).toBe("cannot_answer");
});
```

- [ ] **Step 2: Run + commit**

Run: `cd frontend && npx vitest run src/api/hooks.cs.test.tsx`
Expected: PASS.

```bash
git add frontend/src/api/hooks.cs.test.tsx
git commit -m "test(cs-admin): hook tests for products + escalations"
```

---

## Task 4: Pricing page (the CRUD exemplar)

**Files:** Create `frontend/src/pages/CsPricingPage.tsx`

> **Context:** Mirror `BudgetPage`'s structure: list-in-card + a `Dialog` create/edit form + mutation hooks with `toast`. The other three pages (Tasks 5–7) follow this exact shape — read this page and `BudgetPage` before building them.

- [ ] **Step 1: Implement**

```tsx
import { useState } from "react";
import { toast } from "sonner";
import { Dialog } from "@/components/Dialog";
import { useCsProducts, useCreateProduct, useUpdateProduct, useSetProductActive, useDeleteProduct } from "@/api/hooks";
import type { CsProduct } from "@/api/schemas";

const EMPTY = { name: "", description: "", price: "", currency: "IDR", availability: "" };

export default function CsPricingPage() {
  const products = useCsProducts();
  const create = useCreateProduct();
  const update = useUpdateProduct();
  const setActive = useSetProductActive();
  const del = useDeleteProduct();

  const [open, setOpen] = useState(false);
  const [editId, setEditId] = useState<number | null>(null);
  const [form, setForm] = useState(EMPTY);
  const set = (k: keyof typeof EMPTY) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value });

  const openCreate = () => { setEditId(null); setForm(EMPTY); setOpen(true); };
  const openEdit = (p: CsProduct) => {
    setEditId(p.id);
    setForm({ name: p.name, description: p.description ?? "", price: p.price?.toString() ?? "", currency: p.currency ?? "IDR", availability: p.availability ?? "" });
    setOpen(true);
  };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.name.trim()) { toast.error("Nama wajib diisi"); return; }
    const body = {
      name: form.name.trim(),
      description: form.description || null,
      price: form.price ? Number(form.price) : null,
      currency: form.currency || null,
      availability: form.availability || null,
    };
    const onDone = { onSuccess: () => { toast.success("Tersimpan"); setOpen(false); }, onError: (e: unknown) => toast.error((e as Error).message) };
    if (editId == null) create.mutate(body, onDone);
    else update.mutate({ id: editId, body }, onDone);
  };

  const list = products.data ?? [];

  return (
    <div className="page">
      <div className="page-head flex items-center" style={{ justifyContent: "space-between" }}>
        <h1 className="page-title">Harga / Paket</h1>
        <button className="btn btn-primary" onClick={openCreate}>Tambah</button>
      </div>
      <div className="card">
        <div style={{ padding: "8px 0" }}>
          {list.length === 0 ? (
            <div className="t-sm t-muted" style={{ padding: "16px 20px" }}>Belum ada produk.</div>
          ) : list.map((p) => (
            <div key={p.id} className="flex items-center" style={{ padding: "11px 20px", gap: 12, justifyContent: "space-between" }}>
              <div>
                <div>{p.name} {p.active ? "" : "(nonaktif)"}</div>
                <div className="t-sm t-muted">{p.currency} {p.price ?? "-"} · {p.availability ?? "-"}</div>
              </div>
              <div className="flex items-center" style={{ gap: 8 }}>
                <button className="btn btn-outline btn-sm" onClick={() => openEdit(p)}>Edit</button>
                <button className="btn btn-outline btn-sm" onClick={() => setActive.mutate({ id: p.id, active: !p.active }, { onError: (e) => toast.error((e as Error).message) })}>
                  {p.active ? "Nonaktifkan" : "Aktifkan"}
                </button>
                <button className="icon-btn" onClick={() => del.mutate(p.id, { onSuccess: () => toast.success("Dihapus"), onError: (e) => toast.error((e as Error).message) })}>✕</button>
              </div>
            </div>
          ))}
        </div>
      </div>

      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={editId == null ? "Tambah Produk" : "Edit Produk"}
        footer={<>
          <button className="btn btn-outline" onClick={() => setOpen(false)}>Batal</button>
          <button className="btn btn-primary" onClick={submit} disabled={create.isPending || update.isPending}>Simpan</button>
        </>}
      >
        <form onSubmit={submit}>
          <label className="field"><span className="field-label">Nama</span><input className="input" value={form.name} onChange={set("name")} /></label>
          <label className="field"><span className="field-label">Deskripsi</span><input className="input" value={form.description} onChange={set("description")} /></label>
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field"><span className="field-label">Harga</span><input type="number" className="input" value={form.price} onChange={set("price")} /></label>
            <label className="field"><span className="field-label">Mata uang</span><input className="input" value={form.currency} onChange={set("currency")} /></label>
          </div>
          <label className="field"><span className="field-label">Ketersediaan</span><input className="input" value={form.availability} onChange={set("availability")} /></label>
        </form>
      </Dialog>
    </div>
  );
}
```

> **Implementer note:** the exact CSS class names (`page`, `page-head`, `card`, `btn`, `field`, `input`, `icon-btn`, `t-sm`, `t-muted`) are from the existing design system — verify against `BudgetPage.tsx` and adjust to whatever that file actually uses (e.g. `card-head`, `btn-sm`). Match the real classes; don't invent.

- [ ] **Step 2: Typecheck + commit**

Run: `cd frontend && npx tsc -b 2>&1 | tail -8`

```bash
git add frontend/src/pages/CsPricingPage.tsx
git commit -m "feat(cs-admin): pricing manager page"
```

---

## Task 5: KB Docs page

**Files:** Create `frontend/src/pages/CsDocsPage.tsx`

- [ ] **Step 1: Implement** — same structure as CsPricingPage. List docs (title + truncated body), create/edit Dialog with `title`, `source`, and a `<textarea class="input">` for `body`, delete button, plus a "Reindex" button calling `useReindexKb` (toasts `embedded` count).

```tsx
import { useState } from "react";
import { toast } from "sonner";
import { Dialog } from "@/components/Dialog";
import { useKbDocs, useCreateDoc, useUpdateDoc, useDeleteDoc, useReindexKb } from "@/api/hooks";
import type { KbDoc } from "@/api/schemas";

const EMPTY = { title: "", source: "", body: "" };

export default function CsDocsPage() {
  const docs = useKbDocs();
  const create = useCreateDoc();
  const update = useUpdateDoc();
  const del = useDeleteDoc();
  const reindex = useReindexKb();

  const [open, setOpen] = useState(false);
  const [editId, setEditId] = useState<number | null>(null);
  const [form, setForm] = useState(EMPTY);

  const openCreate = () => { setEditId(null); setForm(EMPTY); setOpen(true); };
  const openEdit = (d: KbDoc) => { setEditId(d.id); setForm({ title: d.title, source: d.source ?? "", body: d.body }); setOpen(true); };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.title.trim() || !form.body.trim()) { toast.error("Judul & isi wajib"); return; }
    const body = { title: form.title.trim(), source: form.source || null, body: form.body };
    const onDone = { onSuccess: () => { toast.success("Tersimpan"); setOpen(false); }, onError: (e: unknown) => toast.error((e as Error).message) };
    if (editId == null) create.mutate(body, onDone); else update.mutate({ id: editId, body }, onDone);
  };

  const list = docs.data ?? [];
  return (
    <div className="page">
      <div className="page-head flex items-center" style={{ justifyContent: "space-between" }}>
        <h1 className="page-title">Knowledge Base</h1>
        <div className="flex items-center" style={{ gap: 8 }}>
          <button className="btn btn-outline" disabled={reindex.isPending}
            onClick={() => reindex.mutate(undefined, { onSuccess: (r) => toast.success(`Re-embed ${(r as { embedded: number }).embedded} potongan`), onError: (e) => toast.error((e as Error).message) })}>
            Reindex
          </button>
          <button className="btn btn-primary" onClick={openCreate}>Tambah</button>
        </div>
      </div>
      <div className="card"><div style={{ padding: "8px 0" }}>
        {list.length === 0 ? <div className="t-sm t-muted" style={{ padding: "16px 20px" }}>Belum ada dokumen.</div> :
          list.map((d) => (
            <div key={d.id} className="flex items-center" style={{ padding: "11px 20px", gap: 12, justifyContent: "space-between" }}>
              <div><div>{d.title}</div><div className="t-sm t-muted">{d.body.slice(0, 80)}…</div></div>
              <div className="flex items-center" style={{ gap: 8 }}>
                <button className="btn btn-outline btn-sm" onClick={() => openEdit(d)}>Edit</button>
                <button className="icon-btn" onClick={() => del.mutate(d.id, { onSuccess: () => toast.success("Dihapus"), onError: (e) => toast.error((e as Error).message) })}>✕</button>
              </div>
            </div>
          ))}
      </div></div>
      <Dialog open={open} onClose={() => setOpen(false)} title={editId == null ? "Tambah Dokumen" : "Edit Dokumen"}
        footer={<>
          <button className="btn btn-outline" onClick={() => setOpen(false)}>Batal</button>
          <button className="btn btn-primary" onClick={submit} disabled={create.isPending || update.isPending}>Simpan</button>
        </>}>
        <form onSubmit={submit}>
          <label className="field"><span className="field-label">Judul</span><input className="input" value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} /></label>
          <label className="field"><span className="field-label">Sumber (opsional)</span><input className="input" value={form.source} onChange={(e) => setForm({ ...form, source: e.target.value })} /></label>
          <label className="field"><span className="field-label">Isi</span><textarea className="input" rows={8} value={form.body} onChange={(e) => setForm({ ...form, body: e.target.value })} /></label>
        </form>
      </Dialog>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck + commit**

```bash
git add frontend/src/pages/CsDocsPage.tsx
git commit -m "feat(cs-admin): knowledge base manager page"
```

---

## Task 6: Orders page

**Files:** Create `frontend/src/pages/CsOrdersPage.tsx`

- [ ] **Step 1: Implement** — list orders (ref, status, customer); create form posts an upsert (`external_ref`, `customer_name`, `customer_contact`, `status`, `details_json`); delete button.

```tsx
import { useState } from "react";
import { toast } from "sonner";
import { Dialog } from "@/components/Dialog";
import { useCsOrders, useUpsertOrder, useDeleteOrder } from "@/api/hooks";

const EMPTY = { external_ref: "", customer_name: "", customer_contact: "", status: "", details_json: "" };

export default function CsOrdersPage() {
  const orders = useCsOrders();
  const upsert = useUpsertOrder();
  const del = useDeleteOrder();
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState(EMPTY);
  const set = (k: keyof typeof EMPTY) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.external_ref.trim() || !form.status.trim()) { toast.error("Ref & status wajib"); return; }
    upsert.mutate(
      { external_ref: form.external_ref.trim(), customer_name: form.customer_name || null, customer_contact: form.customer_contact || null, status: form.status.trim(), details_json: form.details_json || null },
      { onSuccess: () => { toast.success("Tersimpan"); setOpen(false); setForm(EMPTY); }, onError: (e) => toast.error((e as Error).message) },
    );
  };

  const list = orders.data ?? [];
  return (
    <div className="page">
      <div className="page-head flex items-center" style={{ justifyContent: "space-between" }}>
        <h1 className="page-title">Order / Booking</h1>
        <button className="btn btn-primary" onClick={() => { setForm(EMPTY); setOpen(true); }}>Tambah / Update</button>
      </div>
      <div className="card"><div style={{ padding: "8px 0" }}>
        {list.length === 0 ? <div className="t-sm t-muted" style={{ padding: "16px 20px" }}>Belum ada order.</div> :
          list.map((o) => (
            <div key={o.id} className="flex items-center" style={{ padding: "11px 20px", gap: 12, justifyContent: "space-between" }}>
              <div><div>{o.external_ref} — {o.status}</div><div className="t-sm t-muted">{o.customer_name ?? "-"} · {o.customer_contact ?? "-"}</div></div>
              <button className="icon-btn" onClick={() => del.mutate(o.id, { onSuccess: () => toast.success("Dihapus"), onError: (e) => toast.error((e as Error).message) })}>✕</button>
            </div>
          ))}
      </div></div>
      <Dialog open={open} onClose={() => setOpen(false)} title="Order"
        footer={<><button className="btn btn-outline" onClick={() => setOpen(false)}>Batal</button><button className="btn btn-primary" onClick={submit} disabled={upsert.isPending}>Simpan</button></>}>
        <form onSubmit={submit}>
          <label className="field"><span className="field-label">Ref order</span><input className="input" value={form.external_ref} onChange={set("external_ref")} /></label>
          <label className="field"><span className="field-label">Status</span><input className="input" value={form.status} onChange={set("status")} placeholder="mis. diproses / dikirim / selesai" /></label>
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field"><span className="field-label">Nama pelanggan</span><input className="input" value={form.customer_name} onChange={set("customer_name")} /></label>
            <label className="field"><span className="field-label">Kontak (email/HP)</span><input className="input" value={form.customer_contact} onChange={set("customer_contact")} /></label>
          </div>
          <label className="field"><span className="field-label">Detail (opsional)</span><input className="input" value={form.details_json} onChange={set("details_json")} /></label>
        </form>
      </Dialog>
    </div>
  );
}
```

> **Note:** the order lookup tool matches `customer_contact` exactly (case-insensitive) — make the form label clear that this is the contact the customer must quote.

- [ ] **Step 2: Typecheck + commit**

```bash
git add frontend/src/pages/CsOrdersPage.tsx
git commit -m "feat(cs-admin): orders manager page"
```

---

## Task 7: CS Inbox page

**Files:** Create `frontend/src/pages/CsInboxPage.tsx`

- [ ] **Step 1: Implement** — left: open escalations (handle button) + recent conversations (click to select); right: selected conversation transcript + a "Tandai selesai" (resolve) button.

```tsx
import { useState } from "react";
import { toast } from "sonner";
import { useCsConversations, useCsEscalations, useHandleEscalation, useCsTranscript, useResolveConversation } from "@/api/hooks";

export default function CsInboxPage() {
  const convos = useCsConversations();
  const escalations = useCsEscalations();
  const handle = useHandleEscalation();
  const resolve = useResolveConversation();
  const [selected, setSelected] = useState<number | null>(null);
  const transcript = useCsTranscript(selected);

  const convoList = convos.data ?? [];
  const escList = escalations.data ?? [];

  return (
    <div className="page">
      <h1 className="page-title">CS Inbox</h1>
      <div className="grid" style={{ gridTemplateColumns: "320px 1fr", gap: 16, alignItems: "start" }}>
        <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
          <div className="card">
            <div className="card-head"><div className="card-title">Perlu manusia ({escList.length})</div></div>
            <div style={{ padding: "8px 0" }}>
              {escList.length === 0 ? <div className="t-sm t-muted" style={{ padding: "12px 16px" }}>Tidak ada.</div> :
                escList.map((e) => (
                  <div key={e.id} style={{ padding: "10px 16px" }}>
                    <div className="t-sm">{e.summary}</div>
                    <div className="flex items-center" style={{ gap: 8, marginTop: 4 }}>
                      <button className="btn btn-outline btn-sm" onClick={() => setSelected(e.conversation_id)}>Lihat</button>
                      <button className="btn btn-primary btn-sm" onClick={() => handle.mutate(e.id, { onSuccess: () => toast.success("Ditandai ditangani"), onError: (err) => toast.error((err as Error).message) })}>Tangani</button>
                    </div>
                  </div>
                ))}
            </div>
          </div>
          <div className="card">
            <div className="card-head"><div className="card-title">Percakapan</div></div>
            <div style={{ padding: "8px 0" }}>
              {convoList.map((c) => (
                <button key={c.id} className="flex items-center" style={{ width: "100%", textAlign: "left", padding: "10px 16px", gap: 8, justifyContent: "space-between", background: selected === c.id ? "var(--surface-2, #f1f5f9)" : "transparent", border: "none", cursor: "pointer" }} onClick={() => setSelected(c.id)}>
                  <span>{c.visitor_name ?? `#${c.id}`}</span>
                  <span className="t-sm t-muted">{c.status}</span>
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="card">
          <div className="card-head flex items-center" style={{ justifyContent: "space-between" }}>
            <div className="card-title">Transkrip {selected ? `#${selected}` : ""}</div>
            {selected && <button className="btn btn-outline btn-sm" onClick={() => resolve.mutate(selected, { onSuccess: () => toast.success("Diselesaikan"), onError: (e) => toast.error((e as Error).message) })}>Tandai selesai</button>}
          </div>
          <div style={{ padding: 16, display: "flex", flexDirection: "column", gap: 8 }}>
            {selected == null ? <div className="t-sm t-muted">Pilih percakapan.</div> :
              (transcript.data ?? []).map((m) => (
                <div key={m.id} style={{ alignSelf: m.role === "user" ? "flex-start" : "flex-end", maxWidth: "80%", background: m.role === "user" ? "#f1f5f9" : "#2563eb", color: m.role === "user" ? "#111" : "#fff", padding: "8px 10px", borderRadius: 10, whiteSpace: "pre-wrap" }}>
                  {m.content}
                </div>
              ))}
          </div>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Typecheck + commit**

```bash
git add frontend/src/pages/CsInboxPage.tsx
git commit -m "feat(cs-admin): CS inbox page (conversations + transcript + escalations)"
```

---

## Task 8: Routes + nav

**Files:** Modify `frontend/src/App.tsx`, `frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Add routes** — in `App.tsx`, import the 4 pages and add inside the `AppShell` route group:

```tsx
<Route path="cs/admin/docs" element={<CsDocsPage />} />
<Route path="cs/admin/pricing" element={<CsPricingPage />} />
<Route path="cs/admin/orders" element={<CsOrdersPage />} />
<Route path="cs/admin/inbox" element={<CsInboxPage />} />
```
Use the same import style as the other page imports (the codebase may lazy-load or direct-import — match it).

- [ ] **Step 2: Add nav group** — in `AppShell.tsx`'s `NAV_GROUPS`, add a group (pick icons already imported from `lucide-react`, or import suitable ones the file doesn't yet use):

```tsx
{
  title: "Admin (CS)",
  items: [
    { to: "/cs/admin/inbox",   label: "CS Inbox",       icon: Inbox },
    { to: "/cs/admin/docs",    label: "Knowledge Base", icon: BookOpen },
    { to: "/cs/admin/pricing", label: "Harga",          icon: Tag },
    { to: "/cs/admin/orders",  label: "Order",          icon: Package },
  ],
},
```
> **Implementer note:** verify which icons are already imported in `AppShell.tsx`; reuse those or add imports from `lucide-react` for any new ones (`BookOpen`, `Tag`, `Package`). Match the existing `NavItem` shape exactly.

- [ ] **Step 3: Full verification**

Run: `cd frontend && npx tsc -b 2>&1 | tail -12 && npx vitest run src/api/hooks.cs.test.tsx && npm run build 2>&1 | tail -6`
Expected: no type errors; hook tests pass; full build (SPA + widget) succeeds.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/AppShell.tsx
git commit -m "feat(cs-admin): routes + Admin (CS) nav group"
```

---

## Self-Review

**Spec coverage (spec §10 admin — frontend half):**
- KB manager (CRUD + reindex) ✓ Task 5. Pricing manager ✓ Task 4. Orders manager ✓ Task 6. CS Inbox (conversations + transcript + escalations + resolve/handle) ✓ Task 7.
- All under authenticated SPA routes + nav ✓ Task 8.
- Reuses `schemas.ts`/`hooks.ts`/`Dialog`/`toast` conventions ✓ Tasks 1–2.

**Placeholder scan:** No TBD/TODO. Notes instruct verifying real CSS class names + icon imports against the existing codebase (not inventing) — required because exact class/icon names must match the design system.

**Type consistency:** Schemas (`KbDoc`, `CsProduct`, `CsOrder`, `CsConversation`, `CsMessage`, `CsEscalation`) match Plan 4a's serialized row types (incl. `active: number` for SQLite 0/1, nullable fields). Hook names/bodies match page call sites. Mutation bodies match Plan 4a's `Deserialize` structs (`{title,source,body}`, `{name,...}`, `{external_ref,...}`, `{active}`).

---

## Downstream / done

After this plan, Phase 1 is complete: brain + public widget + admin UI. Remaining (separate efforts): Plan 2.5 (Upwork `get_project_status`), Phase 2 (WhatsApp CS number + per-contact routing + WA proactive send). To go live: set `CS_ALLOWED_ORIGINS`, `CS_WIDGET_KEY`, `OPENAI_API_KEY`; add KB docs + pricing + orders via this admin UI; embed the widget snippet.
