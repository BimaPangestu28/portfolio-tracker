# Connect WhatsApp from the Web Frontend — Design

**Date:** 2026-06-02
**Status:** Approved (pending spec review)

## Overview

Today the WhatsApp bot is paired only from the terminal: `make gateway` prints a
QR code, the user scans it, and the Baileys session persists in `auth_state/`.
The frontend has no control over the connection — it only shows a *static*
"Sinkron dengan WhatsApp" badge that is hardcoded and therefore misleading when
the gateway is down.

This feature adds **full WhatsApp connection control from the web UI**: view live
connection status, scan the pairing QR, and disconnect/reconnect — all without
touching the terminal.

## Goals

- Show live connection status (`disconnected` / `connecting` / `qr` / `connected`)
  in the web UI.
- Render the pairing QR code in the browser so the user can scan it from the app.
- Let the user **Connect** (start a session / request a fresh QR) and
  **Disconnect** (logout, clear session) from the web UI.
- Make the existing "Sinkron dengan WhatsApp" badge reflect real status.

## Non-Goals (YAGNI)

- Multiple WhatsApp numbers / multi-tenant routing.
- Persisting connection history in the database.
- Real backend authentication (the app is single-user, self-hosted; gating stays
  client-side via `isUnlocked`).
- SSE / WebSocket transport (polling is sufficient).
- A test runner for the gateway (plain JS today; out of scope unless requested).

## Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| FE scope | Full control (status + QR + connect + disconnect) | User requirement |
| FE real-time | Polling (~2s) | Simplest, testable, sufficient for single-user |
| Command channel | Gateway polls backend (~3s) | Gateway needs no open port; backend needs no gateway address |
| State storage | In-memory `Arc<Mutex<WaState>>` in `AppState` | QR/status are ephemeral runtime data; no DB migration needed |

## Architecture & Data Flow

```
┌──────────┐  poll status (2s)   ┌────────────────────┐  poll commands (3s)  ┌──────────┐
│ Frontend │ ──────────────────> │   Backend (axum)   │ <─────────────────── │ Gateway  │
│ (React)  │ <────────────────── │ Arc<Mutex<WaState>>│ ──────────────────>  │ (Baileys)│
└──────────┘  {status,qr,number} │                    │  {command|null}      └────┬─────┘
     │                           │                    │ <─── push state ──────────┤
     │ POST connect/disconnect   └────────────────────┘   {status,qr,number}  QR scan │
     └───────────────────────────────────^                                   WhatsApp HP
```

### Backend endpoints

**Gateway-facing** (protected by `x-gateway-token` header == `GATEWAY_TOKEN`, same
as the existing `/chat/whatsapp/inbound`):

- `POST /whatsapp/state` — gateway pushes `{ status, qr?, number? }` on every
  change plus a periodic heartbeat. Backend stores it in `WaState` and records
  `last_seen`.
- `GET /whatsapp/commands` — gateway polls; backend returns `{ command | null }`
  and **clears** the pending command (consume-once).

**Frontend-facing** (open on the backend like every other endpoint; gated
client-side):

- `GET /whatsapp/status` — returns `{ status, qr, number }`. FE polls every ~2s.
- `POST /whatsapp/connect` — sets pending command `restart` (start session /
  request fresh QR).
- `POST /whatsapp/disconnect` — sets pending command `logout`.

### State machine

`status`: `disconnected` → `connecting` → `qr` → `connected`, returning to
`disconnected` on logout or dropped connection.

If the gateway heartbeat (`last_seen`) is older than ~10s, `GET /whatsapp/status`
downgrades a reported `connected` to `connecting`/`stale`, so the UI never shows a
false "connected".

## Components & File Changes

### Backend (Rust)

- **`src/wa_state.rs`** *(new)* — isolated state type:
  ```rust
  pub enum WaStatus { Disconnected, Connecting, Qr, Connected }
  pub enum WaCommand { Restart, Logout }
  pub struct WaState {
      pub status: WaStatus,
      pub qr: Option<String>,
      pub number: Option<String>,
      pub pending_command: Option<WaCommand>,
      pub last_seen: Option<std::time::Instant>,
  }
  pub type SharedWaState = Arc<Mutex<WaState>>;
  ```
  Small methods: `set_state()`, `take_command()`, `set_command()`, plus a
  `status_view()` that applies the staleness rule.
- **`src/main.rs`** — extend `AppState { db, wa: SharedWaState }`.
- **`src/api/whatsapp.rs`** — add handlers `push_state`, `poll_commands`
  (gateway-facing, token-checked), `status`, `connect`, `disconnect`
  (FE-facing). Extract the token check into a small helper shared with `inbound`.
- **`src/api/mod.rs`** — register the 5 new routes.

### Gateway (Node — `index.js`)

- `reportState(status, extra)` → `POST /whatsapp/state`.
- `connection.update`: `qr` → report `qr` + keep the string; `open` → report
  `connected` + number; `close` → report `disconnected`.
- `pollCommands()` loop every ~3s → `GET /whatsapp/commands`:
  - `logout` → `sock.logout()` + remove `auth_state/` folder.
  - `restart` → re-init the socket.
- Refactor into small focused functions (≤ ~20–30 lines each).

### Frontend (React)

- **`src/api/whatsapp.ts`** *(new)* — client functions `getWhatsappStatus`,
  `connectWhatsapp`, `disconnectWhatsapp`.
- **`src/pages/WhatsAppPage.tsx`** *(new)* — `useQuery` with
  `refetchInterval: 2000`. Conditional render:
  - `disconnected` → **Connect** button.
  - `qr` → render QR (via `qrcode.react`) + scan instructions.
  - `connecting` → spinner.
  - `connected` → number + **Disconnect** button.
- Nav + routing — add a "WhatsApp" entry following the existing nav pattern.
- **`src/pages/ChatPage.tsx:53`** — make the "Sinkron dengan WhatsApp" badge
  dynamic from status (green when `connected`, grey otherwise).
- New dependency: `qrcode.react`.

## Error Handling

- **Bad/missing token** (gateway-facing) → `400 bad gateway token`, reusing the
  existing `inbound` pattern.
- **Gateway down / stale** → staleness rule on `last_seen` prevents a false
  "connected"; this also fixes the currently-lying static badge.
- **Logout fails to clear `auth_state`** → gateway logs the error, still reports
  `disconnected`; user can retry Connect.
- **FE polling error** → react-query shows an error state on the page and retries;
  buttons disabled while a mutation is pending.
- **Consume-once commands** → `take_command()` returns once then sets `None`, so
  restart/logout cannot fire twice.
- **Mutex poisoning** → handlers map to `AppError::Other` (500), never panic.

## Testing

### Backend (`cargo test`, following existing patterns)

- `connect`/`disconnect` set the correct command; `poll_commands` consumes once
  then returns empty.
- `push_state` updates `WaState`; `status` reflects the result.
- Token: request without / with wrong token → 400; correct → 200.
- Staleness: an old `last_seen` downgrades `connected` to `connecting`/`stale`.

### Frontend (following `ChatPage.test.tsx`)

- `WhatsAppPage` renders correctly per status (disconnected → Connect button,
  qr → QR shown, connected → number + Disconnect).
- Clicking Connect/Disconnect calls the correct API (mocked).

### Gateway

Plain JS without a test runner today. Refactor into small, mostly-pure functions
where possible; a gateway test runner is out of scope unless requested.
