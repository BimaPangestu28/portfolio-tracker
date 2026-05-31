# Investment Tracker — Phase 1B (Frontend Dashboard) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a self-hosted React + TypeScript dashboard that consumes the Phase 1A backend REST API and shows consolidated net worth (dual IDR+USD), performance (ROI + XIRR + realized/unrealized), allocation vs target with drift, value history, plus CRUD for accounts/instruments/categories/transactions and manual price/FX entry.

**Architecture:** Vite + React 18 + TypeScript (strict). Server state via TanStack Query; every API response validated through a zod schema at the boundary (no `any`). Money/quantity values arrive as decimal strings — kept as strings in state, parsed only for display/charts. Charts via Recharts. Routing via react-router. Styling via Tailwind. Pure logic (formatters, schema parsing) is unit-tested; components get render/interaction tests with Vitest + Testing Library + MSW.

**Tech Stack:** Vite, React 18, TypeScript strict, @tanstack/react-query v5, react-router-dom v6, zod, recharts, tailwindcss, vitest, @testing-library/react, @testing-library/user-event, msw.

**Backend API contract (Phase 1A, base `http://localhost:8080`):**
- `GET/POST /accounts`, `DELETE /accounts/:id` — `AccountRow { id, name, account_type, institution?, native_currency, note?, created_at }`
- `GET/POST /categories`, `DELETE /categories/:id` — `CategoryRow { id, name, target_pct: string, tolerance_band_pct?: string, sort_order, color? }`
- `GET/POST /instruments`, `DELETE /instruments/:id` — `InstrumentRow { id, symbol, name, instrument_type, native_currency, category_id?, price_source, decimals, note? }`
- `GET/POST /transactions`, `DELETE /transactions/:id` — `Transaction { id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note? }` (all money fields are strings; `txn_type` is snake_case e.g. `"buy"`)
- `POST /prices/manual` — `{ instrument_id, price, currency, as_of }`
- `POST /fx/manual` — `{ base, quote, rate, as_of }`
- `POST /prices/refresh` — no body
- `GET /portfolio/summary` — `{ net_worth_idr, net_worth_usd, total_unrealized_pnl_idr, total_realized_pnl_idr, xirr: number|null, positions: Position[], allocation: CategoryAllocation[] }`
  - `Position { instrument_id, quantity, avg_cost, cost_basis_total, latest_price, price_stale, market_value_native, market_value_idr, market_value_usd, unrealized_pnl, realized_pnl, income }` (money = string)
  - `CategoryAllocation { category_id, name, target_pct, tolerance_band_pct?, actual_pct, actual_value_idr, drift_pct, out_of_band, rebalance_idr }` (money = string)
- `GET /portfolio/history` — `SnapshotRow[] { as_of, total_idr, total_usd, breakdown_json }`

**Scope note:** Phase 1B is the final piece of Phase 1. Auto-sync, LLM ingestion/budgeting, and chatbot are Phases 2–4.

---

### Task 1: Scaffold Vite + React + TS + Tailwind + Vitest

**Files:** Create under `frontend/`: `package.json`, `tsconfig.json`, `tsconfig.node.json`, `vite.config.ts`, `index.html`, `tailwind.config.js`, `postcss.config.js`, `src/main.tsx`, `src/App.tsx`, `src/index.css`, `src/test/setup.ts`.

- [ ] **Step 1: Create `frontend/package.json`**

```json
{
  "name": "portfolio-tracker-frontend",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "dependencies": {
    "@tanstack/react-query": "^5.51.0",
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-router-dom": "^6.26.0",
    "recharts": "^2.12.7",
    "zod": "^3.23.8"
  },
  "devDependencies": {
    "@testing-library/jest-dom": "^6.4.8",
    "@testing-library/react": "^16.0.0",
    "@testing-library/user-event": "^14.5.2",
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "autoprefixer": "^10.4.19",
    "jsdom": "^24.1.1",
    "msw": "^2.3.5",
    "postcss": "^8.4.40",
    "tailwindcss": "^3.4.7",
    "typescript": "^5.5.4",
    "vite": "^5.3.5",
    "vitest": "^2.0.5"
  }
}
```

- [ ] **Step 2: Create `frontend/tsconfig.json`** (strict)

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "types": ["vitest/globals", "@testing-library/jest-dom"]
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

- [ ] **Step 3: Create `frontend/tsconfig.node.json`**

```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "strict": true
  },
  "include": ["vite.config.ts"]
}
```

- [ ] **Step 4: Create `frontend/vite.config.ts`** (dev proxy to backend + vitest config)

```ts
/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      "/api": { target: "http://localhost:8080", changeOrigin: true, rewrite: (p) => p.replace(/^\/api/, "") },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
});
```

- [ ] **Step 5: Create the remaining scaffold files**

`frontend/index.html`:
```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Portfolio Tracker</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`frontend/tailwind.config.js`:
```js
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: { extend: {} },
  plugins: [],
};
```

`frontend/postcss.config.js`:
```js
export default { plugins: { tailwindcss: {}, autoprefixer: {} } };
```

`frontend/src/index.css`:
```css
@tailwind base;
@tailwind components;
@tailwind utilities;
```

`frontend/src/App.tsx`:
```tsx
export default function App() {
  return <div className="p-6 text-lg font-semibold">Portfolio Tracker</div>;
}
```

`frontend/src/main.tsx`:
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
```

`frontend/src/test/setup.ts`:
```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 6: Install, build, and run a trivial test**

Run: `cd frontend && npm install`
Then add a smoke test `frontend/src/App.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import App from "./App";

test("renders app title", () => {
  render(<App />);
  expect(screen.getByText("Portfolio Tracker")).toBeInTheDocument();
});
```
Run: `npm test`
Expected: 1 test passes.
Run: `npm run build`
Expected: type-checks and builds with no errors.

- [ ] **Step 7: Commit**

```bash
cd /home/bima-pangestu/Works/portfolio-tracker && git add frontend && git commit -m "feat(frontend): scaffold vite react ts tailwind vitest"
```

---

### Task 2: Formatting helpers (money / percent)

**Files:** Create `frontend/src/lib/format.ts`, `frontend/src/lib/format.test.ts`.

- [ ] **Step 1: Write the failing tests** (`format.test.ts`)

```ts
import { formatIDR, formatUSD, formatPct, parseNum } from "./format";

test("parseNum parses decimal strings", () => {
  expect(parseNum("1234.5")).toBe(1234.5);
  expect(parseNum("")).toBe(0);
  expect(parseNum("nope")).toBe(0);
});

test("formatIDR groups with Rp prefix and no decimals", () => {
  expect(formatIDR("4875000")).toBe("Rp 4.875.000");
});

test("formatUSD shows two decimals", () => {
  expect(formatUSD("300")).toBe("$300.00");
});

