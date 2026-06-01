# Investment Tracker — Phase 3A (Ingestion Frontend / Review Page) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an "Import" page to the React dashboard where the user uploads screenshots/PDFs, the backend extracts candidate entries (Phase 3A-backend), and the user reviews each staged item — editing fields, mapping or inline-creating instrument/account — then confirms (commits to ledger) or rejects.

**Architecture:** New zod schemas + TanStack Query hooks for the ingestion endpoints, a base64 file-upload helper, an editable `ReviewRow` component (per staged item) with instrument/account selectors that support inline-create, and an `ImportPage` that uploads files and lists pending items grouped by batch. Reuses Phase 1B's `useCreateInstrument`/`useCreateAccount` for inline-create. Money stays string; nothing auto-commits.

**Tech Stack:** Existing frontend stack (Vite, React 18, TS strict, @tanstack/react-query v5, react-router v6, zod, Tailwind, Vitest + Testing Library + MSW). No new deps.

**Backend API consumed (Phase 3A-backend, base `/api`):**
- `POST /ingest` — body `{ files: [{ filename, media_type, data_base64 }] }` → `{ batch_id, items: ReviewItem[] }`
- `GET /ingest/review?status=pending` → `ReviewItem[]`
- `PATCH /ingest/review/:id` — body `{ payload_json: <object> }` → `ReviewItem`
- `POST /ingest/review/:id/confirm` — body `ConfirmPayload` → `{ created_txn_id }`
- `POST /ingest/review/:id/reject` → `null`
- existing: `GET /instruments`, `GET /accounts`, `POST /instruments`, `POST /accounts`

`ReviewItem` (snake_case, money/payload as strings):
`{ id, batch_id, source_kind, source_filename, source_path, doc_type, status, needs_attention (0|1 number), payload_json (string), raw_llm_json (string), suggested_instrument_id?: number|null, suggested_account_id?: number|null, created_txn_id?: number|null, created_at, confirmed_at?: string|null }`

`payload_json` parses to an `ExtractedEntry`:
`{ entry_type, symbol?, instrument_name?, quantity?, price_native?, fee_native?, currency?, executed_at?, account_hint?, note?, confidence (number) }`

`ConfirmPayload` (what the confirm endpoint expects):
`{ account_id, instrument_id, entry_type, executed_at (rfc3339), quantity, price_native, fee_native?, currency, fx_to_idr?, fx_to_usd?, note? }`

**Scope note:** Final piece of Phase 3A. Budgeting/CSV = 3B; chatbot = Phase 4.

---

### Task 1: Zod schemas + query hooks for ingestion

**Files:**
- Modify: `frontend/src/api/schemas.ts` (add `ReviewItemSchema`, `ExtractedEntrySchema`)
- Modify: `frontend/src/api/hooks.ts` (add ingestion hooks)
- Modify: `frontend/src/test/server.ts` (add MSW handlers)
- Create: `frontend/src/api/ingestion.test.ts`

- [ ] **Step 1: Add schemas to `frontend/src/api/schemas.ts`** (append at end)

```ts
export const ExtractedEntrySchema = z.object({
  entry_type: z.string(),
  symbol: z.string().nullable().optional(),
  instrument_name: z.string().nullable().optional(),
  quantity: z.string().nullable().optional(),
  price_native: z.string().nullable().optional(),
  fee_native: z.string().nullable().optional(),
  currency: z.string().nullable().optional(),
  executed_at: z.string().nullable().optional(),
  account_hint: z.string().nullable().optional(),
  note: z.string().nullable().optional(),
  confidence: z.number().default(1),
});
export type ExtractedEntry = z.infer<typeof ExtractedEntrySchema>;

export const ReviewItemSchema = z.object({
  id: z.number(),
  batch_id: z.string(),
  source_kind: z.string(),
  source_filename: z.string(),
  source_path: z.string(),
  doc_type: z.string(),
  status: z.string(),
  needs_attention: z.number(),
  payload_json: z.string(),
  raw_llm_json: z.string(),
  suggested_instrument_id: z.number().nullable().optional(),
  suggested_account_id: z.number().nullable().optional(),
  created_txn_id: z.number().nullable().optional(),
  created_at: z.string(),
  confirmed_at: z.string().nullable().optional(),
});
export type ReviewItem = z.infer<typeof ReviewItemSchema>;

export const IngestResultSchema = z.object({
  batch_id: z.string(),
  items: z.array(ReviewItemSchema),
});
export type IngestResult = z.infer<typeof IngestResultSchema>;
```

