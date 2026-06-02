# Connect WhatsApp from the Web Frontend — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user view WhatsApp connection status, scan the pairing QR, and connect/disconnect — all from the web UI instead of the terminal.

**Architecture:** Backend keeps live connection state in an in-memory `Arc<Mutex<WaState>>`. The Baileys gateway pushes status/QR to the backend and polls it for control commands (`restart`/`logout`). The React frontend polls a status endpoint (~2s) and renders status + QR + connect/disconnect buttons.

**Tech Stack:** Rust (axum), Node (Baileys), React + react-query + zod, `qrcode.react` for QR rendering, vitest for FE tests.

**Spec:** `docs/superpowers/specs/2026-06-02-whatsapp-connect-fe-design.md`

---

## File Structure

**Backend (Rust)**
- `backend/src/wa_state.rs` *(new)* — `WaState` type + `WaStatus`/`WaCommand` enums + pure logic (set/take command, apply push, stale-aware view). Owns all testable logic.
- `backend/src/main.rs` *(modify)* — register `mod wa_state;`, add `wa: SharedWaState` to `AppState`.
- `backend/src/api/whatsapp.rs` *(modify)* — add gateway-facing (`push_state`, `poll_commands`) and FE-facing (`status`, `connect`, `disconnect`) handlers + a testable token helper; reuse the helper in `inbound`.
- `backend/src/api/mod.rs` *(modify)* — register 5 new routes.

**Gateway (Node)**
- `whatsapp-gateway/index.js` *(modify)* — push state on connection changes, poll for commands, handle logout/restart.
- `whatsapp-gateway/README.md` *(modify)* — document that pairing now happens from the web UI.

**Frontend (React)**
- `frontend/src/api/schemas.ts` *(modify)* — `WhatsappStatusSchema`.
- `frontend/src/api/hooks.ts` *(modify)* — `useWhatsappStatus`, `useConnectWhatsapp`, `useDisconnectWhatsapp`.
- `frontend/src/pages/WhatsAppPage.tsx` *(new)* — the connection UI.
- `frontend/src/pages/WhatsAppPage.test.tsx` *(new)* — render tests.
- `frontend/src/App.tsx` *(modify)* — add `/whatsapp` route.
- `frontend/src/components/AppShell.tsx` *(modify)* — add nav entry.
- `frontend/src/pages/ChatPage.tsx` *(modify)* — make the sync badge dynamic.
- `frontend/package.json` *(modify)* — add `qrcode.react` dependency.

---

## Task 1: Backend `WaState` type and logic

**Files:**
- Create: `backend/src/wa_state.rs`
- Modify: `backend/src/main.rs` (add `mod wa_state;`)

- [ ] **Step 1: Create `wa_state.rs` with the type and tests**

Create `backend/src/wa_state.rs`:

```rust
//! In-memory state of the WhatsApp gateway connection.
//!
//! The gateway pushes status/QR updates here and polls for control commands.
//! State is intentionally ephemeral — a backend restart simply waits for the
//! gateway's next heartbeat to repopulate it.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A reported "connected" is downgraded to "connecting" if the gateway has not
/// sent a heartbeat within this window, so the UI never shows a false positive.
const STALE_AFTER: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaStatus {
    Disconnected,
    Connecting,
    Qr,
    Connected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WaCommand {
    Restart,
    Logout,
}

/// The frontend-facing snapshot of the connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WaStatusView {
    pub status: WaStatus,
    pub qr: Option<String>,
    pub number: Option<String>,
}

#[derive(Debug)]
pub struct WaState {
    status: WaStatus,
    qr: Option<String>,
    number: Option<String>,
    pending_command: Option<WaCommand>,
    last_seen: Option<Instant>,
}

impl Default for WaState {
    fn default() -> Self {
        Self {
            status: WaStatus::Disconnected,
            qr: None,
            number: None,
            pending_command: None,
            last_seen: None,
        }
    }
}

impl WaState {
    /// Apply a state push from the gateway and refresh the heartbeat clock.
    pub fn apply_push(
        &mut self,
        status: WaStatus,
        qr: Option<String>,
        number: Option<String>,
        now: Instant,
    ) {
        self.status = status;
        self.qr = qr;
        self.number = number;
        self.last_seen = Some(now);
    }

    /// Queue a control command for the gateway to pick up on its next poll.
    pub fn set_command(&mut self, command: WaCommand) {
        self.pending_command = Some(command);
    }

    /// Return the pending command exactly once, clearing it (consume-once).
    pub fn take_command(&mut self) -> Option<WaCommand> {
        self.pending_command.take()
    }

    /// Snapshot for the frontend, downgrading a stale "connected" to "connecting".
    pub fn view(&self, now: Instant) -> WaStatusView {
        let stale = self
            .last_seen
            .map_or(true, |seen| now.duration_since(seen) > STALE_AFTER);
        let status = if stale && self.status == WaStatus::Connected {
            WaStatus::Connecting
        } else {
            self.status
        };
        WaStatusView {
            status,
            qr: self.qr.clone(),
            number: self.number.clone(),
        }
    }
}

pub type SharedWaState = Arc<Mutex<WaState>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_command_returns_once_then_none() {
        let mut state = WaState::default();
        state.set_command(WaCommand::Logout);
        assert_eq!(state.take_command(), Some(WaCommand::Logout));
        assert_eq!(state.take_command(), None);
    }

    #[test]
    fn apply_push_updates_the_view() {
        let mut state = WaState::default();
        let now = Instant::now();
        state.apply_push(WaStatus::Qr, Some("abc".into()), None, now);
        let view = state.view(now);
        assert_eq!(view.status, WaStatus::Qr);
        assert_eq!(view.qr.as_deref(), Some("abc"));
    }

    #[test]
    fn connected_downgrades_when_heartbeat_is_stale() {
        let mut state = WaState::default();
        let earlier = Instant::now();
        state.apply_push(WaStatus::Connected, None, Some("62812".into()), earlier);
        // Fresh heartbeat keeps it connected.
        assert_eq!(state.view(earlier).status, WaStatus::Connected);
        // A stale heartbeat downgrades it.
        let later = earlier + STALE_AFTER + Duration::from_secs(1);
        assert_eq!(state.view(later).status, WaStatus::Connecting);
    }
}
```

- [ ] **Step 2: Register the module**

In `backend/src/main.rs`, add `mod wa_state;` alongside the other `mod` declarations (keep alphabetical order — after `mod service;`):

```rust
mod service;
mod wa_state;
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cd backend && cargo test wa_state`
Expected: PASS — 3 tests (`take_command_returns_once_then_none`, `apply_push_updates_the_view`, `connected_downgrades_when_heartbeat_is_stale`).

- [ ] **Step 4: Commit**

```bash
git add backend/src/wa_state.rs backend/src/main.rs
git commit -m "feat(whatsapp): in-memory WaState with stale-aware status view"
```

---

## Task 2: Wire `WaState` into `AppState`

**Files:**
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Extend `AppState`**

In `backend/src/main.rs`, change the struct and use the shared type:

```rust
use db::Db;
use wa_state::{SharedWaState, WaState};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub wa: SharedWaState,
}
```

- [ ] **Step 2: Initialise it in `main`**

In `main()`, replace the `let state = AppState { db: db.clone() };` line with:

```rust
    let state = AppState {
        db: db.clone(),
        wa: Arc::new(Mutex::new(WaState::default())),
    };
```

- [ ] **Step 3: Verify it compiles**

