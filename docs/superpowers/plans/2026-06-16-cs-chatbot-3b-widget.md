# CS Chatbot — Plan 3b: Embeddable Widget Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A single `<script>`-embeddable customer-service chat widget that any of the owner's websites can drop in. It renders a floating bubble + panel in a Shadow DOM (isolated from host-site CSS), captures name+contact before chat, and talks to the Plan 3a `/api/public/cs/*` endpoints.

**Architecture:** A standalone Vite build (`vite.config.widget.ts`) bundles `src/cs-widget/index.ts` into a single self-initializing `cs-widget.js` (IIFE, no hashed filename), served as a static asset by the existing Caddy `file_server`. The widget reads its config (`data-key`, optional `data-api-base`) from its own `<script>` tag, mounts a Shadow DOM root, and uses a small typed API client. Pure logic (config parsing, API client, form validation, message-state reducer) is split into testable modules; the DOM mount is thin glue. Zero impact on the main SPA bundle or its PWA service worker.

**Tech Stack:** TypeScript, Vite (library/IIFE build), Shadow DOM, fetch. No framework (vanilla) to keep the bundle tiny and host-site-safe.

**Depends on:** Plan 3a endpoints:
- `POST {apiBase}/public/cs/session` `{site_key, name, email?, phone?}` → `{session_token}`
- `POST {apiBase}/public/cs/message` `{site_key, session_token, message}` → `{reply}`
- `POST {apiBase}/public/cs/history` `{site_key, session_token}` → `[{role, content, created_at}]`