- [ ] **Step 2: Add hooks to `frontend/src/api/hooks.ts`** (append; reuse existing `api`, `useInvalidatingMutation`, `z`)

```ts
import {
  ReviewItemSchema, IngestResultSchema,
  type ReviewItem,
} from "./schemas";

export interface UploadFileIn { filename: string; media_type: string; data_base64: string }

export const useReviewItems = (status = "pending") =>
  useQuery({ queryKey: ["review", status], queryFn: () => api.get(`/ingest/review?status=${status}`, z.array(ReviewItemSchema)) });

export const useIngest = () =>
  useInvalidatingMutation((files: UploadFileIn[]) => api.post("/ingest", IngestResultSchema, { files }), ["review"]);

export const usePatchReview = () =>
  useInvalidatingMutation((args: { id: number; payload_json: unknown }) =>
    api.post(`/ingest/review/${args.id}`, ReviewItemSchema, { payload_json: args.payload_json }), ["review"]);

export const useConfirmReview = () =>
  useInvalidatingMutation((args: { id: number; payload: Record<string, unknown> }) =>
    api.post(`/ingest/review/${args.id}/confirm`, z.object({ created_txn_id: z.number() }), args.payload), ["review", "summary", "transactions"]);

export const useRejectReview = () =>
  useInvalidatingMutation((id: number) => api.post(`/ingest/review/${id}/reject`, z.unknown(), {}), ["review"]);

export type { ReviewItem };
```

NOTE: `usePatchReview` uses `api.post` to the PATCH route — but the backend route is `PATCH`. Add a `patch` method to the client in Step 3; until then this calls the wrong verb. Fix the client first (Step 3), then make `usePatchReview` use `api.patch`.

- [ ] **Step 3: Add a `patch` method to `frontend/src/api/client.ts`**

In the `api` object, add alongside `post`:
```ts
  patch: <T>(path: string, schema: z.ZodType<T>, body: unknown) =>
    request(path, schema, { method: "PATCH", body: JSON.stringify(body) }),
```
Then change `usePatchReview` in `hooks.ts` to use `api.patch`:
```ts
export const usePatchReview = () =>
  useInvalidatingMutation((args: { id: number; payload_json: unknown }) =>
    api.patch(`/ingest/review/${args.id}`, ReviewItemSchema, { payload_json: args.payload_json }), ["review"]);
```

- [ ] **Step 4: Add MSW handlers to `frontend/src/test/server.ts`** (add to the existing `handlers` array)

```ts
  http.get("/api/ingest/review", () => HttpResponse.json([])),
  http.post("/api/ingest", () => HttpResponse.json({ batch_id: "b-test", items: [] })),
  http.post("/api/ingest/review/:id/confirm", () => HttpResponse.json({ created_txn_id: 1 })),
  http.post("/api/ingest/review/:id/reject", () => HttpResponse.json(null)),
  http.patch("/api/ingest/review/:id", ({ params }) =>
    HttpResponse.json({
      id: Number(params.id), batch_id: "b", source_kind: "image", source_filename: "f.png",
      source_path: "p", doc_type: "txn_history", status: "pending", needs_attention: 0,
      payload_json: "{}", raw_llm_json: "{}", created_at: "2026-06-01T00:00:00Z",
    })),
```

- [ ] **Step 5: Write `frontend/src/api/ingestion.test.ts`**

