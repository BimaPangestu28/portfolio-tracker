# Frontend Redesign with shadcn/ui — Design

**Date:** 2026-06-01
**Status:** Approved (pending spec review)
**Scope:** Restyle the existing Vite + React + TypeScript frontend (`frontend/`) using shadcn/ui. No backend or API changes.

## Goal

Replace the current ad-hoc Tailwind styling with a consistent shadcn/ui design system: a sidebar dashboard layout, light/dark theming, and shadcn components across all five pages — without changing application behavior, data flow, or breaking the existing test suite.

## Non-Goals (YAGNI)

- No changes to API client, React Query hooks, zod schemas, or formatting utilities.
- No new features (auth, new pages, new data).
- No swapping the chart library — recharts stays, restyled to theme tokens.
- No backend changes.

## Context (current state)

- **Stack:** Vite 5, React 18, TypeScript strict, Tailwind 3.4 (bare config), React Router 6, TanStack Query 5, recharts, zod.
- **Pages:** Dashboard, Holdings, Transactions, Planner, Settings (`src/pages/`).
- **Components:** `Layout` (top-nav), `StatCard`, `NetWorthCard`, `AllocationDonut`, `DriftBars`, `HistoryChart`, `PerformanceCards`, `QueryState`.
- **Tests:** Vitest + Testing Library + MSW, one per page/component. All assert on **visible text / headings / aria-labels** — none drive `<select>` interactions. This makes Radix-based shadcn `Select` safe to adopt.
- **No `@/` path alias** currently configured.

## Design

### 1. Foundation (shadcn setup in Vite)

- **Path alias `@/` → `src/`**: add `baseUrl` + `paths` to `tsconfig.json`; add `resolve.alias` to `vite.config.ts`.
- **Re-apply dev proxy** `/api` → `http://localhost:8081` in `vite.config.ts` (it reverted to `:8080`; backend binds `:8081`).
- **Dependencies:** `class-variance-authority`, `clsx`, `tailwind-merge`, `tailwindcss-animate`, `lucide-react`, plus Radix primitives pulled per component.
- **Config files:** `components.json`; `src/lib/utils.ts` exporting `cn()`.
- **Tailwind config:** extend theme with shadcn CSS-variable color tokens, border radius, and the `tailwindcss-animate` keyframes/plugin.
- **`src/index.css`:** add shadcn `:root` and `.dark` HSL token blocks plus the `@layer base` border/background defaults, keeping the existing `@tailwind` directives.
- **ThemeProvider (custom, small):** store `light | dark | system` in `localStorage`, toggle `.dark` on `<html>`, default to system preference. No `next-themes` (this is Vite, not Next).

### 2. shadcn components to add

`button`, `card`, `input`, `label`, `select`, `table`, `badge`, `separator`, `dropdown-menu`, `skeleton`, `tooltip`, `sheet`, `sonner` (toast), and `sidebar` (`SidebarProvider` / `Sidebar` / `SidebarTrigger` / etc.).

### 3. Layout (`Layout.tsx` → `AppLayout`)

- **Left sidebar:** brand "📊 Portfolio" + nav items (Dashboard, Holdings, Transactions, Planner, Settings), each with a lucide icon. Collapsible; on mobile it opens as a `Sheet` via `SidebarTrigger`.
- **Thin topbar:** sidebar trigger (mobile) + current page title + `ModeToggle` (light/dark) on the right.
- Content rendered via React Router `<Outlet/>`.
- The nav must keep the literal text "Dashboard" so `App.test.tsx` (`getAllByText("Dashboard")`) stays green.

### 4. Per-page redesign

| Page | Changes |
|---|---|
| **Dashboard** | `StatCard` / `NetWorthCard` / `PerformanceCards` → shadcn `Card`. Charts (donut / drift / history) stay recharts, wrapped in `Card`, colored via `--chart-1..5` tokens. Refresh button → `Button` with a lucide spinner; keep "Refresh prices" / "Refreshing…" text. |
| **Holdings** | Table → shadcn `Table`. "⚠ stale" → `Badge`. PnL coloring via semantic emerald/red. Empty-state text "No positions yet…" preserved. |
| **Transactions** | Form → `Card` with grid of `Input` / `Select` / `Label`. Submit `Button` keeps text "Add transaction" / "Adding…". `Sonner` toast on mutation success/error. Table → shadcn `Table`; delete → ghost `Button` with trash icon. Empty-state "No transactions yet." preserved. |
| **Planner** | Form `Card`; category list → `Card`/rows; total-target indicator → `Badge`/`Alert`; optional `Progress` bar for target vs actual. Keep total-target text semantics. |
| **Settings** | Each section → `Card` (`CardHeader`/`CardTitle`). Forms use `Input` / `Select` / `Label` / `Button`; lists → tidy rows; toasts. Headings "Accounts", "Instruments", "USD → IDR FX rate" preserved. |

### 5. Accessibility & tests

- **All existing `aria-label`s preserved** on inputs/selects.
- Visible text and headings asserted by tests kept byte-identical where tested.
- **Definition of done:** `npm test` green and `npm run build` passing.

### 6. Components mapping summary

- `StatCard` → rewritten on top of shadcn `Card` (same props/API).
- `NetWorthCard`, `PerformanceCards`, `AllocationDonut`, `DriftBars`, `HistoryChart`, `QueryState` → restyled internally; public props unchanged so pages and tests keep working.
- `Layout` → `AppLayout` (sidebar shell).

## Risks & mitigations

- **Radix `Select` vs native `<select>`:** tests don't drive selects, so swap is safe. aria-labels retained for a11y.
- **recharts theming:** colors come from CSS variables; verify both light and dark render legibly.
- **Path alias regressions:** Vitest must resolve `@/` too — alias added in `vite.config.ts` covers both Vite and Vitest (shared config).
- **`tsc -b` config emit:** keep the `tsconfig.node.json` output redirected so build does not re-emit a stale `vite.config.js` (separate prior fix; do not regress).

## Verification

1. `npm test` — all suites green.
2. `npm run build` — type-check + Vite build pass; no stale `vite.config.js` emitted.
3. Manual: run `make dev`, confirm sidebar nav, light/dark toggle, and each page renders against the backend on `:8081`.