> **Work in the worktree:** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`. Frontend dir: `frontend/`. Tests: `npx vitest run`.

---

## File Structure

- Create: `frontend/src/cs-widget/config.ts` — parse the embedding script tag's data-attributes.
- Create: `frontend/src/cs-widget/api.ts` — typed client for the three endpoints.
- Create: `frontend/src/cs-widget/validate.ts` — pre-chat form validation.
- Create: `frontend/src/cs-widget/ui.ts` — Shadow-DOM render + event wiring (thin).
- Create: `frontend/src/cs-widget/index.ts` — entry: find script tag, read config, mount.
- Create: `frontend/vite.config.widget.ts` — IIFE build → `dist/cs-widget.js`.
- Create: `frontend/src/cs-widget/*.test.ts` — vitest unit tests for config/api/validate.
- Modify: `frontend/package.json` — add `build:widget` script; make `build` also build the widget.
- Modify: `frontend/Dockerfile` — ensure the widget is built into `/srv`.
- Create: `docs/cs-widget-embed.md` — copy-paste embed snippet + setup notes.

---

## Task 1: Config parsing (`config.ts`)

**Files:**
- Create: `frontend/src/cs-widget/config.ts`
- Create: `frontend/src/cs-widget/config.test.ts`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/cs-widget/config.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { readConfig } from "./config";

function scriptEl(attrs: Record<string, string>): HTMLScriptElement {
  const s = document.createElement("script");
  for (const [k, v] of Object.entries(attrs)) s.setAttribute(k, v);
  return s;
}

describe("readConfig", () => {
  it("reads key and defaults apiBase to /api", () => {
    const cfg = readConfig(scriptEl({ "data-key": "abc" }));
    expect(cfg).toEqual({ siteKey: "abc", apiBase: "/api", title: "Customer Service" });
  });

  it("honors explicit api base and title (trims trailing slash)", () => {
    const cfg = readConfig(scriptEl({
      "data-key": "abc",
      "data-api-base": "https://x.com/api/",
      "data-title": "Bantuan",
    }));
    expect(cfg.apiBase).toBe("https://x.com/api");
    expect(cfg.title).toBe("Bantuan");
  });

  it("throws when data-key is missing", () => {
    expect(() => readConfig(scriptEl({}))).toThrow(/data-key/);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd frontend && npx vitest run src/cs-widget/config.test.ts`
Expected: FAIL — `./config` not found.

- [ ] **Step 3: Implement**

Create `frontend/src/cs-widget/config.ts`:

```ts
export interface WidgetConfig {
  siteKey: string;
  apiBase: string;
  title: string;
}

/** Read widget config from the embedding <script> tag's data-attributes. */
export function readConfig(script: HTMLScriptElement): WidgetConfig {
  const siteKey = script.getAttribute("data-key");
  if (!siteKey) {
    throw new Error("cs-widget: missing required data-key attribute on the script tag");
  }
  const apiBase = (script.getAttribute("data-api-base") ?? "/api").replace(/\/+$/, "");
  const title = script.getAttribute("data-title") ?? "Customer Service";
  return { siteKey, apiBase, title };
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd frontend && npx vitest run src/cs-widget/config.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/cs-widget/config.ts frontend/src/cs-widget/config.test.ts
git commit -m "feat(cs-widget): config parsing from script tag"
```

---

## Task 2: API client (`api.ts`)

**Files:**
- Create: `frontend/src/cs-widget/api.ts`
- Create: `frontend/src/cs-widget/api.test.ts`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/cs-widget/api.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest";
import { CsApi } from "./api";

beforeEach(() => { vi.restoreAllMocks(); });

function mockFetch(status: number, body: unknown) {
  return vi.fn().mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
  } as Response);
}

describe("CsApi", () => {
  it("startSession posts site_key + lead fields and returns token", async () => {
    const f = mockFetch(200, { session_token: "tok-1" });
    vi.stubGlobal("fetch", f);
    const api = new CsApi({ siteKey: "k", apiBase: "/api", title: "x" });
    const tok = await api.startSession({ name: "Budi", email: "b@x.com", phone: "" });
    expect(tok).toBe("tok-1");
    const [url, init] = f.mock.calls[0];
    expect(url).toBe("/api/public/cs/session");
    expect(JSON.parse((init as RequestInit).body as string)).toMatchObject({
      site_key: "k", name: "Budi", email: "b@x.com",
    });
  });

  it("sendMessage returns reply", async () => {
    vi.stubGlobal("fetch", mockFetch(200, { reply: "Halo!" }));
    const api = new CsApi({ siteKey: "k", apiBase: "/api", title: "x" });
    expect(await api.sendMessage("tok-1", "halo")).toBe("Halo!");
  });

  it("throws the server error message on non-2xx", async () => {
    vi.stubGlobal("fetch", mockFetch(429, { error: "too many messages, slow down" }));
    const api = new CsApi({ siteKey: "k", apiBase: "/api", title: "x" });
    await expect(api.sendMessage("tok", "hi")).rejects.toThrow(/slow down/);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd frontend && npx vitest run src/cs-widget/api.test.ts`
Expected: FAIL — `./api` not found.

- [ ] **Step 3: Implement**

Create `frontend/src/cs-widget/api.ts`:

```ts
import type { WidgetConfig } from "./config";

export interface Lead { name: string; email: string; phone: string; }
export interface HistoryMessage { role: string; content: string; created_at: string; }

export class CsApi {
  constructor(private cfg: WidgetConfig) {}

  private async post<T>(path: string, body: unknown): Promise<T> {
    const res = await fetch(`${this.cfg.apiBase}${path}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      let msg = `HTTP ${res.status}`;
      try { const b = await res.json(); if (b?.error) msg = b.error; } catch { /* keep default */ }
      throw new Error(msg);
    }
    return (await res.json()) as T;
  }

  async startSession(lead: Lead): Promise<string> {
    const r = await this.post<{ session_token: string }>("/public/cs/session", {
      site_key: this.cfg.siteKey,
      name: lead.name,
      email: lead.email || undefined,
      phone: lead.phone || undefined,
    });
    return r.session_token;
  }

  async sendMessage(token: string, message: string): Promise<string> {
    const r = await this.post<{ reply: string }>("/public/cs/message", {
      site_key: this.cfg.siteKey,
      session_token: token,
      message,
    });
    return r.reply;
  }

  async history(token: string): Promise<HistoryMessage[]> {
    return this.post<HistoryMessage[]>("/public/cs/history", {
      site_key: this.cfg.siteKey,
      session_token: token,
    });
  }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd frontend && npx vitest run src/cs-widget/api.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/cs-widget/api.ts frontend/src/cs-widget/api.test.ts