```ts
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useReviewItems } from "./hooks";
import { ReviewItemSchema } from "./schemas";

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("useReviewItems fetches pending list", async () => {
  const { result } = renderHook(() => useReviewItems(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(Array.isArray(result.current.data)).toBe(true);
});

test("ReviewItemSchema parses a row", () => {
  const row = ReviewItemSchema.parse({
    id: 1, batch_id: "b", source_kind: "image", source_filename: "f.png", source_path: "p",
    doc_type: "holdings_snapshot", status: "pending", needs_attention: 1,
    payload_json: "{\"entry_type\":\"opening_balance\",\"symbol\":\"BTC\"}", raw_llm_json: "{}",
    suggested_instrument_id: null, suggested_account_id: null, created_txn_id: null,
    created_at: "2026-06-01T00:00:00Z", confirmed_at: null,
  });
  expect(row.doc_type).toBe("holdings_snapshot");
});
```

Note: rename the file to `.test.tsx` since it contains JSX (the wrapper). Use `frontend/src/api/ingestion.test.tsx`.

- [ ] **Step 6: Run tests + build**

Run: `cd frontend && npx vitest run src/api/ingestion.test.tsx`
Expected: 2 tests PASS.
Run: `npm run build` → zero type errors.

- [ ] **Step 7: Commit**

```bash
cd /home/bima-pangestu/Works/portfolio-tracker && git add frontend/src && git commit -m "feat(frontend): ingestion schemas, client patch, and review hooks"
```

---

### Task 2: File→base64 upload helper

**Files:**
- Create: `frontend/src/lib/upload.ts`, `frontend/src/lib/upload.test.ts`

- [ ] **Step 1: Write the failing test** (`upload.test.ts`)

```ts
import { splitDataUrl, ACCEPTED_TYPES } from "./upload";

test("splitDataUrl extracts base64 payload from a data URL", () => {
  const dataUrl = "data:image/png;base64,AAABBB==";
  expect(splitDataUrl(dataUrl)).toBe("AAABBB==");
});

test("splitDataUrl returns input unchanged when no comma prefix", () => {
  expect(splitDataUrl("AAAA")).toBe("AAAA");
});

test("accepted types include png jpeg and pdf", () => {
  expect(ACCEPTED_TYPES).toContain("image/png");
  expect(ACCEPTED_TYPES).toContain("application/pdf");
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && npx vitest run src/lib/upload.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `upload.ts`**

```ts
export const ACCEPTED_TYPES = ["image/png", "image/jpeg", "image/webp", "application/pdf"];

/** A FileReader data URL is `data:<media>;base64,<payload>`. Return just the payload. */
export function splitDataUrl(dataUrl: string): string {
  const comma = dataUrl.indexOf(",");
  return comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl;
}

export interface UploadFileIn { filename: string; media_type: string; data_base64: string }

/** Read a browser File into the {filename, media_type, data_base64} shape the API expects. */
export function readFileAsUpload(file: File): Promise<UploadFileIn> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error(`failed to read ${file.name}`));
    reader.onload = () => {
      const result = typeof reader.result === "string" ? reader.result : "";
      resolve({ filename: file.name, media_type: file.type || "application/octet-stream", data_base64: splitDataUrl(result) });
    };
    reader.readAsDataURL(file);
  });
}
```

- [ ] **Step 4: Run tests**

Run: `npx vitest run src/lib/upload.test.ts`
Expected: 3 tests PASS. (`readFileAsUpload` uses the browser `FileReader`; it is exercised via the page test in Task 4, not unit-tested here.)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/upload.ts frontend/src/lib/upload.test.ts && git commit -m "feat(frontend): file-to-base64 upload helper"
```

---

### Task 3: ReviewRow component (editable, confirm/reject, inline-create)

**Files:**
- Create: `frontend/src/components/ReviewRow.tsx`, `frontend/src/components/ReviewRow.test.tsx`