test("formatPct shows one decimal and sign", () => {
  expect(formatPct("-10")).toBe("-10.0%");
  expect(formatPct("40")).toBe("40.0%");
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && npx vitest run src/lib/format.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `format.ts`**

```ts
export function parseNum(s: string | number | null | undefined): number {
  if (typeof s === "number") return Number.isFinite(s) ? s : 0;
  if (!s) return 0;
  const n = Number(s);
  return Number.isFinite(n) ? n : 0;
}

const idr = new Intl.NumberFormat("id-ID", { style: "currency", currency: "IDR", maximumFractionDigits: 0 });
const usd = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", minimumFractionDigits: 2, maximumFractionDigits: 2 });

export function formatIDR(v: string | number): string {
  return idr.format(parseNum(v));
}

export function formatUSD(v: string | number): string {
  return usd.format(parseNum(v));
}

export function formatPct(v: string | number): string {
  return `${parseNum(v).toFixed(1)}%`;
}
```

Note: `Intl` `id-ID` currency formatting uses a non-breaking space (` `) between `Rp` and the number, and `.` as the thousands separator. The test asserts exactly that. If the installed ICU emits a plain space, adjust the expected string in the test to match the runtime output you observe (run once, read actual, fix the expectation) — do not hack the formatter.

- [ ] **Step 4: Run tests**

Run: `npx vitest run src/lib/format.test.ts`
Expected: PASS (adjust the IDR/USD literal only if your ICU differs, per the note).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib && git commit -m "feat(frontend): money and percent formatters"
```

---

### Task 3: Zod schemas for API responses

**Files:** Create `frontend/src/api/schemas.ts`, `frontend/src/api/schemas.test.ts`.

- [ ] **Step 1: Write the failing tests** (`schemas.test.ts`)

```ts
import { PortfolioSummarySchema, AccountSchema } from "./schemas";

test("parses a portfolio summary", () => {
  const json = {
    net_worth_idr: "4875000", net_worth_usd: "300",
    total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0",
    xirr: 1.68,
    positions: [{
      instrument_id: 1, quantity: "2", avg_cost: "100", cost_basis_total: "200",
      latest_price: "150", price_stale: false, market_value_native: "300",
      market_value_idr: "4875000", market_value_usd: "300",
      unrealized_pnl: "100", realized_pnl: "0", income: "0",
    }],
    allocation: [{
      category_id: 1, name: "Crypto", target_pct: "100", tolerance_band_pct: "5",
      actual_pct: "100", actual_value_idr: "4875000", drift_pct: "0",
      out_of_band: false, rebalance_idr: "0",
    }],
  };
  const parsed = PortfolioSummarySchema.parse(json);
  expect(parsed.xirr).toBe(1.68);
  expect(parsed.positions[0].quantity).toBe("2");
  expect(parsed.allocation[0].out_of_band).toBe(false);
});

test("xirr may be null", () => {
  const parsed = PortfolioSummarySchema.parse({
    net_worth_idr: "0", net_worth_usd: "0", total_unrealized_pnl_idr: "0",
    total_realized_pnl_idr: "0", xirr: null, positions: [], allocation: [],
  });
  expect(parsed.xirr).toBeNull();
});

test("account schema requires core fields", () => {
  const a = AccountSchema.parse({ id: 1, name: "M", account_type: "manual", institution: null, native_currency: "USD", note: null, created_at: "2026-01-01T00:00:00Z" });
  expect(a.name).toBe("M");
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npx vitest run src/api/schemas.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `schemas.ts`**

```ts
import { z } from "zod";

export const AccountSchema = z.object({
  id: z.number(),
  name: z.string(),
  account_type: z.string(),
  institution: z.string().nullable().optional(),
  native_currency: z.string(),
  note: z.string().nullable().optional(),
  created_at: z.string(),
});
export type Account = z.infer<typeof AccountSchema>;

export const CategorySchema = z.object({
  id: z.number(),
  name: z.string(),
  target_pct: z.string(),
  tolerance_band_pct: z.string().nullable().optional(),
  sort_order: z.number(),
  color: z.string().nullable().optional(),
});
export type Category = z.infer<typeof CategorySchema>;

export const InstrumentSchema = z.object({
  id: z.number(),
  symbol: z.string(),
  name: z.string(),
  instrument_type: z.string(),
  native_currency: z.string(),
  category_id: z.number().nullable().optional(),
  price_source: z.string(),
  decimals: z.number(),
  note: z.string().nullable().optional(),
});
export type Instrument = z.infer<typeof InstrumentSchema>;

export const TransactionSchema = z.object({
  id: z.number(),
  account_id: z.number(),
  instrument_id: z.number(),
  txn_type: z.string(),
  executed_at: z.string(),
  quantity: z.string(),
  price_native: z.string(),
  fee_native: z.string(),
  currency: z.string(),
  fx_to_idr: z.string(),
  fx_to_usd: z.string(),
  note: z.string().nullable().optional(),
});
export type Transaction = z.infer<typeof TransactionSchema>;

export const PositionSchema = z.object({
  instrument_id: z.number(),
  quantity: z.string(),
  avg_cost: z.string(),
  cost_basis_total: z.string(),
  latest_price: z.string(),
  price_stale: z.boolean(),
  market_value_native: z.string(),
  market_value_idr: z.string(),
  market_value_usd: z.string(),
  unrealized_pnl: z.string(),
  realized_pnl: z.string(),
  income: z.string(),
});
export type Position = z.infer<typeof PositionSchema>;

export const CategoryAllocationSchema = z.object({
  category_id: z.number(),
  name: z.string(),
  target_pct: z.string(),
  tolerance_band_pct: z.string().nullable().optional(),
  actual_pct: z.string(),
  actual_value_idr: z.string(),
  drift_pct: z.string(),
  out_of_band: z.boolean(),
  rebalance_idr: z.string(),
});
export type CategoryAllocation = z.infer<typeof CategoryAllocationSchema>;

export const PortfolioSummarySchema = z.object({
  net_worth_idr: z.string(),
  net_worth_usd: z.string(),
  total_unrealized_pnl_idr: z.string(),
  total_realized_pnl_idr: z.string(),
  xirr: z.number().nullable(),
  positions: z.array(PositionSchema),
  allocation: z.array(CategoryAllocationSchema),
});
export type PortfolioSummary = z.infer<typeof PortfolioSummarySchema>;

export const SnapshotSchema = z.object({
  as_of: z.string(),
  total_idr: z.string(),
  total_usd: z.string(),
  breakdown_json: z.string(),
});
export type Snapshot = z.infer<typeof SnapshotSchema>;
```

- [ ] **Step 4: Run tests**

Run: `npx vitest run src/api/schemas.test.ts`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/schemas.test.ts && git commit -m "feat(frontend): zod schemas for api responses"
```

---

### Task 4: API client + TanStack Query hooks (MSW-tested)

**Files:** Create `frontend/src/api/client.ts`, `frontend/src/api/hooks.ts`, `frontend/src/test/server.ts`, `frontend/src/api/hooks.test.tsx`. Modify `frontend/src/test/setup.ts`.

- [ ] **Step 1: Implement `client.ts`** (typed fetch + zod validation)

```ts
import { z } from "zod";

const BASE = import.meta.env.VITE_API_BASE ?? "/api";

async function request<T>(path: string, schema: z.ZodType<T>, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { "content-type": "application/json" },
    ...init,
  });
  if (!res.ok) {
    let msg = `HTTP ${res.status}`;
    try { const body = await res.json(); if (body?.error) msg = body.error; } catch { /* keep default */ }
    throw new Error(msg);
  }
  const json = await res.json();
  return schema.parse(json);
}

export const api = {
  get: <T>(path: string, schema: z.ZodType<T>) => request(path, schema, { method: "GET" }),
  post: <T>(path: string, schema: z.ZodType<T>, body: unknown) =>
    request(path, schema, { method: "POST", body: JSON.stringify(body) }),
  del: (path: string) => request(path, z.unknown(), { method: "DELETE" }),
};
```

- [ ] **Step 2: Implement `hooks.ts`**

```ts
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { z } from "zod";
import { api } from "./client";
import {
  AccountSchema, CategorySchema, InstrumentSchema, TransactionSchema,
  PortfolioSummarySchema, SnapshotSchema,
  type Account, type Category, type Instrument, type Transaction,
} from "./schemas";

export const useSummary = () =>
  useQuery({ queryKey: ["summary"], queryFn: () => api.get("/portfolio/summary", PortfolioSummarySchema) });

export const useHistory = () =>
  useQuery({ queryKey: ["history"], queryFn: () => api.get("/portfolio/history", z.array(SnapshotSchema)) });

export const useAccounts = () =>
  useQuery({ queryKey: ["accounts"], queryFn: () => api.get("/accounts", z.array(AccountSchema)) });

export const useCategories = () =>
  useQuery({ queryKey: ["categories"], queryFn: () => api.get("/categories", z.array(CategorySchema)) });

export const useInstruments = () =>
  useQuery({ queryKey: ["instruments"], queryFn: () => api.get("/instruments", z.array(InstrumentSchema)) });

export const useTransactions = () =>
  useQuery({ queryKey: ["transactions"], queryFn: () => api.get("/transactions", z.array(TransactionSchema)) });

function useInvalidatingMutation<TInput>(fn: (input: TInput) => Promise<unknown>, keys: string[]) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: () => { keys.forEach((k) => qc.invalidateQueries({ queryKey: [k] })); },
  });
}

