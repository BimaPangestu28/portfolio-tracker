# Portfolio Tracker — Frontend Redesign Brief

**Hand this whole file to a Claude design/frontend session as the prompt.** It is the
instruction set for redesigning the UI. Behavior/data must not change — this is a
**presentation-only** redesign on top of the existing API + hooks.

---

## 0. Hard constraints (do NOT break these)

- **Do not change the data layer.** Keep `src/api/{client,schemas,hooks}.ts` and every TanStack
  Query hook exactly as-is. Money/quantities stay **strings** (parse only for display via
  `src/lib/format.ts` — `formatIDR`/`formatUSD`/`formatPct`). No `any`; strict TS.
- **No backend changes.** Pure frontend. Dev proxy stays `:8080` (`vite.config.ts`).
- **Keep tests green.** Update component tests when markup changes, but don't weaken assertions
  that check real behavior (e.g. "shows net worth", "out of band badge", empty states). Run
  `npm test` + `npm run build` after each page; both must pass.
- **Work incrementally, one page per commit.** Conventional commits (`feat(ui): redesign dashboard`).
- This repo already has a partial shadcn migration merged (`src/components/ui/`, a theme provider,
  a sidebar/mode-toggle, and restyled Chat + Connectors). **Build on it; make every page
  consistent with it.** Do not introduce a second component library or a competing style.

## 1. Product context

A **single-user, self-hosted personal investment + finance tracker** (Bahasa Indonesia user,
finance domain). Pages: **Dashboard, Holdings, Transactions, Planner (allocation targets),
Budget (cashflow), Import (LLM/CSV review queue), Connectors (on-chain/exchange sync), Chat**
(portfolio Q&A). Often checked **on a phone** → mobile-first matters. Values shown in **dual
currency IDR + USD**.

## 2. Design direction

**"Calm fintech dashboard."** Trustworthy, data-dense but uncluttered, modern. Think
Linear/Vercel-clean meets a banking app. Not flashy — numbers are the hero.

- **Component system:** shadcn/ui (Radix + Tailwind) + **lucide-react** icons. Use the existing
  `components/ui/*` primitives (Card, Button, Input, Select, Table, Badge, Tabs, Dialog/Sheet,
  Skeleton, Sonner/Toast, Tooltip). Add more shadcn primitives as needed via the same pattern.
- **Theme:** light + dark, toggle in the shell (already started). Define tokens in CSS variables
  (shadcn convention): `--background, --foreground, --card, --muted, --primary, --border, --ring`
  plus **semantic finance tokens**: `--gain` (green), `--loss` (red), `--warn` (amber, for
  out-of-band / stale). Ensure WCAG AA contrast in both themes.
- **Typography:** one clean sans (Inter or the system stack already configured). Tabular/monospaced
  numerals for money columns (`font-variant-numeric: tabular-nums`) so figures align.
- **Density:** comfortable on desktop, compact tables; generous touch targets on mobile.
- **Color for data:** a fixed 6–8 hue categorical palette for allocation categories (reused by the
  donut, the drift bars, and any legends so a category is always the same color). Gain=green,
  loss=red, on-target/neutral=muted, warning=amber.

## 3. App shell & navigation

- **Responsive sidebar** (collapsible) on desktop → **bottom tab bar or hamburger Sheet** on
  mobile. Items: Dashboard, Holdings, Transactions, Planner, Budget, Import, Connectors, Chat.
  Active state clear. Brand mark top-left (the 📊 → a proper lucide icon + "Portfolio").
- **Top bar:** page title, a **base-currency / theme toggle**, a global "Refresh prices" action,
  and a quick "Ask" affordance that deep-links to Chat.
- Keep routes unchanged (react-router paths: `/`, `/holdings`, `/transactions`, `/planner`,
  `/budget`, `/import`, `/connectors`, `/chat`).

## 4. Page-by-page