Run: `cd backend && cargo build`
Expected: builds successfully (no handlers use `wa` yet — that's Task 3).

- [ ] **Step 4: Commit**

```bash
git add backend/src/main.rs
git commit -m "feat(whatsapp): add shared WaState to AppState"
```

---

## Task 3: Backend endpoints

**Files:**
- Modify: `backend/src/api/whatsapp.rs`
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Add a testable token helper and rewrite `whatsapp.rs`**

Replace the entire contents of `backend/src/api/whatsapp.rs` with:

```rust
use crate::error::AppError;
use crate::llm::claude::ClaudeClient;
use crate::wa_state::{WaCommand, WaStatus, WaStatusView};
use crate::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Deserialize)]
pub struct WaIn {
    pub from: String,
    pub message: String,
}
#[derive(Serialize)]
pub struct WaOut {
    pub reply: String,
}

/// Pure token comparison. `None` expected means no token is configured (local
/// dev) and any request is allowed.
fn token_matches(expected: Option<String>, got: Option<&str>) -> bool {
    match expected {
        Some(exp) => got == Some(exp.as_str()),
        None => true,
    }
}

/// Enforce the shared-secret header `x-gateway-token` == env `GATEWAY_TOKEN`.
pub fn check_gateway_token(headers: &HeaderMap) -> Result<(), AppError> {
    let expected = std::env::var("GATEWAY_TOKEN").ok();
    let got = headers
        .get("x-gateway-token")
        .and_then(|v| v.to_str().ok());
    if token_matches(expected, got) {
        Ok(())
    } else {
        Err(AppError::BadRequest("bad gateway token".into()))
    }
}

fn lock_wa(s: &AppState) -> Result<std::sync::MutexGuard<'_, crate::wa_state::WaState>, AppError> {
    s.wa
        .lock()
        .map_err(|_| AppError::Other(anyhow::anyhow!("wa state poisoned")))
}

/// Called by the Baileys gateway for each inbound WhatsApp text. Returns the
/// assistant reply; the gateway sends it back over WhatsApp.
pub async fn inbound(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<WaIn>,
) -> Result<Json<WaOut>, AppError> {
    check_gateway_token(&headers)?;
    if b.message.trim().is_empty() {
        return Err(AppError::BadRequest("empty message".into()));
    }
    let client = ClaudeClient::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("chat unavailable: {e}")))?;
    let reply = crate::service::chat::answer(&s.db, &client, "whatsapp", &b.message)
        .await
        .map_err(AppError::Other)?;
    let _ = &b.from; // reserved for future per-sender routing
    Ok(Json(WaOut { reply }))
}

// ── Gateway-facing endpoints ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StatePush {
    pub status: WaStatus,
    pub qr: Option<String>,
    pub number: Option<String>,
}

#[derive(Serialize)]
pub struct CommandOut {
    pub command: Option<WaCommand>,
}

/// Gateway pushes its current connection state here.
pub async fn push_state(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<StatePush>,
) -> Result<Json<()>, AppError> {
    check_gateway_token(&headers)?;
    lock_wa(&s)?.apply_push(b.status, b.qr, b.number, Instant::now());
    Ok(Json(()))
}

/// Gateway polls here for a pending control command (consume-once).
pub async fn poll_commands(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CommandOut>, AppError> {
    check_gateway_token(&headers)?;
    let command = lock_wa(&s)?.take_command();
    Ok(Json(CommandOut { command }))
}

// ── Frontend-facing endpoints ───────────────────────────────────────────────

/// Current connection status for the web UI.
pub async fn status(State(s): State<AppState>) -> Result<Json<WaStatusView>, AppError> {
    let view = lock_wa(&s)?.view(Instant::now());
    Ok(Json(view))
}

/// Request the gateway to (re)start a session — produces a fresh QR.
pub async fn connect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_wa(&s)?.set_command(WaCommand::Restart);
    Ok(Json(()))
}

/// Request the gateway to log out and clear its session.
pub async fn disconnect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_wa(&s)?.set_command(WaCommand::Logout);
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_allows_when_unset() {
        assert!(token_matches(None, None));
        assert!(token_matches(None, Some("anything")));
    }

    #[test]
    fn token_matches_requires_equality_when_set() {
        assert!(token_matches(Some("secret".into()), Some("secret")));
        assert!(!token_matches(Some("secret".into()), Some("wrong")));
        assert!(!token_matches(Some("secret".into()), None));
    }
}
```

- [ ] **Step 2: Register the routes**

In `backend/src/api/mod.rs`, add these five routes right after the existing `/chat/whatsapp/inbound` line (line 19):

```rust
        .route("/chat/whatsapp/inbound", post(whatsapp::inbound))
        .route("/whatsapp/state", post(whatsapp::push_state))
        .route("/whatsapp/commands", get(whatsapp::poll_commands))
        .route("/whatsapp/status", get(whatsapp::status))
        .route("/whatsapp/connect", post(whatsapp::connect))
        .route("/whatsapp/disconnect", post(whatsapp::disconnect))
```

(`get` and `post` are already imported — `get` is used by `/health`, `post` by `/chat`.)

- [ ] **Step 3: Run tests + build**

Run: `cd backend && cargo test whatsapp && cargo build`
Expected: PASS — 2 token tests; build succeeds.

- [ ] **Step 4: Commit**

```bash
git add backend/src/api/whatsapp.rs backend/src/api/mod.rs
git commit -m "feat(whatsapp): backend endpoints for status, commands, connect/disconnect"
```

---

## Task 4: Gateway pushes state and polls for commands

**Files:**
- Modify: `whatsapp-gateway/index.js`
- Modify: `whatsapp-gateway/README.md`

- [ ] **Step 1: Rewrite `index.js`**

Replace the entire contents of `whatsapp-gateway/index.js` with:

```js
import makeWASocket, { useMultiFileAuthState, DisconnectReason } from "@whiskeysockets/baileys";
import { rm } from "node:fs/promises";

const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";
const GATEWAY_TOKEN = process.env.GATEWAY_TOKEN ?? "";
const AUTH_DIR = "auth_state";
const COMMAND_POLL_MS = 3000;

const authHeaders = { "content-type": "application/json", "x-gateway-token": GATEWAY_TOKEN };

// The currently-active socket; reassigned whenever we (re)start a session.
let currentSock = null;

/** Push the current connection state to the backend so the web UI can show it. */
async function reportState(status, extra = {}) {
  try {
    await fetch(`${BACKEND}/whatsapp/state`, {
      method: "POST",
      headers: authHeaders,
      body: JSON.stringify({ status, ...extra }),
    });
  } catch (e) {
    console.error("reportState failed", e);
  }
}

/** Ask the backend for a pending control command (consumed once). */
async function fetchCommand() {
  try {
    const res = await fetch(`${BACKEND}/whatsapp/commands`, { headers: authHeaders });
    if (!res.ok) return null;
    const { command } = await res.json();
    return command ?? null;
  } catch (e) {
    console.error("fetchCommand failed", e);
    return null;
  }
}

/** Forward one inbound WhatsApp message to the chatbot and reply. */
async function forwardInbound(sock, from, text) {
  try {
    const res = await fetch(`${BACKEND}/chat/whatsapp/inbound`, {
      method: "POST",
      headers: authHeaders,
      body: JSON.stringify({ from, message: text }),
    });
    if (!res.ok) { console.error("backend error", res.status); return; }
    const { reply } = await res.json();
    await sock.sendMessage(from, { text: reply });
  } catch (e) {
    console.error("gateway error", e);
  }
}

/** Start (or restart) a Baileys session and wire up its event handlers. */
async function start() {
  const { state, saveCreds } = await useMultiFileAuthState(AUTH_DIR);
  const sock = makeWASocket({ auth: state, printQRInTerminal: false });
  currentSock = sock;
  sock.ev.on("creds.update", saveCreds);

  sock.ev.on("connection.update", async (u) => {
    const { connection, lastDisconnect, qr } = u;
    if (qr) await reportState("qr", { qr });
    if (connection === "open") {
      const number = sock.user?.id?.split(":")[0] ?? null;
      await reportState("connected", { number });
      console.log("WhatsApp connected.");
    } else if (connection === "close") {
      await reportState("disconnected");
      const loggedOut = lastDisconnect?.error?.output?.statusCode === DisconnectReason.loggedOut;
      if (!loggedOut) start();
    }
  });

  sock.ev.on("messages.upsert", async ({ messages, type }) => {
    if (type !== "notify") return;
    for (const m of messages) {
      if (m.key.fromMe) continue;
      const text = m.message?.conversation ?? m.message?.extendedTextMessage?.text;
      const from = m.key.remoteJid;
      if (!text || !from) continue;
      await forwardInbound(sock, from, text);
    }
  });
}

/** Poll the backend for control commands and act on them. */
function startCommandLoop() {
  setInterval(async () => {
    const command = await fetchCommand();
    if (command === "logout") {
      try { await currentSock?.logout(); } catch (e) { console.error("logout failed", e); }
      await rm(AUTH_DIR, { recursive: true, force: true });
      await reportState("disconnected");
      start();
    } else if (command === "restart") {
      start();
    }
  }, COMMAND_POLL_MS);
}

reportState("connecting");
startCommandLoop();
start();
```

- [ ] **Step 2: Verify the gateway still starts (syntax + boot)**

Start the backend in one terminal (`cd backend && cargo run`), then:
Run: `cd whatsapp-gateway && timeout 8 node index.js; echo "exit: $?"`
Expected: no syntax/import error; logs show it attempts to connect (a QR may print as a `reportState("qr")` POST — check the backend logs receive `POST /whatsapp/state`). `exit: 124` from `timeout` is fine.

- [ ] **Step 3: Update the README**

In `whatsapp-gateway/README.md`, replace step 3 of the "## Run" list:

```markdown
3. Open the web app, go to the **WhatsApp** page, click **Connect**, and scan the QR shown there with WhatsApp (Linked Devices). Creds persist in `auth_state/`. (The QR is also reported to the backend; nothing prints in the terminal anymore.)
```

- [ ] **Step 4: Commit**

```bash
git add whatsapp-gateway/index.js whatsapp-gateway/README.md
git commit -m "feat(whatsapp): gateway reports state and polls backend for commands"
```

---

## Task 5: Frontend schema and hooks

**Files:**
- Modify: `frontend/src/api/schemas.ts`
- Modify: `frontend/src/api/hooks.ts`

- [ ] **Step 1: Add the schema**

Append to `frontend/src/api/schemas.ts`:

```ts
// ── WhatsApp connection ─────────────────────────────────────────────────────

export const WhatsappStatusSchema = z.object({
  status: z.enum(["disconnected", "connecting", "qr", "connected"]),
  qr: z.string().nullable(),
  number: z.string().nullable(),
});
export type WhatsappStatus = z.infer<typeof WhatsappStatusSchema>;
```

- [ ] **Step 2: Add the hooks**

In `frontend/src/api/hooks.ts`, add `WhatsappStatusSchema` to the existing import from `./schemas`, then append at the end of the file:

```ts
// ── WhatsApp connection hooks ────────────────────────────────────────────────

export const useWhatsappStatus = () =>
  useQuery({
    queryKey: ["whatsapp-status"],
    queryFn: () => api.get("/whatsapp/status", WhatsappStatusSchema),
    refetchInterval: 2000,
  });

export const useConnectWhatsapp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/whatsapp/connect", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["whatsapp-status"] }); },
  });
};

export const useDisconnectWhatsapp = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/whatsapp/disconnect", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["whatsapp-status"] }); },
  });
};
```

(`z`, `useQuery`, `useMutation`, `useQueryClient`, and `api` are already imported in this file.)

- [ ] **Step 3: Verify it type-checks**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts
git commit -m "feat(whatsapp): FE schema and react-query hooks for connection control"
```

---

## Task 6: Frontend WhatsApp page, route, nav, and dynamic badge

**Files:**
- Modify: `frontend/package.json` (+ install `qrcode.react`)
- Create: `frontend/src/pages/WhatsAppPage.tsx`
- Create: `frontend/src/pages/WhatsAppPage.test.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/AppShell.tsx`
- Modify: `frontend/src/pages/ChatPage.tsx`

- [ ] **Step 1: Install the QR library**

Run: `cd frontend && npm install qrcode.react`
Expected: `qrcode.react` added to `dependencies` in `package.json`.

- [ ] **Step 2: Write the failing test**

Create `frontend/src/pages/WhatsAppPage.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import WhatsAppPage from "./WhatsAppPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <WhatsAppPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.restoreAllMocks());

test("shows Connect button when disconnected", async () => {
  vi.spyOn(global, "fetch").mockResolvedValue(
    new Response(JSON.stringify({ status: "disconnected", qr: null, number: null }), {
      headers: { "content-type": "application/json" },
    }),
  );
  renderPage();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /hubungkan whatsapp/i })).toBeInTheDocument(),
  );
});