export const useCreateAccount = () =>
  useInvalidatingMutation((b: Omit<Account, "id" | "created_at">) => api.post("/accounts", AccountSchema, b), ["accounts"]);
export const useCreateCategory = () =>
  useInvalidatingMutation((b: Omit<Category, "id" | "sort_order">) => api.post("/categories", CategorySchema, b), ["categories", "summary"]);
export const useCreateInstrument = () =>
  useInvalidatingMutation((b: Omit<Instrument, "id">) => api.post("/instruments", InstrumentSchema, b), ["instruments"]);
export const useCreateTransaction = () =>
  useInvalidatingMutation((b: Record<string, unknown>) => api.post("/transactions", TransactionSchema, b), ["transactions", "summary"]);
export const useManualPrice = () =>
  useInvalidatingMutation((b: { instrument_id: number; price: string; currency: string; as_of: string }) => api.post("/prices/manual", z.unknown(), b), ["summary"]);
export const useManualFx = () =>
  useInvalidatingMutation((b: { base: string; quote: string; rate: string; as_of: string }) => api.post("/fx/manual", z.unknown(), b), ["summary"]);
export const useRefreshPrices = () =>
  useInvalidatingMutation((_: void) => api.post("/prices/refresh", z.unknown(), {}), ["summary"]);

export const useDeleteAccount = () => useInvalidatingMutation((id: number) => api.del(`/accounts/${id}`), ["accounts"]);
export const useDeleteCategory = () => useInvalidatingMutation((id: number) => api.del(`/categories/${id}`), ["categories", "summary"]);
export const useDeleteInstrument = () => useInvalidatingMutation((id: number) => api.del(`/instruments/${id}`), ["instruments"]);
export const useDeleteTransaction = () => useInvalidatingMutation((id: number) => api.del(`/transactions/${id}`), ["transactions", "summary"]);

export type { Account, Category, Instrument, Transaction };
```

- [ ] **Step 3: Create MSW server `frontend/src/test/server.ts`**

```ts
import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";

export const handlers = [
  http.get("/api/portfolio/summary", () =>
    HttpResponse.json({
      net_worth_idr: "4875000", net_worth_usd: "300",
      total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0", xirr: 1.68,
      positions: [], allocation: [],
    }),
  ),
];

export const server = setupServer(...handlers);
```

- [ ] **Step 4: Wire MSW into `frontend/src/test/setup.ts`**

```ts
import "@testing-library/jest-dom/vitest";
import { afterAll, afterEach, beforeAll } from "vitest";
import { server } from "./server";

beforeAll(() => server.listen({ onUnhandledRequest: "error" }));
afterEach(() => server.resetHandlers());
afterAll(() => server.close());
```

- [ ] **Step 5: Write `frontend/src/api/hooks.test.tsx`**

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useSummary } from "./hooks";

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("useSummary fetches and validates summary", async () => {
  const { result } = renderHook(() => useSummary(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.net_worth_usd).toBe("300");
});
```

- [ ] **Step 6: Run tests**

Run: `cd frontend && npx vitest run src/api/hooks.test.tsx`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/api/client.ts frontend/src/api/hooks.ts frontend/src/test && git commit -m "feat(frontend): api client and react-query hooks with msw tests"
```

---

### Task 5: App shell — providers, routing, nav layout

**Files:** Replace `frontend/src/App.tsx`; create `frontend/src/components/Layout.tsx`. Modify `frontend/src/main.tsx`.

- [ ] **Step 1: Create `frontend/src/components/Layout.tsx`**

```tsx
import { NavLink, Outlet } from "react-router-dom";

const links = [
  { to: "/", label: "Dashboard", end: true },
  { to: "/holdings", label: "Holdings" },
  { to: "/transactions", label: "Transactions" },
  { to: "/planner", label: "Planner" },
  { to: "/settings", label: "Settings" },
];

export default function Layout() {
  return (
    <div className="min-h-screen bg-gray-50 text-gray-900">
      <header className="border-b bg-white">
        <nav className="mx-auto flex max-w-5xl gap-4 px-4 py-3">
          <span className="font-bold">📊 Portfolio</span>
          {links.map((l) => (
            <NavLink
              key={l.to}
              to={l.to}
              end={l.end}
              className={({ isActive }) => (isActive ? "font-semibold text-blue-600" : "text-gray-600")}
            >
              {l.label}
            </NavLink>
          ))}
        </nav>
      </header>
      <main className="mx-auto max-w-5xl p-4">
        <Outlet />
      </main>
    </div>
  );
}
```

- [ ] **Step 2: Replace `frontend/src/App.tsx`**

```tsx
import { Routes, Route } from "react-router-dom";
import Layout from "./components/Layout";
import DashboardPage from "./pages/DashboardPage";
import HoldingsPage from "./pages/HoldingsPage";
import TransactionsPage from "./pages/TransactionsPage";
import PlannerPage from "./pages/PlannerPage";
import SettingsPage from "./pages/SettingsPage";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<DashboardPage />} />
        <Route path="holdings" element={<HoldingsPage />} />
        <Route path="transactions" element={<TransactionsPage />} />
        <Route path="planner" element={<PlannerPage />} />
        <Route path="settings" element={<SettingsPage />} />
      </Route>
    </Routes>
  );
}
```

- [ ] **Step 3: Update `frontend/src/main.tsx`** to add providers

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import "./index.css";

const queryClient = new QueryClient();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </QueryClientProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 4: Create placeholder pages so the build compiles**

Create each of these minimal files (they are fully implemented in later tasks):
`frontend/src/pages/DashboardPage.tsx`, `HoldingsPage.tsx`, `TransactionsPage.tsx`, `PlannerPage.tsx`, `SettingsPage.tsx`, each:
```tsx
export default function Page() {
  return <div>Coming soon</div>;
}
```
(Use the matching default export name per file, e.g. `DashboardPage`.)

- [ ] **Step 5: Build + the App smoke test from Task 1 must be updated**

The Task 1 `App.test.tsx` no longer matches (App now needs Router). Replace `frontend/src/App.test.tsx`:
```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen } from "@testing-library/react";
import App from "./App";