**Dashboard** — the showcase.
- Hero: **net worth** big, dual currency (IDR primary, USD secondary), with a small period delta
  if derivable. Then a row of **KPI stat cards**: Unrealized P&L, Realized P&L, XIRR (color by
  sign). Use Card + tabular nums + up/down arrow icons.
- **Allocation**: donut (Recharts) using the categorical palette, beside **target-vs-actual drift
  bars** — each category a horizontal bar with a target marker; amber when `out_of_band`, with the
  rebalance hint ("Buy Rp X" / "Trim Rp X"). Empty state when no categories.
- **Value history**: clean Recharts area/line, IDR axis with `formatIDR`, soft gradient fill,
  tooltip styled to the theme, graceful "no history yet" empty state.

**Holdings** — shadcn Table: instrument, qty (tabular), avg cost, last price (+ **stale badge**),
market value (IDR), unrealized P&L (green/red, with %). Sortable headers if cheap. Empty state.

**Transactions** — Table + an **Add transaction** in a Dialog/Sheet (not an inline form): selects
for account/instrument/type, fields for qty/price/fee/currency/date. Row delete with confirm.
Type rendered as a colored Badge.

**Planner** — category cards with target % + tolerance band, a 100%-sum indicator, and live
actual% vs target. Add-category in a Dialog.

**Budget** — month picker; three KPI cards (Income / Expense / Net); per-category progress bars
(actual vs `monthly_budget`, amber/red over budget); cashflow entry + category in Dialogs; recent
cashflow list.

**Import** — clearer **review queue**: a dropzone for screenshots/PDFs and a CSV panel; each staged
item as a Card/row with `doc_type` + **needs-attention** badge, editable fields, instrument/account
selectors with inline-create, **Confirm / Reject** buttons. Make the "nothing auto-commits, review
first" model obvious.

**Connectors** — already shadcn; align it: connector cards (kind icon, label, last-synced relative
time), "Sync now" with the inserted/staged/skipped result as a toast, add-connector Dialog (api_key
field stays `type=password`).

**Chat** — already shadcn; polish: message bubbles (assistant left/user right), markdown-ish
rendering of replies, "thinking…" indicator, sticky composer, autoscroll. Note WhatsApp parity
(same answers via the Baileys gateway).

## 5. States & polish (every page)

- **Loading:** shadcn Skeletons, not "Loading…" text.
- **Empty:** a friendly illustration/icon + one line + a primary action.
- **Error:** toast (Sonner) + inline message; never a blank screen.
- **Mutations:** disable + spinner on submit; success toast.
- **Numbers:** tabular nums, IDR grouped (`Rp 4.875.000`), USD `$300.00`, signed % with color,
  stale prices flagged.
- **Motion:** subtle (150–200ms) on hover/expand; respect `prefers-reduced-motion`.

## 6. Process for the executor

1. Read `superpowers:brainstorming` is NOT needed (this brief is the design) — go straight to
   building, but you MAY produce 1–2 quick mockups for the Dashboard hero + sidebar before
   committing if the layout is ambiguous.
2. Establish the theme tokens + shell first (sidebar/topbar, light/dark), then migrate pages
   Dashboard → Holdings → Transactions → Planner → Budget → Import → Connectors → Chat.
3. One page per commit; `npm test` + `npm run build` green each time; update component tests for
   new markup (keep behavior assertions).
4. Don't touch hooks/schemas/format. If a primitive is missing, add it the shadcn way under
   `components/ui/`.
5. Verify at the end by running the app (`npm run dev`, backend on :8080) and screenshotting
   Dashboard + one data-heavy page in both light and dark.

## 7. Acceptance

- All 8 pages visually consistent on the shadcn system, light + dark, mobile + desktop.
- Zero behavior/data changes; all tests pass; build clean; no `any`.
- Dashboard reads like a real fintech app at a glance: net worth, P&L, allocation health, trend.
