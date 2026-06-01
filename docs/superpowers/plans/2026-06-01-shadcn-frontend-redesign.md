# shadcn/ui Frontend Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restyle the existing Vite + React + TypeScript frontend with shadcn/ui — sidebar dashboard layout, light/dark theming, shadcn components on all five pages — without changing behavior or breaking the test suite.

**Architecture:** Add a `@/` path alias and the shadcn token system (CSS variables in `index.css`, theme tokens in `tailwind.config.js`). Pull shadcn component primitives into `src/components/ui/` via the CLI. Replace the top-nav `Layout` with a sidebar `AppLayout`. Restyle shared components (keeping their public props) and rewrite each page's JSX to use shadcn `Card`/`Table`/`Input`/`Select`/`Button`/`Badge`, preserving every visible string and `aria-label` the tests assert on.

**Tech Stack:** Vite 5, React 18, TypeScript (strict), Tailwind 3.4, shadcn/ui (new-york, slate), Radix UI, lucide-react, sonner, recharts (kept), Vitest + Testing Library + MSW.

**Working directory:** all paths are relative to `frontend/` unless noted. Run all `npm` commands from `frontend/`.

**Definition of done:** `npm test` green and `npm run build` passing, with sidebar nav + light/dark toggle working against the backend on `:8081`.

---

## File Structure

**Created:**
- `frontend/components.json` — shadcn config
- `frontend/src/lib/utils.ts` — `cn()` helper
- `frontend/src/components/theme-provider.tsx` — Vite theme context (light/dark/system)
- `frontend/src/components/mode-toggle.tsx` — theme dropdown button
- `frontend/src/components/AppLayout.tsx` — sidebar shell (replaces `Layout.tsx`)
- `frontend/src/components/ui/*` — shadcn primitives (CLI-generated): `button`, `card`, `input`, `label`, `select`, `table`, `badge`, `separator`, `dropdown-menu`, `skeleton`, `tooltip`, `sheet`, `sidebar`, `sonner`

**Modified:**
- `frontend/tsconfig.json` — `baseUrl` + `paths`
- `frontend/vite.config.ts` — `@` alias + restore `/api` → `:8081` proxy
- `frontend/tailwind.config.js` — shadcn theme tokens + animate plugin
- `frontend/src/index.css` — token blocks (`:root`, `.dark`) + base layer
- `frontend/src/test/setup.ts` — `matchMedia` mock (sidebar uses it)
- `frontend/src/main.tsx` — wrap in `ThemeProvider` + mount `Toaster`
- `frontend/src/App.tsx` — import `AppLayout`
- `frontend/src/components/{StatCard,NetWorthCard,PerformanceCards,AllocationDonut,DriftBars,HistoryChart,QueryState}.tsx` — restyle to tokens (props unchanged)
- `frontend/src/pages/{Dashboard,Holdings,Transactions,Planner,Settings}Page.tsx` — shadcn rewrite
- `frontend/README.md` — note shadcn

**Deleted:**
- `frontend/src/components/Layout.tsx` (replaced by `AppLayout.tsx`)

---

## Task 1: Foundation — alias, deps, shadcn config, tokens

**Files:**
- Modify: `tsconfig.json`, `vite.config.ts`, `tailwind.config.js`, `src/index.css`
- Create: `src/lib/utils.ts`, `components.json`

- [ ] **Step 1: Install dependencies**

Run:
```bash
npm i class-variance-authority clsx tailwind-merge tailwindcss-animate lucide-react sonner
```
Expected: added to `dependencies`, no errors.

- [ ] **Step 2: Add `@/` path alias to `tsconfig.json`**

In `compilerOptions`, add these two keys (keep all existing keys):
```jsonc
"baseUrl": ".",
"paths": { "@/*": ["./src/*"] }
```

- [ ] **Step 3: Update `vite.config.ts` (alias + restore :8081 proxy)**

Replace the whole file with:
```ts
/// <reference types="vitest" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  server: {
    proxy: {
      "/api": { target: "http://localhost:8081", changeOrigin: true, rewrite: (p) => p.replace(/^\/api/, "") },
    },
  },
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
});
```

- [ ] **Step 4: Create `src/lib/utils.ts`**

```ts
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
```

- [ ] **Step 5: Create `components.json`**

```json
{
  "$schema": "https://ui.shadcn.com/schema.json",
  "style": "new-york",
  "rsc": false,
  "tsx": true,
  "tailwind": {
    "config": "tailwind.config.js",
    "css": "src/index.css",
    "baseColor": "slate",
    "cssVariables": true,
    "prefix": ""
  },
  "aliases": {
    "components": "@/components",
    "utils": "@/lib/utils",
    "ui": "@/components/ui",
    "lib": "@/lib",
    "hooks": "@/hooks"
  },
  "iconLibrary": "lucide"
}
```

- [ ] **Step 6: Replace `tailwind.config.js`**

```js
import tailwindcssAnimate from "tailwindcss-animate";

/** @type {import('tailwindcss').Config} */
export default {
  darkMode: ["class"],
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    container: { center: true, padding: "2rem", screens: { "2xl": "1400px" } },
    extend: {
      colors: {
        border: "hsl(var(--border))",
        input: "hsl(var(--input))",
        ring: "hsl(var(--ring))",
        background: "hsl(var(--background))",
        foreground: "hsl(var(--foreground))",
        primary: { DEFAULT: "hsl(var(--primary))", foreground: "hsl(var(--primary-foreground))" },
        secondary: { DEFAULT: "hsl(var(--secondary))", foreground: "hsl(var(--secondary-foreground))" },
        destructive: { DEFAULT: "hsl(var(--destructive))", foreground: "hsl(var(--destructive-foreground))" },
        muted: { DEFAULT: "hsl(var(--muted))", foreground: "hsl(var(--muted-foreground))" },
        accent: { DEFAULT: "hsl(var(--accent))", foreground: "hsl(var(--accent-foreground))" },
        popover: { DEFAULT: "hsl(var(--popover))", foreground: "hsl(var(--popover-foreground))" },
        card: { DEFAULT: "hsl(var(--card))", foreground: "hsl(var(--card-foreground))" },
        chart: {
          1: "hsl(var(--chart-1))",
          2: "hsl(var(--chart-2))",
          3: "hsl(var(--chart-3))",
          4: "hsl(var(--chart-4))",
          5: "hsl(var(--chart-5))",
        },
        sidebar: {
          DEFAULT: "hsl(var(--sidebar-background))",
          foreground: "hsl(var(--sidebar-foreground))",
          primary: "hsl(var(--sidebar-primary))",
          "primary-foreground": "hsl(var(--sidebar-primary-foreground))",
          accent: "hsl(var(--sidebar-accent))",
          "accent-foreground": "hsl(var(--sidebar-accent-foreground))",
          border: "hsl(var(--sidebar-border))",
          ring: "hsl(var(--sidebar-ring))",
        },
      },
      borderRadius: { lg: "var(--radius)", md: "calc(var(--radius) - 2px)", sm: "calc(var(--radius) - 4px)" },
      keyframes: {
        "accordion-down": { from: { height: "0" }, to: { height: "var(--radix-accordion-content-height)" } },
        "accordion-up": { from: { height: "var(--radix-accordion-content-height)" }, to: { height: "0" } },
      },
      animation: { "accordion-down": "accordion-down 0.2s ease-out", "accordion-up": "accordion-up 0.2s ease-out" },
    },
  },
  plugins: [tailwindcssAnimate],
};
```