test("renders nav with Dashboard link", () => {
  const qc = new QueryClient();
  render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <App />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  expect(screen.getByText("Dashboard")).toBeInTheDocument();
});
```
Run: `npx vitest run src/App.test.tsx` → PASS. Run `npm run build` → no type errors.

- [ ] **Step 6: Commit**

```bash
git add frontend/src && git commit -m "feat(frontend): app shell with routing and nav layout"
```

---

### Task 6: Dashboard — NetWorthCard + PerformanceCards

**Files:** Create `frontend/src/components/NetWorthCard.tsx`, `frontend/src/components/PerformanceCards.tsx`, `frontend/src/components/StatCard.tsx`, and `frontend/src/components/PerformanceCards.test.tsx`.

- [ ] **Step 1: Create `StatCard.tsx`** (presentational)

```tsx
export function StatCard({ label, value, sub, tone }: { label: string; value: string; sub?: string; tone?: "pos" | "neg" | "neutral" }) {
  const color = tone === "pos" ? "text-green-600" : tone === "neg" ? "text-red-600" : "text-gray-900";
  return (
    <div className="rounded-lg border bg-white p-4">
      <div className="text-xs uppercase tracking-wide text-gray-500">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${color}`}>{value}</div>
      {sub && <div className="mt-1 text-sm text-gray-500">{sub}</div>}
    </div>
  );
}
```

- [ ] **Step 2: Create `NetWorthCard.tsx`**

```tsx
import { formatIDR, formatUSD } from "../lib/format";
import { StatCard } from "./StatCard";
import type { PortfolioSummary } from "../api/schemas";

export function NetWorthCard({ s }: { s: PortfolioSummary }) {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <StatCard label="Net Worth (IDR)" value={formatIDR(s.net_worth_idr)} />
      <StatCard label="Net Worth (USD)" value={formatUSD(s.net_worth_usd)} />
    </div>
  );
}
```

- [ ] **Step 3: Write the failing test** (`PerformanceCards.test.tsx`)

```tsx
import { render, screen } from "@testing-library/react";
import { PerformanceCards } from "./PerformanceCards";
import type { PortfolioSummary } from "../api/schemas";

const base: PortfolioSummary = {
  net_worth_idr: "4875000", net_worth_usd: "300",
  total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0",
  xirr: 0.168, positions: [], allocation: [],
};

test("shows XIRR as a percentage", () => {
  render(<PerformanceCards s={base} />);
  expect(screen.getByText("16.8%")).toBeInTheDocument();
});

test("shows dash when XIRR is null", () => {
  render(<PerformanceCards s={{ ...base, xirr: null }} />);
  expect(screen.getByText("—")).toBeInTheDocument();
});
```

- [ ] **Step 4: Run to verify failure**

Run: `npx vitest run src/components/PerformanceCards.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 5: Implement `PerformanceCards.tsx`**

```tsx
import { formatIDR } from "../lib/format";
import { StatCard } from "./StatCard";
import type { PortfolioSummary } from "../api/schemas";

function pnlTone(v: string): "pos" | "neg" | "neutral" {
  const n = Number(v);
  if (n > 0) return "pos";
  if (n < 0) return "neg";
  return "neutral";
}

export function PerformanceCards({ s }: { s: PortfolioSummary }) {
  const xirr = s.xirr == null ? "—" : `${(s.xirr * 100).toFixed(1)}%`;
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
      <StatCard label="XIRR (annualized)" value={xirr} tone={s.xirr != null && s.xirr >= 0 ? "pos" : s.xirr != null ? "neg" : "neutral"} />
      <StatCard label="Unrealized P&L (IDR)" value={formatIDR(s.total_unrealized_pnl_idr)} tone={pnlTone(s.total_unrealized_pnl_idr)} />
      <StatCard label="Realized P&L (IDR)" value={formatIDR(s.total_realized_pnl_idr)} tone={pnlTone(s.total_realized_pnl_idr)} />
    </div>
  );
}
```

- [ ] **Step 6: Run tests + commit**

Run: `npx vitest run src/components/PerformanceCards.test.tsx` → PASS.
```bash
git add frontend/src/components && git commit -m "feat(frontend): net worth and performance stat cards"
```

---

### Task 7: Dashboard — AllocationDonut + DriftBars

**Files:** Create `frontend/src/components/AllocationDonut.tsx`, `frontend/src/components/DriftBars.tsx`, `frontend/src/components/DriftBars.test.tsx`.

- [ ] **Step 1: Write the failing test** (`DriftBars.test.tsx`)

```tsx
import { render, screen } from "@testing-library/react";
import { DriftBars } from "./DriftBars";
import type { CategoryAllocation } from "../api/schemas";

const cats: CategoryAllocation[] = [
  { category_id: 1, name: "USD ETF", target_pct: "50", tolerance_band_pct: "5", actual_pct: "40", actual_value_idr: "400", drift_pct: "-10", out_of_band: true, rebalance_idr: "100" },
  { category_id: 2, name: "Saham ID", target_pct: "50", tolerance_band_pct: "5", actual_pct: "60", actual_value_idr: "600", drift_pct: "10", out_of_band: false, rebalance_idr: "-100" },
];

test("renders each category name and flags out-of-band", () => {
  render(<DriftBars allocation={cats} />);
  expect(screen.getByText("USD ETF")).toBeInTheDocument();
  expect(screen.getByText("Saham ID")).toBeInTheDocument();
  // out-of-band category shows a warning marker
  expect(screen.getByTestId("oob-1")).toBeInTheDocument();
  expect(screen.queryByTestId("oob-2")).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npx vitest run src/components/DriftBars.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `DriftBars.tsx`**

```tsx
import { formatIDR, formatPct } from "../lib/format";
import type { CategoryAllocation } from "../api/schemas";

export function DriftBars({ allocation }: { allocation: CategoryAllocation[] }) {
  if (allocation.length === 0) return <div className="text-sm text-gray-500">No categories yet.</div>;
  return (
    <div className="space-y-3">
      {allocation.map((c) => {
        const actual = Number(c.actual_pct);
        const target = Number(c.target_pct);
        const reb = Number(c.rebalance_idr);
        return (
          <div key={c.category_id} className="rounded border bg-white p-3">
            <div className="flex items-center justify-between text-sm">
              <span className="font-medium">
                {c.name}
                {c.out_of_band && (
                  <span data-testid={`oob-${c.category_id}`} className="ml-2 rounded bg-red-100 px-1.5 py-0.5 text-xs text-red-700">
                    out of band
                  </span>
                )}
              </span>
              <span className="text-gray-600">
                {formatPct(c.actual_pct)} / target {formatPct(c.target_pct)} (drift {formatPct(c.drift_pct)})
              </span>
            </div>
            <div className="mt-2 h-2 w-full rounded bg-gray-100">
              <div className={`h-2 rounded ${c.out_of_band ? "bg-red-500" : "bg-blue-500"}`} style={{ width: `${Math.min(100, Math.max(0, actual))}%` }} />
            </div>
            <div className="mt-1 text-xs text-gray-500">
              {reb > 0 ? `Buy ${formatIDR(c.rebalance_idr)} to reach target` : reb < 0 ? `Trim ${formatIDR(Math.abs(reb))} to reach target` : "On target"}
              {` · target marker at ${target.toFixed(0)}%`}
            </div>
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: Implement `AllocationDonut.tsx`** (Recharts pie of actual values)

```tsx
import { PieChart, Pie, Cell, ResponsiveContainer, Tooltip, Legend } from "recharts";
import type { CategoryAllocation } from "../api/schemas";

const COLORS = ["#2563eb", "#16a34a", "#f59e0b", "#dc2626", "#7c3aed", "#0891b2", "#db2777"];

export function AllocationDonut({ allocation }: { allocation: CategoryAllocation[] }) {
  const data = allocation
    .map((c) => ({ name: c.name, value: Number(c.actual_value_idr) }))
    .filter((d) => d.value > 0);
  if (data.length === 0) return <div className="text-sm text-gray-500">No holdings to allocate.</div>;
  return (
    <div className="h-64 w-full rounded border bg-white p-2">
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie data={data} dataKey="value" nameKey="name" innerRadius="55%" outerRadius="80%">
            {data.map((_, i) => (
              <Cell key={i} fill={COLORS[i % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip />
          <Legend />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}
```

- [ ] **Step 5: Run tests + commit**

Run: `npx vitest run src/components/DriftBars.test.tsx` → PASS.
Run: `npm run build` → no type errors (recharts types resolve).
```bash
git add frontend/src/components && git commit -m "feat(frontend): allocation donut and target-vs-actual drift bars"
```

---

### Task 8: Dashboard — HistoryChart

**Files:** Create `frontend/src/components/HistoryChart.tsx`, `frontend/src/components/HistoryChart.test.tsx`.

- [ ] **Step 1: Write the failing test**

```tsx
import { render } from "@testing-library/react";
import { HistoryChart } from "./HistoryChart";
import type { Snapshot } from "../api/schemas";

test("renders without crashing for empty and non-empty data", () => {
  const empty: Snapshot[] = [];
  const { rerender, container } = render(<HistoryChart snapshots={empty} />);
  expect(container).toBeTruthy();
  const data: Snapshot[] = [
    { as_of: "2026-05-30", total_idr: "1000", total_usd: "0.06", breakdown_json: "[]" },
    { as_of: "2026-05-31", total_idr: "1100", total_usd: "0.07", breakdown_json: "[]" },
  ];
  rerender(<HistoryChart snapshots={data} />);
  expect(container).toBeTruthy();
});
```

- [ ] **Step 2: Run to verify failure**

Run: `npx vitest run src/components/HistoryChart.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `HistoryChart.tsx`**

```tsx
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from "recharts";
import { formatIDR } from "../lib/format";
import type { Snapshot } from "../api/schemas";

export function HistoryChart({ snapshots }: { snapshots: Snapshot[] }) {
  const data = snapshots.map((s) => ({ date: s.as_of, idr: Number(s.total_idr) }));
  if (data.length === 0) return <div className="text-sm text-gray-500">No history yet — snapshots accumulate daily.</div>;
  return (
    <div className="h-64 w-full rounded border bg-white p-2">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 10, right: 20, bottom: 0, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey="date" fontSize={11} />
          <YAxis tickFormatter={(v) => formatIDR(v)} width={90} fontSize={11} />
          <Tooltip formatter={(v: number) => formatIDR(v)} />
          <Line type="monotone" dataKey="idr" stroke="#2563eb" dot={false} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
```

- [ ] **Step 4: Run tests + commit**

Run: `npx vitest run src/components/HistoryChart.test.tsx` → PASS.
```bash
git add frontend/src/components/HistoryChart.tsx frontend/src/components/HistoryChart.test.tsx && git commit -m "feat(frontend): portfolio value history line chart"
```

---

### Task 9: DashboardPage assembly

**Files:** Replace `frontend/src/pages/DashboardPage.tsx`; create `frontend/src/components/QueryState.tsx`.

- [ ] **Step 1: Create `QueryState.tsx`** (loading/error wrapper)

```tsx
import type { ReactNode } from "react";

export function QueryState({ isLoading, error, children }: { isLoading: boolean; error: unknown; children: ReactNode }) {
  if (isLoading) return <div className="p-4 text-gray-500">Loading…</div>;
  if (error) return <div className="p-4 text-red-600">Error: {error instanceof Error ? error.message : "unknown"}</div>;
  return <>{children}</>;
}
```

- [ ] **Step 2: Replace `DashboardPage.tsx`**

```tsx
import { useSummary, useHistory, useRefreshPrices } from "../api/hooks";
import { NetWorthCard } from "../components/NetWorthCard";
import { PerformanceCards } from "../components/PerformanceCards";
import { AllocationDonut } from "../components/AllocationDonut";
import { DriftBars } from "../components/DriftBars";
import { HistoryChart } from "../components/HistoryChart";
import { QueryState } from "../components/QueryState";

export default function DashboardPage() {
  const summary = useSummary();
  const history = useHistory();
  const refresh = useRefreshPrices();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Dashboard</h1>
        <button
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
          className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50"
        >
          {refresh.isPending ? "Refreshing…" : "Refresh prices"}
        </button>
      </div>

      <QueryState isLoading={summary.isLoading} error={summary.error}>
        {summary.data && (
          <>
            <NetWorthCard s={summary.data} />
            <PerformanceCards s={summary.data} />
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <section>
                <h2 className="mb-2 text-sm font-semibold text-gray-700">Allocation</h2>
                <AllocationDonut allocation={summary.data.allocation} />
              </section>
              <section>
                <h2 className="mb-2 text-sm font-semibold text-gray-700">Target vs Actual</h2>
                <DriftBars allocation={summary.data.allocation} />
              </section>
            </div>
          </>
        )}
      </QueryState>

      <section>
        <h2 className="mb-2 text-sm font-semibold text-gray-700">Value History</h2>
        <QueryState isLoading={history.isLoading} error={history.error}>
          <HistoryChart snapshots={history.data ?? []} />
        </QueryState>
      </section>
    </div>
  );
}
```

- [ ] **Step 3: Add a page test** `frontend/src/pages/DashboardPage.test.tsx` (uses MSW summary handler; add a history handler)

First extend `frontend/src/test/server.ts` handlers with history + refresh:
```ts
  http.get("/api/portfolio/history", () => HttpResponse.json([])),
  http.post("/api/prices/refresh", () => HttpResponse.json(null)),
```
Then:
```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import DashboardPage from "./DashboardPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("dashboard shows net worth from the API", async () => {
  renderPage();
  await waitFor(() => expect(screen.getByText("Net Worth (USD)")).toBeInTheDocument());
  expect(screen.getByText("$300.00")).toBeInTheDocument();
});
```

- [ ] **Step 4: Run tests + build + commit**

Run: `npx vitest run src/pages/DashboardPage.test.tsx` → PASS.
Run: `npm run build` → clean.
```bash
git add frontend/src && git commit -m "feat(frontend): dashboard page assembling net worth, performance, allocation, history"
```

---

### Task 10: HoldingsPage (positions table)

**Files:** Replace `frontend/src/pages/HoldingsPage.tsx`; create `frontend/src/pages/HoldingsPage.test.tsx`. Extend MSW handlers with `/instruments`.

- [ ] **Step 1: Replace `HoldingsPage.tsx`**

```tsx
import { useSummary, useInstruments } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, formatUSD, formatPct } from "../lib/format";

export default function HoldingsPage() {
  const summary = useSummary();
  const instruments = useInstruments();
  const nameOf = (id: number) => instruments.data?.find((i) => i.id === id)?.symbol ?? `#${id}`;

  return (
    <div className="space-y-4">
      <h1 className="text-xl font-semibold">Holdings</h1>
      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <div className="overflow-x-auto rounded border bg-white">
          <table className="w-full text-sm">
            <thead className="bg-gray-50 text-left text-xs uppercase text-gray-500">
              <tr>
                <th className="p-2">Instrument</th>
                <th className="p-2">Qty</th>
                <th className="p-2">Avg cost</th>
                <th className="p-2">Price</th>
                <th className="p-2">Value (IDR)</th>
                <th className="p-2">Unrealized</th>
              </tr>
            </thead>
            <tbody>
              {(summary.data?.positions ?? []).map((p) => (
                <tr key={p.instrument_id} className="border-t">
                  <td className="p-2 font-medium">
                    {nameOf(p.instrument_id)}
                    {p.price_stale && <span className="ml-1 text-xs text-amber-600" title="Price may be outdated">⚠ stale</span>}
                  </td>
                  <td className="p-2">{p.quantity}</td>
                  <td className="p-2">{formatUSD(p.avg_cost)}</td>
                  <td className="p-2">{formatUSD(p.latest_price)}</td>
                  <td className="p-2">{formatIDR(p.market_value_idr)}</td>
                  <td className={`p-2 ${Number(p.unrealized_pnl) >= 0 ? "text-green-600" : "text-red-600"}`}>
                    {formatUSD(p.unrealized_pnl)} ({formatPct(((Number(p.unrealized_pnl) / (Number(p.cost_basis_total) || 1)) * 100).toString())})
                  </td>
                </tr>
              ))}
              {(summary.data?.positions ?? []).length === 0 && (
                <tr><td className="p-3 text-gray-500" colSpan={6}>No positions yet. Add transactions to see holdings.</td></tr>
              )}
            </tbody>
          </table>
        </div>
      </QueryState>
    </div>
  );
}
```

- [ ] **Step 2: Extend MSW handlers** in `frontend/src/test/server.ts`

```ts
  http.get("/api/instruments", () => HttpResponse.json([])),
```

- [ ] **Step 3: Write `HoldingsPage.test.tsx`**

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import HoldingsPage from "./HoldingsPage";

test("shows empty state when no positions", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><HoldingsPage /></QueryClientProvider>);
  await waitFor(() => expect(screen.getByText(/No positions yet/)).toBeInTheDocument());
});
```

- [ ] **Step 4: Run tests + build + commit**

Run: `npx vitest run src/pages/HoldingsPage.test.tsx` → PASS; `npm run build` clean.
```bash
git add frontend/src && git commit -m "feat(frontend): holdings table with positions and stale-price flag"
```

---

### Task 11: TransactionsPage (list + create form + delete)

**Files:** Replace `frontend/src/pages/TransactionsPage.tsx`; create `frontend/src/pages/TransactionsPage.test.tsx`. Extend MSW handlers (`/accounts`, `/transactions` GET+POST+DELETE).

- [ ] **Step 1: Replace `TransactionsPage.tsx`**

```tsx
import { useState } from "react";
import { useAccounts, useInstruments, useTransactions, useCreateTransaction, useDeleteTransaction } from "../api/hooks";
import { QueryState } from "../components/QueryState";

const TXN_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];

export default function TransactionsPage() {
  const txns = useTransactions();
  const accounts = useAccounts();
  const instruments = useInstruments();
  const create = useCreateTransaction();
  const del = useDeleteTransaction();

  const [form, setForm] = useState({
    account_id: "", instrument_id: "", txn_type: "buy",
    executed_at: new Date().toISOString().slice(0, 16),
    quantity: "", price_native: "", fee_native: "0",
    currency: "USD", fx_to_idr: "16000", fx_to_usd: "1",
  });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate({
      account_id: Number(form.account_id),
      instrument_id: Number(form.instrument_id),
      txn_type: form.txn_type,
      executed_at: new Date(form.executed_at).toISOString(),
      quantity: form.quantity,
      price_native: form.price_native,
      fee_native: form.fee_native,
      currency: form.currency,
      fx_to_idr: form.fx_to_idr,
      fx_to_usd: form.fx_to_usd,
    });
  };

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => setForm({ ...form, [k]: e.target.value });
  const input = "rounded border px-2 py-1 text-sm";

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Transactions</h1>

      <form onSubmit={submit} className="grid grid-cols-2 gap-2 rounded border bg-white p-4 sm:grid-cols-4">
        <select className={input} value={form.account_id} onChange={set("account_id")} required>
          <option value="">Account…</option>
          {(accounts.data ?? []).map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
        </select>
        <select className={input} value={form.instrument_id} onChange={set("instrument_id")} required>
          <option value="">Instrument…</option>
          {(instruments.data ?? []).map((i) => <option key={i.id} value={i.id}>{i.symbol}</option>)}
        </select>
        <select className={input} value={form.txn_type} onChange={set("txn_type")}>
          {TXN_TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
        </select>
        <input className={input} type="datetime-local" value={form.executed_at} onChange={set("executed_at")} />
        <input className={input} placeholder="Quantity" value={form.quantity} onChange={set("quantity")} required />
        <input className={input} placeholder="Price (native)" value={form.price_native} onChange={set("price_native")} required />
        <input className={input} placeholder="Fee" value={form.fee_native} onChange={set("fee_native")} />
        <input className={input} placeholder="Currency" value={form.currency} onChange={set("currency")} />
        <input className={input} placeholder="FX→IDR" value={form.fx_to_idr} onChange={set("fx_to_idr")} />
        <input className={input} placeholder="FX→USD" value={form.fx_to_usd} onChange={set("fx_to_usd")} />
        <button className="col-span-2 rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50 sm:col-span-4" disabled={create.isPending}>
          {create.isPending ? "Adding…" : "Add transaction"}
        </button>
        {create.error && <div className="col-span-2 text-sm text-red-600 sm:col-span-4">{(create.error as Error).message}</div>}
      </form>

      <QueryState isLoading={txns.isLoading} error={txns.error}>
        <div className="overflow-x-auto rounded border bg-white">
          <table className="w-full text-sm">
            <thead className="bg-gray-50 text-left text-xs uppercase text-gray-500">
              <tr><th className="p-2">Date</th><th className="p-2">Type</th><th className="p-2">Instr</th><th className="p-2">Qty</th><th className="p-2">Price</th><th className="p-2"></th></tr>
            </thead>
            <tbody>
              {(txns.data ?? []).map((t) => (
                <tr key={t.id} className="border-t">
                  <td className="p-2">{t.executed_at.slice(0, 10)}</td>
                  <td className="p-2">{t.txn_type}</td>
                  <td className="p-2">#{t.instrument_id}</td>
                  <td className="p-2">{t.quantity}</td>
                  <td className="p-2">{t.price_native} {t.currency}</td>
                  <td className="p-2 text-right">
                    <button onClick={() => del.mutate(t.id)} className="text-xs text-red-600 hover:underline">delete</button>
                  </td>
                </tr>
              ))}
              {(txns.data ?? []).length === 0 && <tr><td colSpan={6} className="p-3 text-gray-500">No transactions yet.</td></tr>}
            </tbody>
          </table>
        </div>
      </QueryState>
    </div>
  );
}
```

- [ ] **Step 2: Extend MSW handlers** in `server.ts`

```ts
  http.get("/api/accounts", () => HttpResponse.json([])),
  http.get("/api/transactions", () => HttpResponse.json([])),
```

- [ ] **Step 3: Write `TransactionsPage.test.tsx`**

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import TransactionsPage from "./TransactionsPage";

test("renders the add-transaction form and empty list", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><TransactionsPage /></QueryClientProvider>);
  expect(screen.getByText("Add transaction")).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText("No transactions yet.")).toBeInTheDocument());
});
```

- [ ] **Step 4: Run tests + build + commit**

Run: `npx vitest run src/pages/TransactionsPage.test.tsx` → PASS; `npm run build` clean.
```bash
git add frontend/src && git commit -m "feat(frontend): transactions page with create form and delete"
```

---

### Task 12: PlannerPage (category targets + bands)

**Files:** Replace `frontend/src/pages/PlannerPage.tsx`; create `frontend/src/pages/PlannerPage.test.tsx`. Extend MSW handlers (`/categories` GET+POST+DELETE).

- [ ] **Step 1: Replace `PlannerPage.tsx`**

```tsx
import { useState } from "react";
import { useCategories, useCreateCategory, useDeleteCategory, useSummary } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatPct } from "../lib/format";

export default function PlannerPage() {
  const cats = useCategories();
  const summary = useSummary();
  const create = useCreateCategory();
  const del = useDeleteCategory();
  const [form, setForm] = useState({ name: "", target_pct: "", tolerance_band_pct: "" });

  const totalTarget = (cats.data ?? []).reduce((acc, c) => acc + Number(c.target_pct), 0);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate({
      name: form.name,
      target_pct: form.target_pct,
      tolerance_band_pct: form.tolerance_band_pct || null,
      color: null,
    });
    setForm({ name: "", target_pct: "", tolerance_band_pct: "" });
  };
  const input = "rounded border px-2 py-1 text-sm";

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Allocation Planner</h1>

      <form onSubmit={submit} className="grid grid-cols-2 gap-2 rounded border bg-white p-4 sm:grid-cols-4">
        <input className={input} placeholder="Category name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} required />
        <input className={input} placeholder="Target %" value={form.target_pct} onChange={(e) => setForm({ ...form, target_pct: e.target.value })} required />
        <input className={input} placeholder="Tolerance band % (optional)" value={form.tolerance_band_pct} onChange={(e) => setForm({ ...form, tolerance_band_pct: e.target.value })} />
        <button className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50" disabled={create.isPending}>Add category</button>
        {create.error && <div className="col-span-2 text-sm text-red-600 sm:col-span-4">{(create.error as Error).message}</div>}
      </form>

      <div className={`text-sm ${Math.abs(totalTarget - 100) > 0.01 ? "text-amber-600" : "text-gray-500"}`}>
        Total target: {totalTarget.toFixed(1)}% {Math.abs(totalTarget - 100) > 0.01 ? "(should sum to 100%)" : "✓"}
      </div>

      <QueryState isLoading={cats.isLoading} error={cats.error}>
        <div className="space-y-2">
          {(cats.data ?? []).map((c) => {
            const a = summary.data?.allocation.find((x) => x.category_id === c.id);
            return (
              <div key={c.id} className="flex items-center justify-between rounded border bg-white p-3 text-sm">
                <div>
                  <span className="font-medium">{c.name}</span>
                  <span className="ml-2 text-gray-500">target {formatPct(c.target_pct)}{c.tolerance_band_pct ? ` ±${c.tolerance_band_pct}%` : ""}</span>
                  {a && <span className={`ml-2 ${a.out_of_band ? "text-red-600" : "text-gray-600"}`}>actual {formatPct(a.actual_pct)}</span>}
                </div>
                <button onClick={() => del.mutate(c.id)} className="text-xs text-red-600 hover:underline">delete</button>
              </div>
            );
          })}
          {(cats.data ?? []).length === 0 && <div className="text-gray-500">No categories yet.</div>}
        </div>
      </QueryState>
    </div>
  );
}
```

- [ ] **Step 2: Extend MSW handlers** in `server.ts`

```ts
  http.get("/api/categories", () => HttpResponse.json([])),
```

- [ ] **Step 3: Write `PlannerPage.test.tsx`**

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import PlannerPage from "./PlannerPage";

test("shows planner form and total-target hint", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><PlannerPage /></QueryClientProvider>);
  expect(screen.getByText("Add category")).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText(/Total target:/)).toBeInTheDocument());
});
```

- [ ] **Step 4: Run tests + build + commit**

Run: `npx vitest run src/pages/PlannerPage.test.tsx` → PASS; `npm run build` clean.
```bash
git add frontend/src && git commit -m "feat(frontend): allocation planner page with target/band and 100% check"
```

---

### Task 13: SettingsPage (accounts, instruments, manual price, manual FX)

**Files:** Replace `frontend/src/pages/SettingsPage.tsx`; create `frontend/src/pages/SettingsPage.test.tsx`.

- [ ] **Step 1: Replace `SettingsPage.tsx`**

```tsx
import { useState } from "react";
import {
  useAccounts, useCreateAccount, useDeleteAccount,
  useInstruments, useCreateInstrument, useDeleteInstrument,
  useManualPrice, useManualFx,
} from "../api/hooks";

const input = "rounded border px-2 py-1 text-sm";
const today = () => new Date().toISOString().slice(0, 10);

export default function SettingsPage() {
  const accounts = useAccounts();
  const instruments = useInstruments();
  const createAccount = useCreateAccount();
  const delAccount = useDeleteAccount();
  const createInstrument = useCreateInstrument();
  const delInstrument = useDeleteInstrument();
  const manualPrice = useManualPrice();
  const manualFx = useManualFx();

  const [acc, setAcc] = useState({ name: "", account_type: "manual", native_currency: "IDR" });
  const [ins, setIns] = useState({ symbol: "", name: "", instrument_type: "crypto", native_currency: "USD", category_id: "", price_source: "manual" });
  const [price, setPrice] = useState({ instrument_id: "", price: "", currency: "USD" });
  const [fx, setFx] = useState({ rate: "" });

  return (
    <div className="space-y-8">
      <h1 className="text-xl font-semibold">Settings</h1>

      <section className="space-y-2">
        <h2 className="font-semibold">Accounts</h2>
        <form onSubmit={(e) => { e.preventDefault(); createAccount.mutate({ ...acc, institution: null, note: null }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input className={input} placeholder="Name" value={acc.name} onChange={(e) => setAcc({ ...acc, name: e.target.value })} required />
          <select className={input} value={acc.account_type} onChange={(e) => setAcc({ ...acc, account_type: e.target.value })}>
            {["manual", "exchange", "broker", "bank", "wallet"].map((t) => <option key={t}>{t}</option>)}
          </select>
          <input className={input} placeholder="Currency" value={acc.native_currency} onChange={(e) => setAcc({ ...acc, native_currency: e.target.value })} />
          <button className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Add</button>
        </form>
        <ul className="text-sm">
          {(accounts.data ?? []).map((a) => (
            <li key={a.id} className="flex justify-between border-b py-1">
              <span>{a.name} · {a.account_type} · {a.native_currency}</span>
              <button onClick={() => delAccount.mutate(a.id)} className="text-xs text-red-600">delete</button>
            </li>
          ))}
        </ul>
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">Instruments</h2>
        <form onSubmit={(e) => { e.preventDefault(); createInstrument.mutate({ symbol: ins.symbol, name: ins.name, instrument_type: ins.instrument_type, native_currency: ins.native_currency, category_id: ins.category_id ? Number(ins.category_id) : null, price_source: ins.price_source, decimals: 8, note: null }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input className={input} placeholder="Symbol" value={ins.symbol} onChange={(e) => setIns({ ...ins, symbol: e.target.value })} required />
          <input className={input} placeholder="Name" value={ins.name} onChange={(e) => setIns({ ...ins, name: e.target.value })} required />
          <select className={input} value={ins.instrument_type} onChange={(e) => setIns({ ...ins, instrument_type: e.target.value })}>
            {["crypto", "stock_id", "stock_us", "etf", "mutual_fund", "cash", "bond", "gold", "other"].map((t) => <option key={t}>{t}</option>)}
          </select>
          <input className={input} placeholder="Currency" value={ins.native_currency} onChange={(e) => setIns({ ...ins, native_currency: e.target.value })} />
          <input className={input} placeholder="category_id (optional)" value={ins.category_id} onChange={(e) => setIns({ ...ins, category_id: e.target.value })} />
          <input className={input} placeholder="price_source (e.g. coingecko:bitcoin, yahoo:BBCA.JK, manual)" value={ins.price_source} onChange={(e) => setIns({ ...ins, price_source: e.target.value })} />
          <button className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Add</button>
        </form>
        <ul className="text-sm">
          {(instruments.data ?? []).map((i) => (
            <li key={i.id} className="flex justify-between border-b py-1">
              <span>{i.symbol} · {i.instrument_type} · {i.price_source}</span>
              <button onClick={() => delInstrument.mutate(i.id)} className="text-xs text-red-600">delete</button>
            </li>
          ))}
        </ul>
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">Manual price (for reksadana NAV / manual instruments)</h2>
        <form onSubmit={(e) => { e.preventDefault(); manualPrice.mutate({ instrument_id: Number(price.instrument_id), price: price.price, currency: price.currency, as_of: today() }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input className={input} placeholder="instrument_id" value={price.instrument_id} onChange={(e) => setPrice({ ...price, instrument_id: e.target.value })} required />
          <input className={input} placeholder="price" value={price.price} onChange={(e) => setPrice({ ...price, price: e.target.value })} required />
          <input className={input} placeholder="currency" value={price.currency} onChange={(e) => setPrice({ ...price, currency: e.target.value })} />
          <button className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Set price</button>
        </form>
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">USD → IDR FX rate</h2>
        <form onSubmit={(e) => { e.preventDefault(); manualFx.mutate({ base: "USD", quote: "IDR", rate: fx.rate, as_of: today() }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input className={input} placeholder="e.g. 16250" value={fx.rate} onChange={(e) => setFx({ rate: e.target.value })} required />
          <button className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Set FX</button>
        </form>
      </section>
    </div>
  );
}
```

- [ ] **Step 2: Write `SettingsPage.test.tsx`**

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import SettingsPage from "./SettingsPage";

test("renders settings sections", () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><SettingsPage /></QueryClientProvider>);
  expect(screen.getByText("Accounts")).toBeInTheDocument();
  expect(screen.getByText("Instruments")).toBeInTheDocument();
  expect(screen.getByText("USD → IDR FX rate")).toBeInTheDocument();
});
```

- [ ] **Step 3: Run full test suite + build + commit**

Run: `cd frontend && npm test` → all pass.
Run: `npm run build` → no type errors.
```bash
git add frontend/src && git commit -m "feat(frontend): settings page for accounts, instruments, manual price and FX"
```

---

### Task 14: End-to-end manual verification + README

**Files:** Create `frontend/README.md`. No production code changes.

- [ ] **Step 1: Write `frontend/README.md`**

```md
# Portfolio Tracker — Frontend