git commit -m "feat(cs-widget): typed API client for public CS endpoints"
```

---

## Task 3: Form validation (`validate.ts`)

**Files:**
- Create: `frontend/src/cs-widget/validate.ts`
- Create: `frontend/src/cs-widget/validate.test.ts`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/cs-widget/validate.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { validateLead } from "./validate";

describe("validateLead", () => {
  it("requires a name", () => {
    expect(validateLead({ name: "", email: "a@x.com", phone: "" })).toMatch(/nama/i);
  });
  it("requires email or phone", () => {
    expect(validateLead({ name: "Budi", email: "", phone: "" })).toMatch(/email|nomor/i);
  });
  it("rejects a malformed email when phone is absent", () => {
    expect(validateLead({ name: "Budi", email: "not-an-email", phone: "" })).toMatch(/email/i);
  });
  it("accepts name + valid email", () => {
    expect(validateLead({ name: "Budi", email: "a@x.com", phone: "" })).toBeNull();
  });
  it("accepts name + phone (no email)", () => {
    expect(validateLead({ name: "Budi", email: "", phone: "0812345678" })).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd frontend && npx vitest run src/cs-widget/validate.test.ts`
Expected: FAIL — `./validate` not found.

- [ ] **Step 3: Implement**

Create `frontend/src/cs-widget/validate.ts`:

```ts
import type { Lead } from "./api";

const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

/** Returns an error message (Bahasa) or null when the lead is valid. */
export function validateLead(lead: Lead): string | null {
  if (!lead.name.trim()) return "Mohon isi nama kamu.";
  const hasEmail = lead.email.trim().length > 0;
  const hasPhone = lead.phone.trim().length > 0;
  if (!hasEmail && !hasPhone) return "Mohon isi email atau nomor HP supaya kami bisa menghubungi kamu.";
  if (hasEmail && !EMAIL_RE.test(lead.email.trim())) return "Format email belum benar.";
  return null;
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd frontend && npx vitest run src/cs-widget/validate.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/cs-widget/validate.ts frontend/src/cs-widget/validate.test.ts
git commit -m "feat(cs-widget): pre-chat lead validation"
```

---

## Task 4: Shadow-DOM UI (`ui.ts`)

**Files:**
- Create: `frontend/src/cs-widget/ui.ts`

> **Context:** `ui.ts` is the thin DOM layer. It is not unit-tested (DOM-heavy glue); correctness comes from the tested `config`/`api`/`validate` modules it composes. Keep it focused: build a shadow root, render the bubble + panel + pre-chat form + message list, and wire events to the `CsApi`.

- [ ] **Step 1: Implement**

Create `frontend/src/cs-widget/ui.ts`:

```ts
import { CsApi, type Lead } from "./api";
import type { WidgetConfig } from "./config";
import { validateLead } from "./validate";

const STYLE = `
:host { all: initial; }
.bubble { position: fixed; right: 20px; bottom: 20px; width: 56px; height: 56px; border-radius: 50%;
  background: #2563eb; color: #fff; font: 24px sans-serif; border: none; cursor: pointer; box-shadow: 0 4px 12px rgba(0,0,0,.25); z-index: 2147483000; }
.panel { position: fixed; right: 20px; bottom: 88px; width: 340px; max-width: calc(100vw - 40px); height: 460px; max-height: calc(100vh - 120px);
  background: #fff; border-radius: 12px; box-shadow: 0 8px 30px rgba(0,0,0,.25); display: none; flex-direction: column; overflow: hidden; z-index: 2147483000; font: 14px sans-serif; color: #111; }
.panel.open { display: flex; }
.header { background: #2563eb; color: #fff; padding: 12px 14px; font-weight: 600; }
.body { flex: 1; overflow-y: auto; padding: 12px; display: flex; flex-direction: column; gap: 8px; }
.msg { padding: 8px 10px; border-radius: 10px; max-width: 85%; white-space: pre-wrap; }
.msg.user { align-self: flex-end; background: #2563eb; color: #fff; }
.msg.bot { align-self: flex-start; background: #f1f5f9; color: #111; }
.foot { border-top: 1px solid #e5e7eb; padding: 8px; display: flex; gap: 6px; }
.foot input, .form input { flex: 1; padding: 8px; border: 1px solid #cbd5e1; border-radius: 8px; font: inherit; }
.foot button, .form button { padding: 8px 12px; background: #2563eb; color: #fff; border: none; border-radius: 8px; cursor: pointer; }
.form { padding: 14px; display: flex; flex-direction: column; gap: 8px; }
.err { color: #b91c1c; font-size: 12px; min-height: 14px; }
`;

export function mountWidget(cfg: WidgetConfig) {
  const api = new CsApi(cfg);
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root = host.attachShadow({ mode: "open" });
  root.innerHTML = `
    <style>${STYLE}</style>
    <button class="bubble" aria-label="Chat">💬</button>
    <div class="panel" role="dialog" aria-label="${cfg.title}">
      <div class="header">${cfg.title}</div>
      <div class="body"></div>
      <form class="form">
        <input name="name" placeholder="Nama" autocomplete="name" />
        <input name="email" placeholder="Email" autocomplete="email" />
        <input name="phone" placeholder="No. HP (opsional jika ada email)" autocomplete="tel" />
        <div class="err"></div>
        <button type="submit">Mulai chat</button>
      </form>
      <div class="foot" style="display:none">
        <input class="msg-input" placeholder="Tulis pesan..." />
        <button class="send">Kirim</button>
      </div>
    </div>`;

  const $ = <T extends Element>(sel: string) => root.querySelector(sel) as T;
  const panel = $(".panel") as HTMLDivElement;
  const body = $(".body") as HTMLDivElement;
  const form = $(".form") as HTMLFormElement;
  const foot = $(".foot") as HTMLDivElement;
  const err = $(".err") as HTMLDivElement;
  const input = $(".msg-input") as HTMLInputElement;

  let token: string | null = null;

  const addMsg = (text: string, who: "user" | "bot") => {
    const d = document.createElement("div");
    d.className = `msg ${who}`;
    d.textContent = text;
    body.appendChild(d);
    body.scrollTop = body.scrollHeight;
  };

  $(".bubble").addEventListener("click", () => panel.classList.toggle("open"));

  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const data = new FormData(form);
    const lead: Lead = {
      name: String(data.get("name") ?? ""),
      email: String(data.get("email") ?? ""),
      phone: String(data.get("phone") ?? ""),
    };
    const problem = validateLead(lead);
    if (problem) { err.textContent = problem; return; }
    err.textContent = "";
    try {
      token = await api.startSession(lead);
      form.style.display = "none";
      foot.style.display = "flex";
      addMsg(`Halo ${lead.name}! Ada yang bisa kami bantu?`, "bot");
    } catch (e2) {
      err.textContent = (e2 as Error).message;
    }
  });

  const send = async () => {
    const text = input.value.trim();
    if (!text || !token) return;
    addMsg(text, "user");
    input.value = "";
    try {
      const reply = await api.sendMessage(token, text);
      addMsg(reply, "bot");
    } catch (e2) {
      addMsg(`⚠️ ${(e2 as Error).message}`, "bot");
    }
  };
  $(".send").addEventListener("click", send);
  input.addEventListener("keydown", (e) => { if ((e as KeyboardEvent).key === "Enter") send(); });
}
```

- [ ] **Step 2: Verify typecheck**

Run: `cd frontend && npx tsc -b 2>&1 | tail -10` (or `npx tsc --noEmit`)
Expected: no type errors in `cs-widget`. Fix any.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/cs-widget/ui.ts
git commit -m "feat(cs-widget): shadow-DOM bubble/panel UI"
```

---

## Task 5: Entry point (`index.ts`)

**Files:**
- Create: `frontend/src/cs-widget/index.ts`

- [ ] **Step 1: Implement**

Create `frontend/src/cs-widget/index.ts`:

```ts
import { readConfig } from "./config";
import { mountWidget } from "./ui";

// Find THIS script tag (document.currentScript works during initial execution;
// fall back to the last script with data-key for async/defer edge cases).
function ownScript(): HTMLScriptElement | null {
  if (document.currentScript instanceof HTMLScriptElement) return document.currentScript;
  const all = Array.from(document.querySelectorAll("script[data-key]"));
  return (all[all.length - 1] as HTMLScriptElement) ?? null;
}

try {
  const script = ownScript();
  if (!script) throw new Error("cs-widget: could not locate its own <script> tag");
  const cfg = readConfig(script);
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => mountWidget(cfg));
  } else {
    mountWidget(cfg);
  }
} catch (e) {
  // Never break the host page; log and bail.
  console.error(e);
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/cs-widget/index.ts
git commit -m "feat(cs-widget): self-initializing entry point"
```

---

## Task 6: Vite widget build + package script

**Files:**
- Create: `frontend/vite.config.widget.ts`
- Modify: `frontend/package.json`

- [ ] **Step 1: Create the widget Vite config**

Create `frontend/vite.config.widget.ts`:

```ts
import { defineConfig } from "vite";
import { fileURLToPath, URL } from "node:url";

// Standalone single-file build of the embeddable widget. No hashing, no PWA,
// no code-splitting — one self-initializing cs-widget.js served as a static asset.
export default defineConfig({
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  build: {
    emptyOutDir: false, // do NOT wipe the SPA's dist
    lib: {
      entry: fileURLToPath(new URL("./src/cs-widget/index.ts", import.meta.url)),
      formats: ["iife"],
      name: "CsWidget",
      fileName: () => "cs-widget.js",
    },
    rollupOptions: {
      output: { entryFileNames: "cs-widget.js", inlineDynamicImports: true },
    },
  },
});
```

- [ ] **Step 2: Add the build script**

In `frontend/package.json` `scripts`, add `build:widget` and chain it into `build`. Find the current `"build"` line (the research showed `build` runs `tsc -b && vite build`) and update:

```jsonc
{
  "scripts": {
    // ...
    "build": "tsc -b && vite build && vite build -c vite.config.widget.ts",
    "build:widget": "vite build -c vite.config.widget.ts"
  }
}
```

> **Implementer note:** preserve the EXACT existing `build` prefix (whatever `tsc -b && vite build` actually is in the file) and only append `&& vite build -c vite.config.widget.ts`. Keep the rest of `scripts` untouched.

- [ ] **Step 3: Build and verify the artifact**

Run: `cd frontend && npm run build:widget && ls -la dist/cs-widget.js`
Expected: `dist/cs-widget.js` exists and is a single file (tens of KB). Open it briefly to confirm it's a self-contained IIFE (no `import` statements at top level).

> **If `tsc -b` errors block `npm run build`:** the widget files must typecheck. Ensure `src/cs-widget/**` is covered by the existing `tsconfig`. If the widget needs DOM types not already enabled, confirm `lib` includes `DOM` (it does for a Vite React app). Do not loosen `strict`.

- [ ] **Step 4: Commit**

```bash
git add frontend/vite.config.widget.ts frontend/package.json
git commit -m "feat(cs-widget): standalone IIFE vite build"
```

---

## Task 7: Serve via Docker/Caddy + embed docs

**Files:**
- Modify: `frontend/Dockerfile` (if needed)
- Create: `docs/cs-widget-embed.md`

- [ ] **Step 1: Confirm the Docker build produces the widget**

Read `frontend/Dockerfile`. The build stage runs `npm run build`; since Task 6 chained `build:widget` into `build`, `dist/cs-widget.js` is produced and the existing `COPY --from=builder /app/dist /srv` already ships it. Caddy's `file_server` then serves it at `/cs-widget.js`.

- [ ] **Step 2: Verify no extra Dockerfile change is required**

If `frontend/Dockerfile`'s build command is NOT `npm run build` (e.g. it calls `vite build` directly), add the widget build:

```dockerfile
RUN npm run build && npm run build:widget
```

Only change it if the current command would skip the widget. State what you found.

- [ ] **Step 3: Write the embed doc**

Create `docs/cs-widget-embed.md`:

```markdown
# Embedding the Customer-Service Widget

Add this one line before `</body>` on any page where you want the chat bubble:

```html
<script
  src="https://portfolio.catalystlabs.id/cs-widget.js"
  data-key="YOUR_CS_WIDGET_KEY"
  data-title="Customer Service"
  defer></script>
```

- `data-key` (required): the value of `CS_WIDGET_KEY` set on the backend. It is **not a secret** (it ships in page JS); abuse is controlled server-side by the Origin allowlist (`CS_ALLOWED_ORIGINS`), rate limiting, and per-conversation caps.
- `data-api-base` (optional): override the API base. Defaults to `/api` (same-origin). For a site on a **different** domain than the backend, set the absolute base, e.g. `data-api-base="https://portfolio.catalystlabs.id/api"`, and add that site's origin to `CS_ALLOWED_ORIGINS`.
- `data-title` (optional): panel header text. Default "Customer Service".

## Backend setup
Set both env vars (together) to enable the public endpoints:

```
CS_ALLOWED_ORIGINS=https://your-site.com,https://www.your-site.com
CS_WIDGET_KEY=<any non-secret routing key>
OPENAI_API_KEY=<for KB embeddings>   # already used for ingestion
```

Populate the knowledge base, pricing, and orders via the admin UI (Plan 4) before going live.
```

- [ ] **Step 4: Final verification**

Run: `cd frontend && npx vitest run src/cs-widget && npm run build:widget 2>&1 | tail -5`
Expected: all widget unit tests pass; `dist/cs-widget.js` builds.

- [ ] **Step 5: Commit**

```bash
git add docs/cs-widget-embed.md frontend/Dockerfile
git commit -m "docs(cs-widget): embed instructions + serve via Caddy"
```

---

## Self-Review

**Spec coverage (spec §6 web widget):**
- Separate lightweight bundle, Shadow DOM, not entangled with SPA/PWA ✓ Tasks 4,6 (`emptyOutDir:false`, IIFE lib build, `:host { all: initial }`).
- Pre-chat form (name + email/phone) before chat ✓ Tasks 3,4 (`validateLead` + form gating).
- Calls `/api/public/cs/*` with site-key ✓ Task 2.
- Embed via `<script src=... data-key=... defer>` ✓ Tasks 1,5,7.
- Served at a stable path by Caddy ✓ Task 7.

**Placeholder scan:** No TBD/TODO. The `package.json`/`Dockerfile` notes instruct preserving real existing content, not inventing.

**Type consistency:** `WidgetConfig {siteKey, apiBase, title}` flows config→api→ui consistently. `Lead {name,email,phone}` shared by `api`, `validate`, `ui`. `CsApi.{startSession,sendMessage,history}` return types match handler bodies. API paths (`/public/cs/session|message|history`) and request field names (`site_key`, `session_token`) match Plan 3a's `Deserialize` structs exactly.

---

## Downstream

- **Plan 4 — Admin UI:** KB/pricing/orders managers + CS inbox inside the authenticated SPA (`api/cs_admin.rs` + schemas/hooks/pages). The KB save path triggers `kb::embed_pending` so the widget's `kb_search` has vectors to match.
- **Plan 2.5 — Upwork `get_project_status`.**
- **Phase 2 — WhatsApp CS** (separate number/gateway, per-contact routing).