- [ ] **Step 7: Replace `src/index.css`**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

@layer base {
  :root {
    --background: 0 0% 100%;
    --foreground: 222.2 84% 4.9%;
    --card: 0 0% 100%;
    --card-foreground: 222.2 84% 4.9%;
    --popover: 0 0% 100%;
    --popover-foreground: 222.2 84% 4.9%;
    --primary: 222.2 47.4% 11.2%;
    --primary-foreground: 210 40% 98%;
    --secondary: 210 40% 96.1%;
    --secondary-foreground: 222.2 47.4% 11.2%;
    --muted: 210 40% 96.1%;
    --muted-foreground: 215.4 16.3% 46.9%;
    --accent: 210 40% 96.1%;
    --accent-foreground: 222.2 47.4% 11.2%;
    --destructive: 0 84.2% 60.2%;
    --destructive-foreground: 210 40% 98%;
    --border: 214.3 31.8% 91.4%;
    --input: 214.3 31.8% 91.4%;
    --ring: 222.2 84% 4.9%;
    --radius: 0.5rem;
    --chart-1: 12 76% 61%;
    --chart-2: 173 58% 39%;
    --chart-3: 197 37% 24%;
    --chart-4: 43 74% 66%;
    --chart-5: 27 87% 67%;
    --sidebar-background: 0 0% 98%;
    --sidebar-foreground: 240 5.3% 26.1%;
    --sidebar-primary: 240 5.9% 10%;
    --sidebar-primary-foreground: 0 0% 98%;
    --sidebar-accent: 240 4.8% 95.9%;
    --sidebar-accent-foreground: 240 5.9% 10%;
    --sidebar-border: 220 13% 91%;
    --sidebar-ring: 217.2 91.2% 59.8%;
  }
  .dark {
    --background: 222.2 84% 4.9%;
    --foreground: 210 40% 98%;
    --card: 222.2 84% 4.9%;
    --card-foreground: 210 40% 98%;
    --popover: 222.2 84% 4.9%;
    --popover-foreground: 210 40% 98%;
    --primary: 210 40% 98%;
    --primary-foreground: 222.2 47.4% 11.2%;
    --secondary: 217.2 32.6% 17.5%;
    --secondary-foreground: 210 40% 98%;
    --muted: 217.2 32.6% 17.5%;
    --muted-foreground: 215 20.2% 65.1%;
    --accent: 217.2 32.6% 17.5%;
    --accent-foreground: 210 40% 98%;
    --destructive: 0 62.8% 30.6%;
    --destructive-foreground: 210 40% 98%;
    --border: 217.2 32.6% 17.5%;
    --input: 217.2 32.6% 17.5%;
    --ring: 212.7 26.8% 83.9%;
    --chart-1: 220 70% 50%;
    --chart-2: 160 60% 45%;
    --chart-3: 30 80% 55%;
    --chart-4: 280 65% 60%;
    --chart-5: 340 75% 55%;
    --sidebar-background: 240 5.9% 10%;
    --sidebar-foreground: 240 4.8% 95.9%;
    --sidebar-primary: 224.3 76.3% 48%;
    --sidebar-primary-foreground: 0 0% 100%;
    --sidebar-accent: 240 3.7% 15.9%;
    --sidebar-accent-foreground: 240 4.8% 95.9%;
    --sidebar-border: 240 3.7% 15.9%;
    --sidebar-ring: 217.2 91.2% 59.8%;
  }
}