test("shows the connected number and Disconnect button when connected", async () => {
  vi.spyOn(global, "fetch").mockResolvedValue(
    new Response(JSON.stringify({ status: "connected", qr: null, number: "62812" }), {
      headers: { "content-type": "application/json" },
    }),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/62812/)).toBeInTheDocument());
  expect(screen.getByRole("button", { name: /putuskan/i })).toBeInTheDocument();
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd frontend && npx vitest run src/pages/WhatsAppPage.test.tsx`
Expected: FAIL — `Failed to resolve import "./WhatsAppPage"` (file does not exist yet).

- [ ] **Step 4: Create the page**

Create `frontend/src/pages/WhatsAppPage.tsx`:

```tsx
import { QRCodeSVG } from "qrcode.react";
import { toast } from "sonner";
import { useWhatsappStatus, useConnectWhatsapp, useDisconnectWhatsapp } from "../api/hooks";

/**
 * WhatsApp connection control. Polls the backend for live status and renders
 * the pairing QR / connect / disconnect controls accordingly.
 */
export default function WhatsAppPage() {
  const statusQuery = useWhatsappStatus();
  const connect = useConnectWhatsapp();
  const disconnect = useDisconnectWhatsapp();

  const state = statusQuery.data?.status ?? "disconnected";
  const number = statusQuery.data?.number;
  const qr = statusQuery.data?.qr;

  const handleConnect = () =>
    connect.mutate(undefined, {
      onSuccess: () => toast.success("Memulai koneksi WhatsApp…"),
      onError: (err) => toast.error((err as Error).message),
    });

  const handleDisconnect = () =>
    disconnect.mutate(undefined, {
      onSuccess: () => toast.success("WhatsApp diputuskan"),
      onError: (err) => toast.error((err as Error).message),
    });

  return (
    <div>
      <h1 className="t-h1">WhatsApp</h1>
      <div className="t-sm t-muted" style={{ marginBottom: 12 }}>Hubungkan bot WhatsApp</div>

      <div className="card" style={{ padding: 22, maxWidth: 420 }}>
        {state === "connected" && (
          <div className="col gap-3">
            <p className="t-sm">
              Terhubung sebagai <strong>{number ?? "-"}</strong>
            </p>
            <button
              type="button"
              className="btn btn-danger"
              disabled={disconnect.isPending}
              onClick={handleDisconnect}
            >
              Putuskan
            </button>
          </div>
        )}

        {state === "qr" && qr && (
          <div style={{ textAlign: "center" }}>
            <QRCodeSVG value={qr} size={240} />
            <p className="t-sm t-muted" style={{ marginTop: 12 }}>
              Buka WhatsApp → Perangkat Tertaut → Tautkan Perangkat, lalu scan kode ini.
            </p>
          </div>
        )}

        {state === "connecting" && <p className="t-sm t-muted">Menyambungkan…</p>}

        {state === "disconnected" && (
          <button
            type="button"
            className="btn btn-primary"
            disabled={connect.isPending}
            onClick={handleConnect}
          >
            Hubungkan WhatsApp
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd frontend && npx vitest run src/pages/WhatsAppPage.test.tsx`
Expected: PASS — 2 tests.

- [ ] **Step 6: Add the route**

In `frontend/src/App.tsx`, add the import near the other page imports:

```tsx
import WhatsAppPage from "./pages/WhatsAppPage";
```

Then add the route inside the `<Route element={<AppShell />}>` block, after the `chat` route:

```tsx
        <Route path="chat" element={<ChatPage />} />
        <Route path="whatsapp" element={<WhatsAppPage />} />
```

- [ ] **Step 7: Add the nav entry**

In `frontend/src/components/AppShell.tsx`, add `MessageCircle` to the existing `lucide-react` import, then add this entry to the `NAV` array after the `chat` item:

```tsx
  { to: "/chat",     label: "Chat",       icon: MessageSquare },
  { to: "/whatsapp", label: "WhatsApp",   icon: MessageCircle },
```

- [ ] **Step 8: Make the ChatPage badge dynamic**

In `frontend/src/pages/ChatPage.tsx`, import the status hook (add near the existing imports):

```tsx
import { useWhatsappStatus } from "../api/hooks";
```

Inside the `ChatPage` component body, add:

```tsx
  const waStatus = useWhatsappStatus();
  const waConnected = waStatus.data?.status === "connected";
```

Then replace the static badge block (currently `<span className="badge badge-gain">…Sinkron dengan WhatsApp</span>`) with:

```tsx
        <span className={waConnected ? "badge badge-gain" : "badge"}>
          <span className="badge-dot" style={{ background: "currentColor" }} />
          {waConnected ? "Sinkron dengan WhatsApp" : "WhatsApp tidak terhubung"}
        </span>
```

- [ ] **Step 9: Update the existing ChatPage test for the badge wording**

In `frontend/src/pages/ChatPage.test.tsx`, the `shows WhatsApp sync badge` test currently asserts `/Sinkron dengan WhatsApp/`. With no fetch mock the status is undefined → "WhatsApp tidak terhubung". Update that test to match the default state:

```tsx
test("shows WhatsApp status badge", async () => {
  render(<ChatPage />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText(/WhatsApp tidak terhubung/)).toBeInTheDocument(),
  );
});
```

- [ ] **Step 10: Run the full FE test suite + type-check + build**

Run: `cd frontend && npx tsc --noEmit && npm test && npm run build`
Expected: type-check clean; all tests pass (including the updated ChatPage badge test); production build succeeds.

- [ ] **Step 11: Commit**

```bash
git add frontend/package.json frontend/package-lock.json \
  frontend/src/pages/WhatsAppPage.tsx frontend/src/pages/WhatsAppPage.test.tsx \
  frontend/src/App.tsx frontend/src/components/AppShell.tsx \
  frontend/src/pages/ChatPage.tsx frontend/src/pages/ChatPage.test.tsx
git commit -m "feat(whatsapp): web page to connect/disconnect WhatsApp + dynamic chat badge"
```

---

## Final verification

- [ ] **Backend:** `cd backend && cargo test && cargo build` — all pass.
- [ ] **Frontend:** `cd frontend && npx tsc --noEmit && npm test && npm run build` — all pass.
- [ ] **End-to-end (manual):** `make backend`, then `make gateway`, open the web app → **WhatsApp** page → **Connect** → QR appears → scan → status flips to **connected** with the number → **Putuskan** → status returns to **disconnected**. The Chat page badge tracks the same status.