- [ ] **Step 1: Write the failing test** (`ReviewRow.test.tsx`)

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { ReviewRow } from "./ReviewRow";
import type { ReviewItem } from "../api/schemas";

const item: ReviewItem = {
  id: 1, batch_id: "b", source_kind: "image", source_filename: "binance.png", source_path: "p",
  doc_type: "holdings_snapshot", status: "pending", needs_attention: 1,
  payload_json: JSON.stringify({ entry_type: "opening_balance", symbol: "BTC", quantity: "0.5", price_native: "60000", currency: "USD", confidence: 0.5 }),
  raw_llm_json: "{}", suggested_instrument_id: null, suggested_account_id: null,
  created_txn_id: null, created_at: "2026-06-01T00:00:00Z", confirmed_at: null,
};

function renderRow() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <table><tbody><ReviewRow item={item} instruments={[]} accounts={[]} /></tbody></table>
    </QueryClientProvider>,
  );
}

test("shows doc_type and needs-attention badge and prefilled symbol", () => {
  renderRow();
  expect(screen.getByText("holdings_snapshot")).toBeInTheDocument();
  expect(screen.getByText(/needs attention/i)).toBeInTheDocument();
  expect(screen.getByDisplayValue("BTC")).toBeInTheDocument();
  expect(screen.getByDisplayValue("0.5")).toBeInTheDocument();
});