@layer base {
  * {
    @apply border-border;
  }
  body {
    @apply bg-background text-foreground;
  }
}
```

- [ ] **Step 8: Verify existing tests + build still pass**

Run: `npm test`
Expected: all suites PASS (no UI changed yet; config only).
Run: `npm run build`
Expected: type-check + Vite build succeed. Confirm no `vite.config.js` re-emitted: `ls vite.config.*` shows only `vite.config.ts`.

- [ ] **Step 9: Commit**

```bash
git add frontend/tsconfig.json frontend/vite.config.ts frontend/tailwind.config.js frontend/src/index.css frontend/src/lib/utils.ts frontend/components.json frontend/package.json frontend/package-lock.json
git commit -m "feat(frontend): shadcn foundation — path alias, theme tokens, deps"
```

---

## Task 2: Pull shadcn primitives + mock matchMedia

**Files:**
- Create: `src/components/ui/*` (CLI)
- Modify: `src/test/setup.ts`

- [ ] **Step 1: Add shadcn components via CLI**

Run:
```bash
npx shadcn@latest add button card input label select table badge separator dropdown-menu skeleton tooltip sheet sidebar sonner --yes
```
Expected: files created under `src/components/ui/`. The `sidebar` add also pulls `use-mobile` hook (`src/hooks/use-mobile.ts` or `src/components/ui/use-mobile`). If the CLI reports a component already exists, accept overwrite (`--overwrite` if needed).

- [ ] **Step 2: Verify primitives landed**

Run: `ls src/components/ui`
Expected: includes `button.tsx card.tsx input.tsx label.tsx select.tsx table.tsx badge.tsx separator.tsx dropdown-menu.tsx skeleton.tsx tooltip.tsx sheet.tsx sidebar.tsx sonner.tsx`.

- [ ] **Step 3: Rewrite the generated `src/components/ui/sonner.tsx`**

The CLI version imports `next-themes` (not installed in this Vite app). Replace the file with our theme-aware version:
```tsx
import { useTheme } from "@/components/theme-provider";
import { Toaster as Sonner, type ToasterProps } from "sonner";

export function Toaster({ ...props }: ToasterProps) {
  const { theme = "system" } = useTheme();

  return (
    <Sonner
      theme={theme as ToasterProps["theme"]}
      className="toaster group"
      style={
        {
          "--normal-bg": "hsl(var(--popover))",
          "--normal-text": "hsl(var(--popover-foreground))",
          "--normal-border": "hsl(var(--border))",
        } as React.CSSProperties
      }
      {...props}
    />
  );
}
```
(`@/components/theme-provider` is created in Task 3. Build/lint of this file is verified after Task 3.)

- [ ] **Step 4: Add `matchMedia` mock to `src/test/setup.ts`**

The shadcn sidebar's `useIsMobile` calls `window.matchMedia`, which jsdom lacks. Add this block right after the existing `ResizeObserver` mock (keep everything else):
```ts
// shadcn sidebar (useIsMobile) relies on matchMedia which jsdom lacks
if (!window.matchMedia) {
  window.matchMedia = (query: string) =>
    ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    }) as unknown as MediaQueryList;
}
```

- [ ] **Step 5: Verify tests still pass**

Run: `npm test`
Expected: all suites PASS (primitives unused yet; sonner.tsx not imported until Task 3, so no break).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/ui frontend/src/hooks frontend/src/test/setup.ts frontend/package.json frontend/package-lock.json
git commit -m "feat(frontend): add shadcn ui primitives + matchMedia test mock"
```

---

## Task 3: Theme provider, mode toggle, wire main.tsx

**Files:**
- Create: `src/components/theme-provider.tsx`, `src/components/mode-toggle.tsx`
- Modify: `src/main.tsx`

- [ ] **Step 1: Create `src/components/theme-provider.tsx`**

```tsx
import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

type Theme = "dark" | "light" | "system";

type ThemeProviderState = { theme: Theme; setTheme: (theme: Theme) => void };

const initialState: ThemeProviderState = { theme: "system", setTheme: () => null };

const ThemeProviderContext = createContext<ThemeProviderState>(initialState);

export function ThemeProvider({
  children,
  defaultTheme = "system",
  storageKey = "portfolio-theme",
}: {
  children: ReactNode;
  defaultTheme?: Theme;
  storageKey?: string;
}) {
  const [theme, setThemeState] = useState<Theme>(
    () => (localStorage.getItem(storageKey) as Theme) || defaultTheme,
  );

  useEffect(() => {
    const root = window.document.documentElement;
    root.classList.remove("light", "dark");
    if (theme === "system") {
      const systemTheme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
      root.classList.add(systemTheme);
      return;
    }
    root.classList.add(theme);
  }, [theme]);

  const value: ThemeProviderState = {
    theme,
    setTheme: (next: Theme) => {
      localStorage.setItem(storageKey, next);
      setThemeState(next);
    },
  };

  return <ThemeProviderContext.Provider value={value}>{children}</ThemeProviderContext.Provider>;
}

// Note: returns the default context (never throws) so components like ModeToggle
// render safely even when not wrapped in a provider (e.g. App.test renders <App/> directly).
export function useTheme() {
  return useContext(ThemeProviderContext);
}
```

- [ ] **Step 2: Create `src/components/mode-toggle.tsx`**

```tsx
import { Moon, Sun } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTheme } from "@/components/theme-provider";

export function ModeToggle() {
  const { setTheme } = useTheme();
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button variant="outline" size="icon">
          <Sun className="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0" />
          <Moon className="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
          <span className="sr-only">Toggle theme</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={() => setTheme("light")}>Light</DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme("dark")}>Dark</DropdownMenuItem>
        <DropdownMenuItem onClick={() => setTheme("system")}>System</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
```

- [ ] **Step 3: Wire `src/main.tsx`**

Replace the whole file with:
```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter } from "react-router-dom";
import App from "./App";
import { ThemeProvider } from "@/components/theme-provider";
import { Toaster } from "@/components/ui/sonner";
import "./index.css";

const queryClient = new QueryClient();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider defaultTheme="system" storageKey="portfolio-theme">
      <QueryClientProvider client={queryClient}>
        <BrowserRouter>
          <App />
          <Toaster />
        </BrowserRouter>
      </QueryClientProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 4: Verify build + tests**

Run: `npm run build`
Expected: PASS (sonner.tsx now resolves `useTheme`).
Run: `npm test`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/theme-provider.tsx frontend/src/components/mode-toggle.tsx frontend/src/main.tsx
git commit -m "feat(frontend): theme provider + mode toggle, mount Toaster"
```

---

## Task 4: Sidebar layout (AppLayout)

**Files:**
- Create: `src/components/AppLayout.tsx`
- Delete: `src/components/Layout.tsx`
- Modify: `src/App.tsx`
- Test: `src/App.test.tsx` (existing — must stay green)

- [ ] **Step 1: Create `src/components/AppLayout.tsx`**

```tsx
import { NavLink, Outlet, useLocation } from "react-router-dom";
import { LayoutDashboard, Wallet, ArrowLeftRight, Target, Settings } from "lucide-react";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarInset,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import { ModeToggle } from "@/components/mode-toggle";

const links = [
  { to: "/", label: "Dashboard", icon: LayoutDashboard, end: true },
  { to: "/holdings", label: "Holdings", icon: Wallet, end: false },
  { to: "/transactions", label: "Transactions", icon: ArrowLeftRight, end: false },
  { to: "/planner", label: "Planner", icon: Target, end: false },
  { to: "/settings", label: "Settings", icon: Settings, end: false },
];

export default function AppLayout() {
  const location = useLocation();
  const current = links.find((l) =>
    l.end ? location.pathname === l.to : location.pathname.startsWith(l.to),
  );

  return (
    <SidebarProvider>
      <Sidebar>
        <SidebarHeader>
          <div className="flex items-center gap-2 px-2 py-1.5 text-base font-semibold">
            <span aria-hidden="true">📊</span> Portfolio
          </div>
        </SidebarHeader>
        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupContent>
              <SidebarMenu>
                {links.map((l) => (
                  <SidebarMenuItem key={l.to}>
                    <SidebarMenuButton asChild tooltip={l.label}>
                      <NavLink to={l.to} end={l.end}>
                        <l.icon />
                        <span>{l.label}</span>
                      </NavLink>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
      </Sidebar>
      <SidebarInset>
        <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 h-4" />
          <h1 className="text-sm font-semibold">{current?.label ?? "Portfolio"}</h1>
          <div className="ml-auto">
            <ModeToggle />
          </div>
        </header>
        <main className="flex-1 p-4 md:p-6">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
```

- [ ] **Step 2: Point `src/App.tsx` at AppLayout**

Replace the whole file with:
```tsx
import { Routes, Route } from "react-router-dom";
import AppLayout from "./components/AppLayout";
import DashboardPage from "./pages/DashboardPage";
import HoldingsPage from "./pages/HoldingsPage";
import TransactionsPage from "./pages/TransactionsPage";
import PlannerPage from "./pages/PlannerPage";
import SettingsPage from "./pages/SettingsPage";

export default function App() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
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

- [ ] **Step 3: Delete the old layout**

Run: `git rm frontend/src/components/Layout.tsx`

- [ ] **Step 4: Verify App.test stays green**

Run: `npm test -- src/App.test.tsx`
Expected: PASS. (`App.test` renders at `/`, so "Dashboard" appears in both the sidebar nav and the header `h1` → `getAllByText("Dashboard").length` ≥ 1.)

- [ ] **Step 5: Full test + build**

Run: `npm test`
Expected: all PASS.
Run: `npm run build`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/AppLayout.tsx frontend/src/App.tsx
git rm frontend/src/components/Layout.tsx
git commit -m "feat(frontend): sidebar AppLayout with mode toggle"
```

---

## Task 5: Restyle shared cards (StatCard, NetWorthCard, PerformanceCards)

**Files:**
- Modify: `src/components/StatCard.tsx`
- Test: `src/components/PerformanceCards.test.tsx` (existing — keep green)

`NetWorthCard` and `PerformanceCards` consume `StatCard` and need no changes; restyling `StatCard` flows through. Keep `StatCard`'s props (`label`, `value`, `sub`, `tone`) identical.

- [ ] **Step 1: Rewrite `src/components/StatCard.tsx` on shadcn Card**

```tsx
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

export function StatCard({
  label,
  value,
  sub,
  tone,
}: {
  label: string;
  value: string;
  sub?: string;
  tone?: "pos" | "neg" | "neutral";
}) {
  const color =
    tone === "pos" ? "text-emerald-600 dark:text-emerald-400" : tone === "neg" ? "text-red-600 dark:text-red-400" : "text-foreground";
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-xs font-medium uppercase tracking-wide text-muted-foreground">{label}</CardTitle>
      </CardHeader>
      <CardContent>
        <div className={cn("text-2xl font-semibold", color)}>{value}</div>
        {sub && <div className="mt-1 text-sm text-muted-foreground">{sub}</div>}
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 2: Verify PerformanceCards test + suite**

Run: `npm test -- src/components/PerformanceCards.test.tsx`
Expected: PASS (label/value text unchanged).
Run: `npm test`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/StatCard.tsx
git commit -m "feat(frontend): StatCard on shadcn Card with semantic tones"
```

---

## Task 6: Restyle charts + helpers (AllocationDonut, DriftBars, HistoryChart, QueryState)

**Files:**
- Modify: `src/components/AllocationDonut.tsx`, `src/components/DriftBars.tsx`, `src/components/HistoryChart.tsx`, `src/components/QueryState.tsx`
- Test: `src/components/DriftBars.test.tsx`, `src/components/HistoryChart.test.tsx` (existing — keep green; preserve `data-testid="oob-<id>"` and empty-state strings)

- [ ] **Step 1: Rewrite `src/components/QueryState.tsx`**

```tsx
import type { ReactNode } from "react";

export function QueryState({ isLoading, error, children }: { isLoading: boolean; error: unknown; children: ReactNode }) {
  if (isLoading) return <div className="p-4 text-muted-foreground">Loading…</div>;
  if (error) return <div className="p-4 text-destructive">Error: {error instanceof Error ? error.message : "unknown"}</div>;
  return <>{children}</>;
}
```

- [ ] **Step 2: Rewrite `src/components/AllocationDonut.tsx`**

Use chart tokens and a card surface (`bg-card`); keep the empty-state string and recharts structure.
```tsx
import { PieChart, Pie, Cell, ResponsiveContainer, Tooltip, Legend } from "recharts";
import type { CategoryAllocation } from "../api/schemas";

const COLORS = [
  "hsl(var(--chart-1))",
  "hsl(var(--chart-2))",
  "hsl(var(--chart-3))",
  "hsl(var(--chart-4))",
  "hsl(var(--chart-5))",
];

export function AllocationDonut({ allocation }: { allocation: CategoryAllocation[] }) {
  const data = allocation
    .map((c) => ({ name: c.name, value: Number(c.actual_value_idr) }))
    .filter((d) => d.value > 0);
  if (data.length === 0) return <div className="text-sm text-muted-foreground">No holdings to allocate.</div>;
  return (
    <div className="h-64 w-full rounded-lg border bg-card p-2">
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie data={data} dataKey="value" nameKey="name" innerRadius="55%" outerRadius="80%">
            {data.map((_, i) => (
              <Cell key={i} fill={COLORS[i % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip
            contentStyle={{
              background: "hsl(var(--popover))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "var(--radius)",
              color: "hsl(var(--popover-foreground))",
            }}
          />
          <Legend />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}
```

- [ ] **Step 3: Rewrite `src/components/DriftBars.tsx`**

Preserve `data-testid={`oob-${c.category_id}`}` and the "No categories yet." / "out of band" strings; swap gray/white/blue utility colors for tokens.
```tsx
import { formatIDR, formatPct } from "../lib/format";
import type { CategoryAllocation } from "../api/schemas";

export function DriftBars({ allocation }: { allocation: CategoryAllocation[] }) {
  if (allocation.length === 0) return <div className="text-sm text-muted-foreground">No categories yet.</div>;
  return (
    <div className="space-y-3">
      {allocation.map((c) => {
        const actual = Number(c.actual_pct);
        const target = Number(c.target_pct);
        const reb = Number(c.rebalance_idr);
        return (
          <div key={c.category_id} className="rounded-lg border bg-card p-3">
            <div className="flex items-center justify-between text-sm">
              <span className="font-medium">
                {c.name}
                {c.out_of_band && (
                  <span
                    data-testid={`oob-${c.category_id}`}
                    className="ml-2 rounded bg-destructive/10 px-1.5 py-0.5 text-xs text-destructive"
                  >
                    out of band
                  </span>
                )}
              </span>
              <span className="text-muted-foreground">
                {formatPct(c.actual_pct)} / target {formatPct(c.target_pct)} (drift {formatPct(c.drift_pct)})
              </span>
            </div>
            <div className="mt-2 h-2 w-full rounded bg-muted">
              <div
                className={`h-2 rounded ${c.out_of_band ? "bg-destructive" : "bg-primary"}`}
                style={{ width: `${Math.min(100, Math.max(0, actual))}%` }}
              />
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
              {reb > 0
                ? `Buy ${formatIDR(c.rebalance_idr)} to reach target`
                : reb < 0
                  ? `Trim ${formatIDR(Math.abs(reb))} to reach target`
                  : "On target"}
              {` · target marker at ${target.toFixed(0)}%`}
            </div>
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: Rewrite `src/components/HistoryChart.tsx`**

Keep the empty-state string and recharts structure; theme the line + axes.
```tsx
import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from "recharts";
import { formatIDR } from "../lib/format";
import type { Snapshot } from "../api/schemas";

export function HistoryChart({ snapshots }: { snapshots: Snapshot[] }) {
  const data = snapshots.map((s) => ({ date: s.as_of, idr: Number(s.total_idr) }));
  if (data.length === 0)
    return <div className="text-sm text-muted-foreground">No history yet — snapshots accumulate daily.</div>;
  return (
    <div className="h-64 w-full rounded-lg border bg-card p-2">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 10, right: 20, bottom: 0, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
          <XAxis dataKey="date" fontSize={11} stroke="hsl(var(--muted-foreground))" />
          <YAxis tickFormatter={(v) => formatIDR(v)} width={90} fontSize={11} stroke="hsl(var(--muted-foreground))" />
          <Tooltip
            formatter={(v: number) => formatIDR(v)}
            contentStyle={{
              background: "hsl(var(--popover))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "var(--radius)",
              color: "hsl(var(--popover-foreground))",
            }}
          />
          <Line type="monotone" dataKey="idr" stroke="hsl(var(--chart-1))" dot={false} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
```

- [ ] **Step 5: Verify chart tests + suite**

Run: `npm test -- src/components/DriftBars.test.tsx src/components/HistoryChart.test.tsx`
Expected: PASS.
Run: `npm test`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/QueryState.tsx frontend/src/components/AllocationDonut.tsx frontend/src/components/DriftBars.tsx frontend/src/components/HistoryChart.tsx
git commit -m "feat(frontend): theme charts + QueryState with shadcn tokens"
```

---

## Task 7: Dashboard page

**Files:**
- Modify: `src/pages/DashboardPage.tsx`
- Test: `src/pages/DashboardPage.test.tsx` (existing — keep green)

- [ ] **Step 1: Rewrite `src/pages/DashboardPage.tsx`**

Keep the "Refresh prices"/"Refreshing…" button text and section headings; replace the raw button with shadcn `Button` + spinner, and wrap chart sections in `Card`.
```tsx
import { Loader2, RefreshCw } from "lucide-react";
import { useSummary, useHistory, useRefreshPrices } from "../api/hooks";
import { NetWorthCard } from "../components/NetWorthCard";
import { PerformanceCards } from "../components/PerformanceCards";
import { AllocationDonut } from "../components/AllocationDonut";
import { DriftBars } from "../components/DriftBars";
import { HistoryChart } from "../components/HistoryChart";
import { QueryState } from "../components/QueryState";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export default function DashboardPage() {
  const summary = useSummary();
  const history = useHistory();
  const refresh = useRefreshPrices();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Dashboard</h1>
        <Button type="button" onClick={() => refresh.mutate()} disabled={refresh.isPending} size="sm">
          {refresh.isPending ? (
            <>
              <Loader2 className="animate-spin" /> Refreshing…
            </>
          ) : (
            <>
              <RefreshCw /> Refresh prices
            </>
          )}
        </Button>
      </div>

      <QueryState isLoading={summary.isLoading} error={summary.error}>
        {summary.data && (
          <>
            <NetWorthCard s={summary.data} />
            <PerformanceCards s={summary.data} />
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Allocation</CardTitle>
                </CardHeader>
                <CardContent>
                  <AllocationDonut allocation={summary.data.allocation} />
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Target vs Actual</CardTitle>
                </CardHeader>
                <CardContent>
                  <DriftBars allocation={summary.data.allocation} />
                </CardContent>
              </Card>
            </div>
          </>
        )}
      </QueryState>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Value History</CardTitle>
        </CardHeader>
        <CardContent>
          <QueryState isLoading={history.isLoading} error={history.error}>
            <HistoryChart snapshots={history.data ?? []} />
          </QueryState>
        </CardContent>
      </Card>
    </div>
  );
}
```

- [ ] **Step 2: Verify Dashboard test + suite**

Run: `npm test -- src/pages/DashboardPage.test.tsx`
Expected: PASS.
Run: `npm test`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/DashboardPage.tsx
git commit -m "feat(frontend): dashboard on shadcn Button + Card"
```

---

## Task 8: Holdings page

**Files:**
- Modify: `src/pages/HoldingsPage.tsx`
- Test: `src/pages/HoldingsPage.test.tsx` (existing — keep green; preserve column headers + "No positions yet…" string)

- [ ] **Step 1: Rewrite `src/pages/HoldingsPage.tsx`**

```tsx
import { useSummary, useInstruments } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, formatUSD, formatPct } from "../lib/format";
import { Card } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { cn } from "@/lib/utils";

export default function HoldingsPage() {
  const summary = useSummary();
  const instruments = useInstruments();
  const nameOf = (id: number) => instruments.data?.find((i) => i.id === id)?.symbol ?? `#${id}`;
  const positions = summary.data?.positions ?? [];

  return (
    <div className="space-y-4">
      <h1 className="text-xl font-semibold">Holdings</h1>
      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <Card className="overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Instrument</TableHead>
                <TableHead>Qty</TableHead>
                <TableHead>Avg cost</TableHead>
                <TableHead>Price</TableHead>
                <TableHead>Value (IDR)</TableHead>
                <TableHead>Unrealized</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {positions.map((p) => (
                <TableRow key={p.instrument_id}>
                  <TableCell className="font-medium">
                    {nameOf(p.instrument_id)}
                    {p.price_stale && (
                      <Badge variant="outline" className="ml-2 border-amber-500 text-amber-600 dark:text-amber-400" title="Price may be outdated">
                        ⚠ stale
                      </Badge>
                    )}
                  </TableCell>
                  <TableCell>{p.quantity}</TableCell>
                  <TableCell>{formatUSD(p.avg_cost)}</TableCell>
                  <TableCell>{formatUSD(p.latest_price)}</TableCell>
                  <TableCell>{formatIDR(p.market_value_idr)}</TableCell>
                  <TableCell className={cn(Number(p.unrealized_pnl) >= 0 ? "text-emerald-600 dark:text-emerald-400" : "text-red-600 dark:text-red-400")}>
                    {formatUSD(p.unrealized_pnl)} ({formatPct(((Number(p.unrealized_pnl) / (Number(p.cost_basis_total) || 1)) * 100).toString())})
                  </TableCell>
                </TableRow>
              ))}
              {positions.length === 0 && (
                <TableRow>
                  <TableCell className="text-muted-foreground" colSpan={6}>
                    No positions yet. Add transactions to see holdings.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </Card>
      </QueryState>
    </div>
  );
}
```

- [ ] **Step 2: Verify Holdings test + suite**

Run: `npm test -- src/pages/HoldingsPage.test.tsx`
Expected: PASS.
Run: `npm test`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/HoldingsPage.tsx
git commit -m "feat(frontend): holdings table on shadcn Table + Badge"
```

---

## Task 9: Transactions page (form + table + toast)

**Files:**
- Modify: `src/pages/TransactionsPage.tsx`
- Test: `src/pages/TransactionsPage.test.tsx` (existing — keep green; preserve "Add transaction", "No transactions yet." strings and all `aria-label`s)

- [ ] **Step 1: Rewrite `src/pages/TransactionsPage.tsx`**

Uses shadcn `Input`, `Label`, `Select`, `Button`, `Card`, `Table`, and a `sonner` toast on mutation. Every field keeps its existing `aria-label`. Native option lists become shadcn `Select` items.
```tsx
import { useState } from "react";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useAccounts, useInstruments, useTransactions, useCreateTransaction, useDeleteTransaction } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";

const TXN_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];

export default function TransactionsPage() {
  const txns = useTransactions();
  const accounts = useAccounts();
  const instruments = useInstruments();
  const create = useCreateTransaction();
  const del = useDeleteTransaction();

  const [form, setForm] = useState({
    account_id: "",
    instrument_id: "",
    txn_type: "buy",
    executed_at: new Date().toISOString().slice(0, 16),
    quantity: "",
    price_native: "",
    fee_native: "0",
    currency: "USD",
    fx_to_idr: "16000",
    fx_to_usd: "1",
  });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
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
      },
      {
        onSuccess: () => toast.success("Transaction added"),
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  const setField = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value });
  const txns_data = txns.data ?? [];

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Transactions</h1>

      <Card>
        <CardContent className="pt-6">
          <form onSubmit={submit} className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <div className="space-y-1">
              <Label>Account</Label>
              <Select value={form.account_id} onValueChange={(v) => setForm({ ...form, account_id: v })}>
                <SelectTrigger aria-label="Account">
                  <SelectValue placeholder="Account…" />
                </SelectTrigger>
                <SelectContent>
                  {(accounts.data ?? []).map((a) => (
                    <SelectItem key={a.id} value={String(a.id)}>
                      {a.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Instrument</Label>
              <Select value={form.instrument_id} onValueChange={(v) => setForm({ ...form, instrument_id: v })}>
                <SelectTrigger aria-label="Instrument">
                  <SelectValue placeholder="Instrument…" />
                </SelectTrigger>
                <SelectContent>
                  {(instruments.data ?? []).map((i) => (
                    <SelectItem key={i.id} value={String(i.id)}>
                      {i.symbol}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Type</Label>
              <Select value={form.txn_type} onValueChange={(v) => setForm({ ...form, txn_type: v })}>
                <SelectTrigger aria-label="Transaction type">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {TXN_TYPES.map((t) => (
                    <SelectItem key={t} value={t}>
                      {t}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Executed at</Label>
              <Input aria-label="Executed at" type="datetime-local" value={form.executed_at} onChange={setField("executed_at")} />
            </div>
            <div className="space-y-1">
              <Label>Quantity</Label>
              <Input aria-label="Quantity" placeholder="Quantity" value={form.quantity} onChange={setField("quantity")} required />
            </div>
            <div className="space-y-1">
              <Label>Price (native)</Label>
              <Input aria-label="Price (native)" placeholder="Price (native)" value={form.price_native} onChange={setField("price_native")} required />
            </div>
            <div className="space-y-1">
              <Label>Fee</Label>
              <Input aria-label="Fee" placeholder="Fee" value={form.fee_native} onChange={setField("fee_native")} />
            </div>
            <div className="space-y-1">
              <Label>Currency</Label>
              <Input aria-label="Currency" placeholder="Currency" value={form.currency} onChange={setField("currency")} />
            </div>
            <div className="space-y-1">
              <Label>FX → IDR</Label>
              <Input aria-label="FX to IDR" placeholder="FX→IDR" value={form.fx_to_idr} onChange={setField("fx_to_idr")} />
            </div>
            <div className="space-y-1">
              <Label>FX → USD</Label>
              <Input aria-label="FX to USD" placeholder="FX→USD" value={form.fx_to_usd} onChange={setField("fx_to_usd")} />
            </div>
            <Button type="submit" className="col-span-2 sm:col-span-4" disabled={create.isPending}>
              {create.isPending ? "Adding…" : "Add transaction"}
            </Button>
            {create.error && (
              <div className="col-span-2 text-sm text-destructive sm:col-span-4">{(create.error as Error).message}</div>
            )}
          </form>
        </CardContent>
      </Card>

      <QueryState isLoading={txns.isLoading} error={txns.error}>
        <Card className="overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Date</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Instr</TableHead>
                <TableHead>Qty</TableHead>
                <TableHead>Price</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {txns_data.map((t) => (
                <TableRow key={t.id}>
                  <TableCell>{t.executed_at.slice(0, 10)}</TableCell>
                  <TableCell>{t.txn_type}</TableCell>
                  <TableCell>#{t.instrument_id}</TableCell>
                  <TableCell>{t.quantity}</TableCell>
                  <TableCell>
                    {t.price_native} {t.currency}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      aria-label="delete"
                      onClick={() => del.mutate(t.id)}
                      className="text-destructive hover:text-destructive"
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {txns_data.length === 0 && (
                <TableRow>
                  <TableCell colSpan={6} className="text-muted-foreground">
                    No transactions yet.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </Card>
      </QueryState>
    </div>
  );
}
```

- [ ] **Step 2: Verify Transactions test + suite**

Run: `npm test -- src/pages/TransactionsPage.test.tsx`
Expected: PASS ("Add transaction" and "No transactions yet." still present).
Run: `npm test`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/TransactionsPage.tsx
git commit -m "feat(frontend): transactions form + table on shadcn, toast feedback"
```

---

## Task 10: Planner page

**Files:**
- Modify: `src/pages/PlannerPage.tsx`
- Test: `src/pages/PlannerPage.test.tsx` (existing — keep green; preserve `aria-label`s + "Add category" + total-target text)

- [ ] **Step 1: Rewrite `src/pages/PlannerPage.tsx`**

```tsx
import { useState } from "react";
import { Trash2 } from "lucide-react";
import { useCategories, useCreateCategory, useDeleteCategory, useSummary } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatPct } from "../lib/format";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

export default function PlannerPage() {
  const cats = useCategories();
  const summary = useSummary();
  const create = useCreateCategory();
  const del = useDeleteCategory();
  const [form, setForm] = useState({ name: "", target_pct: "", tolerance_band_pct: "" });

  const totalTarget = (cats.data ?? []).reduce((acc, c) => acc + Number(c.target_pct), 0);
  const offTarget = Math.abs(totalTarget - 100) > 0.01;

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

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Allocation Planner</h1>

      <Card>
        <CardContent className="pt-6">
          <form onSubmit={submit} className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <div className="space-y-1">
              <Label>Category name</Label>
              <Input aria-label="Category name" placeholder="Category name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} required />
            </div>
            <div className="space-y-1">
              <Label>Target %</Label>
              <Input aria-label="Target percent" placeholder="Target %" value={form.target_pct} onChange={(e) => setForm({ ...form, target_pct: e.target.value })} required />
            </div>
            <div className="space-y-1">
              <Label>Tolerance band %</Label>
              <Input aria-label="Tolerance band percent" placeholder="Tolerance band % (optional)" value={form.tolerance_band_pct} onChange={(e) => setForm({ ...form, tolerance_band_pct: e.target.value })} />
            </div>
            <div className="flex items-end">
              <Button type="submit" className="w-full" disabled={create.isPending}>
                Add category
              </Button>
            </div>
            {create.error && (
              <div className="col-span-2 text-sm text-destructive sm:col-span-4">{(create.error as Error).message}</div>
            )}
          </form>
        </CardContent>
      </Card>

      <div className={cn("text-sm", offTarget ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground")}>
        Total target: {totalTarget.toFixed(1)}% {offTarget ? "(should sum to 100%)" : "✓"}
      </div>

      <QueryState isLoading={cats.isLoading} error={cats.error}>
        <div className="space-y-2">
          {(cats.data ?? []).map((c) => {
            const a = summary.data?.allocation.find((x) => x.category_id === c.id);
            return (
              <Card key={c.id}>
                <CardContent className="flex items-center justify-between py-3 text-sm">
                  <div>
                    <span className="font-medium">{c.name}</span>
                    <span className="ml-2 text-muted-foreground">
                      target {formatPct(c.target_pct)}
                      {c.tolerance_band_pct ? ` ±${c.tolerance_band_pct}%` : ""}
                    </span>
                    {a && (
                      <span className={cn("ml-2", a.out_of_band ? "text-destructive" : "text-muted-foreground")}>
                        actual {formatPct(a.actual_pct)}
                      </span>
                    )}
                  </div>
                  <Button type="button" variant="ghost" size="icon" aria-label="delete" onClick={() => del.mutate(c.id)} className="text-destructive hover:text-destructive">
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </CardContent>
              </Card>
            );
          })}
          {(cats.data ?? []).length === 0 && <div className="text-muted-foreground">No categories yet.</div>}
        </div>
      </QueryState>
    </div>
  );
}
```

- [ ] **Step 2: Verify Planner test + suite**

Run: `npm test -- src/pages/PlannerPage.test.tsx`
Expected: PASS.
Run: `npm test`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/PlannerPage.tsx
git commit -m "feat(frontend): planner on shadcn Card + Input"
```

---

## Task 11: Settings page

**Files:**
- Modify: `src/pages/SettingsPage.tsx`
- Test: `src/pages/SettingsPage.test.tsx` (existing — keep green; preserve "Accounts", "Instruments", "USD → IDR FX rate" headings + all `aria-label`s)

- [ ] **Step 1: Rewrite `src/pages/SettingsPage.tsx`**

Each section becomes a `Card` with `CardHeader`/`CardTitle`. Account-type and instrument-type pickers use shadcn `Select`; free-text fields use `Input`. Toast on success/error. Headings kept byte-identical.
```tsx
import { useState } from "react";
import { toast } from "sonner";
import {
  useAccounts,
  useCreateAccount,
  useDeleteAccount,
  useInstruments,
  useCreateInstrument,
  useDeleteInstrument,
  useManualPrice,
  useManualFx,
} from "../api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

const today = () => new Date().toISOString().slice(0, 10);
const ACCOUNT_TYPES = ["manual", "exchange", "broker", "bank", "wallet"];
const INSTRUMENT_TYPES = ["crypto", "stock_id", "stock_us", "etf", "mutual_fund", "cash", "bond", "gold", "other"];

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
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle>Accounts</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              createAccount.mutate(
                { ...acc, institution: null, note: null },
                { onSuccess: () => toast.success("Account added"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="Account name" className="w-40" placeholder="Name" value={acc.name} onChange={(e) => setAcc({ ...acc, name: e.target.value })} required />
            <Select value={acc.account_type} onValueChange={(v) => setAcc({ ...acc, account_type: v })}>
              <SelectTrigger aria-label="Account type" className="w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ACCOUNT_TYPES.map((t) => (
                  <SelectItem key={t} value={t}>
                    {t}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input aria-label="Account currency" className="w-28" placeholder="Currency" value={acc.native_currency} onChange={(e) => setAcc({ ...acc, native_currency: e.target.value })} />
            <Button type="submit">Add</Button>
          </form>
          <ul className="text-sm">
            {(accounts.data ?? []).map((a) => (
              <li key={a.id} className="flex justify-between border-b py-1.5">
                <span>
                  {a.name} · {a.account_type} · {a.native_currency}
                </span>
                <button type="button" onClick={() => delAccount.mutate(a.id)} className="text-xs text-destructive hover:underline">
                  delete
                </button>
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Instruments</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              createInstrument.mutate(
                {
                  symbol: ins.symbol,
                  name: ins.name,
                  instrument_type: ins.instrument_type,
                  native_currency: ins.native_currency,
                  category_id: ins.category_id ? Number(ins.category_id) : null,
                  price_source: ins.price_source,
                  decimals: 8,
                  note: null,
                },
                { onSuccess: () => toast.success("Instrument added"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="Instrument symbol" className="w-32" placeholder="Symbol" value={ins.symbol} onChange={(e) => setIns({ ...ins, symbol: e.target.value })} required />
            <Input aria-label="Instrument name" className="w-40" placeholder="Name" value={ins.name} onChange={(e) => setIns({ ...ins, name: e.target.value })} required />
            <Select value={ins.instrument_type} onValueChange={(v) => setIns({ ...ins, instrument_type: v })}>
              <SelectTrigger aria-label="Instrument type" className="w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {INSTRUMENT_TYPES.map((t) => (
                  <SelectItem key={t} value={t}>
                    {t}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input aria-label="Instrument currency" className="w-28" placeholder="Currency" value={ins.native_currency} onChange={(e) => setIns({ ...ins, native_currency: e.target.value })} />
            <Input aria-label="Category id" className="w-40" placeholder="category_id (optional)" value={ins.category_id} onChange={(e) => setIns({ ...ins, category_id: e.target.value })} />
            <Input aria-label="Price source" className="w-72" placeholder="price_source (e.g. coingecko:bitcoin, yahoo:BBCA.JK, manual)" value={ins.price_source} onChange={(e) => setIns({ ...ins, price_source: e.target.value })} />
            <Button type="submit">Add</Button>
          </form>
          <ul className="text-sm">
            {(instruments.data ?? []).map((i) => (
              <li key={i.id} className="flex justify-between border-b py-1.5">
                <span>
                  {i.symbol} · {i.instrument_type} · {i.price_source}
                </span>
                <button type="button" onClick={() => delInstrument.mutate(i.id)} className="text-xs text-destructive hover:underline">
                  delete
                </button>
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Manual price (for reksadana NAV / manual instruments)</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              manualPrice.mutate(
                { instrument_id: Number(price.instrument_id), price: price.price, currency: price.currency, as_of: today() },
                { onSuccess: () => toast.success("Price set"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="Price instrument id" className="w-36" placeholder="instrument_id" value={price.instrument_id} onChange={(e) => setPrice({ ...price, instrument_id: e.target.value })} required />
            <Input aria-label="Price" className="w-32" placeholder="price" value={price.price} onChange={(e) => setPrice({ ...price, price: e.target.value })} required />
            <Input aria-label="Price currency" className="w-28" placeholder="currency" value={price.currency} onChange={(e) => setPrice({ ...price, currency: e.target.value })} />
            <Button type="submit">Set price</Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>USD → IDR FX rate</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              manualFx.mutate(
                { base: "USD", quote: "IDR", rate: fx.rate, as_of: today() },
                { onSuccess: () => toast.success("FX set"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="USD to IDR rate" className="w-40" placeholder="e.g. 16250" value={fx.rate} onChange={(e) => setFx({ rate: e.target.value })} required />
            <Button type="submit">Set FX</Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
```

- [ ] **Step 2: Verify Settings test + suite**

Run: `npm test -- src/pages/SettingsPage.test.tsx`
Expected: PASS (the three headings still render).
Run: `npm test`
Expected: all PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/SettingsPage.tsx
git commit -m "feat(frontend): settings sections on shadcn Card + Select"
```

---

## Task 12: Final verification + docs

**Files:**
- Modify: `frontend/README.md`

- [ ] **Step 1: Full test run**

Run: `npm test`
Expected: ALL suites PASS.

- [ ] **Step 2: Production build**

Run: `npm run build`
Expected: type-check + Vite build succeed. `ls vite.config.*` shows only `vite.config.ts` (no stale emit).

- [ ] **Step 3: Manual smoke (optional but recommended)**

Run backend then frontend: `make dev` from repo root. In the browser at `http://localhost:5173`:
- Sidebar shows Dashboard / Holdings / Transactions / Planner / Settings with icons.
- Mode toggle switches light/dark and persists on reload.
- Each page renders; adding a transaction/category shows a toast.

- [ ] **Step 4: Update `frontend/README.md`**

Add a line under the title noting the UI stack. Replace the first paragraph:
```markdown
Vite + React + TypeScript dashboard for the Phase 1A backend, styled with shadcn/ui (sidebar layout, light/dark theme).
```

- [ ] **Step 5: Commit**

```bash
git add frontend/README.md
git commit -m "docs(frontend): note shadcn/ui in README"
```

---

## Self-Review Notes (author)

- **Spec coverage:** Foundation (Task 1), primitives (Task 2), theme+toggle (Task 3), sidebar layout (Task 4), shared components (Tasks 5–6), all five pages (Tasks 7–11), verification+docs (Task 12). Re-applied `:8081` proxy (Task 1, Step 3). All spec sections mapped.
- **Test safety:** every page/component task preserves the exact strings and `aria-label`s asserted by existing tests; `matchMedia` mock added (Task 2) for the sidebar; `useTheme` returns default context (never throws) so `App.test` renders without a provider.
- **Type consistency:** `cn` from `@/lib/utils`, `useTheme` from `@/components/theme-provider`, `Toaster` from `@/components/ui/sonner`, `toast` from `sonner` — used consistently across tasks.
- **Risk — shadcn CLI network:** Task 2 uses `npx shadcn@latest add … --yes`. If the environment blocks the registry, components must be authored manually from ui.shadcn.com sources (same files); the rest of the plan is unaffected.