Vite + React + TypeScript dashboard for the Phase 1A backend.

## Dev
1. Start the backend: `cd ../backend && cargo run` (binds http://localhost:8080).
2. Start the frontend: `npm install && npm run dev` (Vite serves http://localhost:5173, proxies `/api` → `:8080`).

## Test
`npm test` (Vitest + Testing Library + MSW).

## Build
`npm run build` → static assets in `dist/`.

## API base
Defaults to `/api` (proxied in dev). Override with `VITE_API_BASE` for production.
```

- [ ] **Step 2: Manual end-to-end smoke (requires backend running)**

This is a manual checklist — run the backend on a free port, set USD/IDR FX in Settings, create an account + instrument + a buy transaction + manual price, and confirm the Dashboard shows non-zero net worth in both currencies, the allocation donut renders, and the holdings table lists the position. If port 8080 is occupied by another process on this machine, run the backend with a different bind or stop the other process first. Record the result in the commit message.

- [ ] **Step 3: Commit**

```bash
git add frontend/README.md && git commit -m "docs(frontend): add dev/test/build readme"
```

---

## Self-Review

**Spec coverage (Phase 1 dashboard requirements → task):**
- Net worth consolidated dual IDR+USD → Task 6 (NetWorthCard), Task 9 ✅
- Performance ROI + XIRR + realized/unrealized → Task 6 (PerformanceCards: XIRR, unrealized, realized; per-position ROI in Task 10) ✅
- Allocation vs target + drift + rebalance hint → Task 7 (DriftBars + AllocationDonut), Task 12 (Planner) ✅
- History chart → Task 8, Task 9 ✅
- CRUD accounts/instruments/categories/transactions → Tasks 11, 12, 13 ✅
- Manual price (reksadana NAV) + manual FX → Task 13 ✅
- Refresh prices trigger → Task 9 (Dashboard button) ✅
- Typed, zod-validated API boundary, no `any` → Tasks 3, 4 ✅
- Money as strings, parsed only for display → Task 2 (format), used throughout ✅

**Placeholder scan:** Task 5 creates intentional placeholder pages that Tasks 9–13 each fully replace — every placeholder has a replacing task. No `TODO`/`TBD` left in shipped code. The Task 1 `App.test.tsx` is intentionally rewritten in Task 5 Step 5 (App gains routing) — called out explicitly.

**Type consistency:** schema type names (`PortfolioSummary`, `Position`, `CategoryAllocation`, `Account`, `Category`, `Instrument`, `Transaction`, `Snapshot`) and hook names (`useSummary`, `useHistory`, `useAccounts`, `useCategories`, `useInstruments`, `useTransactions`, `useCreate*`, `useDelete*`, `useManualPrice`, `useManualFx`, `useRefreshPrices`) are defined in Tasks 3–4 and used consistently in Tasks 6–13. MSW handlers are added incrementally and each page test only relies on handlers added by or before its task.

**Known follow-ups (non-blocking, match backend follow-ups):**
- Transaction list shows `#instrument_id`; could join to symbol like Holdings does (minor).
- No optimistic updates; relies on query invalidation (acceptable for single-user).
- 422 deserialize errors from the backend surface as raw text in mutation error messages (mirrors backend M-1 follow-up).

---

## Execution Handoff

Plan complete — 14 tasks. Backend must be running for the Task 14 manual smoke; all automated tests use MSW and need no backend.