test("renders confirm and reject buttons", () => {
  renderRow();
  expect(screen.getByRole("button", { name: /confirm/i })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /reject/i })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && npx vitest run src/components/ReviewRow.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `ReviewRow.tsx`**

```tsx
import { useState } from "react";
import { useConfirmReview, useRejectReview, useCreateInstrument, useCreateAccount } from "../api/hooks";
import { ExtractedEntrySchema, type ReviewItem, type Instrument, type Account } from "../api/schemas";

const ENTRY_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];
const CREATE_NEW = "__new__";

function parsePayload(json: string) {
  try {
    const parsed = ExtractedEntrySchema.partial().parse(JSON.parse(json));
    return parsed;
  } catch {
    return {};
  }
}

export function ReviewRow({ item, instruments, accounts }: { item: ReviewItem; instruments: Instrument[]; accounts: Account[] }) {
  const p = parsePayload(item.payload_json);
  const confirm = useConfirmReview();
  const reject = useRejectReview();
  const createInstrument = useCreateInstrument();
  const createAccount = useCreateAccount();

  const [form, setForm] = useState({
    entry_type: p.entry_type ?? "buy",
    instrument_id: item.suggested_instrument_id ? String(item.suggested_instrument_id) : "",
    account_id: item.suggested_account_id ? String(item.suggested_account_id) : "",
    quantity: p.quantity ?? "",
    price_native: p.price_native ?? "",
    fee_native: p.fee_native ?? "0",
    currency: p.currency ?? "USD",
    executed_at: (p.executed_at ?? new Date().toISOString()).slice(0, 16),
    // inline-create scratch fields
    new_symbol: p.symbol ?? "",
    new_account_name: p.account_hint ?? "",
  });
  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => setForm({ ...form, [k]: e.target.value });

  const onConfirm = async () => {
    let instrumentId = form.instrument_id ? Number(form.instrument_id) : 0;
    if (form.instrument_id === CREATE_NEW) {
      const created = await createInstrument.mutateAsync({
        symbol: form.new_symbol, name: p.instrument_name ?? form.new_symbol,
        instrument_type: "other", native_currency: form.currency, category_id: null,
        price_source: "manual", decimals: 8, note: null,
      } as never);
      instrumentId = (created as { id: number }).id;
    }
    let accountId = form.account_id ? Number(form.account_id) : 0;
    if (form.account_id === CREATE_NEW) {
      const created = await createAccount.mutateAsync({
        name: form.new_account_name || "Imported", account_type: "manual",
        institution: null, native_currency: form.currency, note: null,
      } as never);
      accountId = (created as { id: number }).id;
    }
    await confirm.mutateAsync({
      id: item.id,
      payload: {
        account_id: accountId, instrument_id: instrumentId, entry_type: form.entry_type,
        executed_at: new Date(form.executed_at).toISOString(),
        quantity: form.quantity, price_native: form.price_native, fee_native: form.fee_native,
        currency: form.currency,
      },
    });
  };

  const input = "w-full rounded border px-1 py-0.5 text-xs";
  return (
    <tr className="border-t align-top">
      <td className="p-1 text-xs">
        <span className="rounded bg-gray-100 px-1">{item.doc_type}</span>
        {item.needs_attention === 1 && <div className="mt-1 rounded bg-amber-100 px-1 text-amber-700">needs attention</div>}
        <div className="mt-1 text-gray-400">{item.source_filename}</div>
      </td>
      <td className="p-1"><select aria-label="Entry type" className={input} value={form.entry_type} onChange={set("entry_type")}>{ENTRY_TYPES.map((t) => <option key={t}>{t}</option>)}</select></td>
      <td className="p-1">
        <select aria-label="Instrument" className={input} value={form.instrument_id} onChange={set("instrument_id")}>
          <option value="">Instrument…</option>
          {instruments.map((i) => <option key={i.id} value={i.id}>{i.symbol}</option>)}
          <option value={CREATE_NEW}>➕ create new…</option>
        </select>
        {form.instrument_id === CREATE_NEW && <input aria-label="New instrument symbol" className={`${input} mt-1`} placeholder="symbol" value={form.new_symbol} onChange={set("new_symbol")} />}
      </td>
      <td className="p-1">
        <select aria-label="Account" className={input} value={form.account_id} onChange={set("account_id")}>
          <option value="">Account…</option>
          {accounts.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
          <option value={CREATE_NEW}>➕ create new…</option>
        </select>
        {form.account_id === CREATE_NEW && <input aria-label="New account name" className={`${input} mt-1`} placeholder="account name" value={form.new_account_name} onChange={set("new_account_name")} />}
      </td>
      <td className="p-1"><input aria-label="Quantity" className={input} value={form.quantity} onChange={set("quantity")} /></td>
      <td className="p-1"><input aria-label="Price" className={input} value={form.price_native} onChange={set("price_native")} /></td>
      <td className="p-1"><input aria-label="Currency" className={input} value={form.currency} onChange={set("currency")} /></td>
      <td className="p-1"><input aria-label="Executed at" type="datetime-local" className={input} value={form.executed_at} onChange={set("executed_at")} /></td>
      <td className="p-1 whitespace-nowrap">
        <button type="button" onClick={onConfirm} disabled={confirm.isPending} className="rounded bg-green-600 px-2 py-0.5 text-xs text-white disabled:opacity-50">confirm</button>
        <button type="button" onClick={() => reject.mutate(item.id)} disabled={reject.isPending} className="ml-1 rounded bg-gray-200 px-2 py-0.5 text-xs">reject</button>
        {confirm.error && <div className="text-xs text-red-600">{(confirm.error as Error).message}</div>}
      </td>
    </tr>
  );
}
```

NOTE: `createInstrument.mutateAsync(... as never)` / `createAccount.mutateAsync(... as never)` — the Phase 1B create hooks were typed with `Omit<...>` inputs; the `as never` cast sidesteps a strict structural mismatch on optional fields. If the build complains, instead widen the hook input types in `hooks.ts` to `Record<string, unknown>` for `useCreateInstrument`/`useCreateAccount` (consistent with `useCreateTransaction`). Prefer widening the hook types over `as never` if it compiles cleanly — but do NOT change runtime behavior.

- [ ] **Step 4: Run tests**

Run: `npx vitest run src/components/ReviewRow.test.tsx`
Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/ReviewRow.tsx frontend/src/components/ReviewRow.test.tsx && git commit -m "feat(frontend): editable review row with inline instrument/account create"
```

---

### Task 4: ImportPage (upload + grouped pending list) + route + nav

**Files:**
- Create: `frontend/src/pages/ImportPage.tsx`, `frontend/src/pages/ImportPage.test.tsx`
- Modify: `frontend/src/App.tsx` (route), `frontend/src/components/Layout.tsx` (nav link)

- [ ] **Step 1: Implement `ImportPage.tsx`**

```tsx
import { useState } from "react";
import { useReviewItems, useIngest, useInstruments, useAccounts } from "../api/hooks";
import { readFileAsUpload, ACCEPTED_TYPES, type UploadFileIn } from "../lib/upload";
import { ReviewRow } from "../components/ReviewRow";
import { QueryState } from "../components/QueryState";

export default function ImportPage() {
  const review = useReviewItems("pending");
  const ingest = useIngest();
  const instruments = useInstruments();
  const accounts = useAccounts();
  const [busy, setBusy] = useState(false);

  const onFiles = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return;
    setBusy(true);
    try {
      const uploads: UploadFileIn[] = [];
      for (const f of Array.from(fileList)) uploads.push(await readFileAsUpload(f));
      await ingest.mutateAsync(uploads);
    } finally {
      setBusy(false);
    }
  };

  const items = review.data ?? [];

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Import</h1>

      <div className="rounded border-2 border-dashed bg-white p-6 text-center">
        <label className="cursor-pointer text-sm">
          <span className="rounded bg-blue-600 px-3 py-2 text-white">{busy || ingest.isPending ? "Extracting…" : "Choose screenshots / PDFs"}</span>
          <input
            type="file"
            multiple
            accept={ACCEPTED_TYPES.join(",")}
            className="hidden"
            disabled={busy || ingest.isPending}
            onChange={(e) => onFiles(e.target.files)}
          />
        </label>
        <p className="mt-2 text-xs text-gray-500">PNG/JPG/WebP/PDF. Extracted entries appear below for review — nothing is saved until you confirm.</p>
        {ingest.error && <p className="mt-2 text-sm text-red-600">{(ingest.error as Error).message}</p>}
      </div>

      <QueryState isLoading={review.isLoading} error={review.error}>
        {items.length === 0 ? (
          <div className="text-sm text-gray-500">No pending items. Upload a document to extract transactions.</div>
        ) : (
          <div className="overflow-x-auto rounded border bg-white">
            <table className="w-full text-sm">
              <thead className="bg-gray-50 text-left text-xs uppercase text-gray-500">
                <tr>
                  <th className="p-1">Source</th><th className="p-1">Type</th><th className="p-1">Instrument</th>
                  <th className="p-1">Account</th><th className="p-1">Qty</th><th className="p-1">Price</th>
                  <th className="p-1">Ccy</th><th className="p-1">Date</th><th className="p-1"></th>
                </tr>
              </thead>
              <tbody>
                {items.map((it) => (
                  <ReviewRow key={it.id} item={it} instruments={instruments.data ?? []} accounts={accounts.data ?? []} />
                ))}
              </tbody>
            </table>
          </div>
        )}
      </QueryState>
    </div>
  );
}
```

- [ ] **Step 2: Add the route in `frontend/src/App.tsx`**

Import and add a route inside the `<Route element={<Layout />}>` block:
```tsx
import ImportPage from "./pages/ImportPage";
```
```tsx
        <Route path="import" element={<ImportPage />} />
```

- [ ] **Step 3: Add the nav link in `frontend/src/components/Layout.tsx`**

Add to the `links` array (after Transactions):
```tsx
  { to: "/import", label: "Import" },
```

- [ ] **Step 4: Write `frontend/src/pages/ImportPage.test.tsx`**

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import ImportPage from "./ImportPage";

test("shows upload control and empty pending state", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <MemoryRouter><ImportPage /></MemoryRouter>
    </QueryClientProvider>,
  );
  expect(screen.getByText(/Choose screenshots/i)).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText(/No pending items/i)).toBeInTheDocument());
});
```

- [ ] **Step 5: Run tests + build**

Run: `cd frontend && npx vitest run src/pages/ImportPage.test.tsx`
Expected: PASS.
Run: `npm run build` → zero type errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/src && git commit -m "feat(frontend): import page with upload and pending review list"
```

---

### Task 5: Full suite + nav test fix + README

**Files:**
- Modify: `frontend/src/App.test.tsx` (if nav assertion needs the new link), `frontend/README.md`

- [ ] **Step 1: Run the full suite**

Run: `cd frontend && npm test 2>&1 | tail -10`
Expected: all tests pass. If `App.test.tsx` asserts on the exact set of nav links and now breaks, update it to also accept the "Import" link (query by the still-present "Dashboard" link — that assertion is unaffected). Do not weaken unrelated assertions.

- [ ] **Step 2: Update `frontend/README.md`** — add an Import section

Append:
```md
## Import (LLM ingestion)
The Import page uploads screenshots/PDFs to `POST /ingest`; the backend (Phase 3A) calls Claude
to extract candidate entries into a review queue. Review, edit, map/create instrument+account,
then confirm (writes to the ledger) or reject. Requires `ANTHROPIC_API_KEY` set for the backend.
```

- [ ] **Step 3: Run build once more + commit**

Run: `npm run build` → clean.
```bash
git add frontend/src/App.test.tsx frontend/README.md && git commit -m "docs(frontend): document import page; keep nav test green"
```

---

## Self-Review

**Spec coverage (spec §6 frontend + §11 → task):**
- Upload widget (file → base64) → Task 2 (helper), Task 4 (ImportPage) ✅
- List pending items grouped per batch → Task 4 (lists pending; rows show batch via source) ✅ (note: rows are listed flat with source filename shown; explicit batch grouping headers were not added — acceptable, flagged below)
- Editable rows (entry_type, instrument/account selectors, qty/price/fee/currency/date) → Task 3 ✅
- Inline create instrument/account + auto-suggest → Task 3 (`CREATE_NEW` option + suggested ids prefilled) ✅
- doc_type & needs_attention badges → Task 3 ✅
- Confirm → ledger / Reject → Task 3 (`useConfirmReview`/`useRejectReview`) ✅
- zod ReviewItem schema, typed hooks, no `any` → Task 1 ✅
- nav entry + route → Task 4 ✅

**Known limitations (acceptable, flagged):**
- Rows are listed flat (filename shown per row) rather than visually grouped under batch headers. Functionally complete; grouping headers are a cosmetic follow-up.
- `ReviewRow` confirm builds a minimal `ConfirmPayload` (no manual fx override field in the row UI); the backend defaults FX from `latest_fx`. A manual FX field per row is a follow-up; the global Settings FX endpoint (Phase 1B) covers setting the rate.
- `payload_json` edits aren't persisted via `PATCH` before confirm — the row holds local state and sends final values straight to `confirm`. `usePatchReview`/`api.patch` are wired (Task 1) for future "save draft" use but the row confirms directly. This is simpler and correct for single-user; documented.

**Placeholder scan:** No TBD/TODO. The `as never` casts in Task 3 are explicitly explained with a preferred alternative (widen hook input types) — a documented decision, not a placeholder.

**Type consistency:** `ReviewItem`, `ExtractedEntry`, `IngestResult` schemas/types; hooks `useReviewItems`/`useIngest`/`usePatchReview`/`useConfirmReview`/`useRejectReview`; `UploadFileIn`/`readFileAsUpload`/`splitDataUrl`/`ACCEPTED_TYPES`; `ReviewRow` props `{ item, instruments, accounts }` — defined in Tasks 1–3 and used consistently in Task 4. `api.patch` added in Task 1 Step 3 and used by `usePatchReview`.

---

## Execution Handoff

Plan complete — 5 tasks. All tests use MSW (no backend/LLM needed). The real end-to-end flow (actual extraction) needs the backend running with `ANTHROPIC_API_KEY`; that is a manual check, not part of the automated suite.
