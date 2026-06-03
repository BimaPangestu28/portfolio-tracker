# JWT Authentication — Design

Date: 2026-06-03

## Problem

Authentication today is a **frontend-only mock** (`frontend/src/auth/AuthContext.tsx`,
self-labelled "⚠️ FRONTEND MOCK — NOT REAL SECURITY"): the master password is
SHA-256'd and stored in `localStorage`, and the backend has no auth at all. After
deploying to a public domain, the entire API (transactions, portfolio, chat,
WhatsApp control + pairing QR) was reachable by anyone. A stop-gap HTTP basic-auth
was added at the nginx ingress, but it is a band-aid (and is causing browser
`ERR_TOO_MANY_RETRIES` from a stale cached credential).

Replace it with real server-side auth: the backend verifies the master password
and issues a **JWT**; protected API routes require that token. Then remove the
ingress basic-auth.

## Goals

- Server-enforced auth on all data/control endpoints.
- A single master password, provisioned via env/secret, exchanged for a JWT.
- Keep the gateway's existing `x-gateway-token` mechanism for gateway endpoints.
- Remove the ingress basic-auth band-aid.

## Non-goals

- Multi-user, roles, or permissions (single-user app).
- First-run password setup / in-app password change (password is server-configured).
- Refresh tokens / rotation.
- Demo mode (removed).

## Approach

Stateless **JWT (HS256)**: `POST /auth/login` verifies the password and returns a
signed token; a backend middleware validates the token on every protected route.
No server-side session store.

Alternative considered — httpOnly session cookie (safer against XSS, but needs a
session store + CSRF handling). For a single-user, self-hosted app with no
third-party scripts, JWT-in-localStorage is the right trade-off. Recorded under
Security notes as the future hardening path.

## Backend (Rust / axum)

### Config (env, supplied via the k8s `portfolio-secrets`)

- `AUTH_PASSWORD` — the master password (compared in constant time).
- `JWT_SECRET` — HS256 signing key, ≥32 bytes random.
- `AUTH_TOKEN_TTL_DAYS` — optional, default `30`.

**Enforcement toggle (deliberate):** auth is enforced only when both
`AUTH_PASSWORD` and `JWT_SECRET` are set. When unset (local dev, the existing test
suite), the middleware allows requests and `/auth/login` returns a dev token. This
mirrors the codebase's existing `gateway token: None ⇒ allow` philosophy and avoids
retrofitting tokens into ~84 existing endpoint tests. The production k8s manifest
sets both, so production is always enforced. The trade-off (forgetting to set them
leaves the deployment open) is mitigated by making them required in the manifest
and noting it in `k8s/README.md`.

### New dependency

- `jsonwebtoken = "9"` (HS256 encode/verify).
- Constant-time comparison via the `subtle` crate (or `constant_time_eq`).

### Endpoints

- `POST /auth/login` `{ "password": "..." }`
  - On match: `200 { "token": "<jwt>" }`. Claims: `{ sub: "owner", iat, exp }`.
  - On mismatch: `401 { "error": "Sandi salah" }`.
- `GET /auth/me` (protected) → `200 { "ok": true }`. Lets the frontend validate a
  stored token on load.

### Middleware

`require_auth`: extract `Authorization: Bearer <jwt>`, verify signature + `exp`
with `JWT_SECRET`. On failure → `401 { error }`. No-op when auth is not configured
(dev).

### Router grouping (`backend/src/api/mod.rs`)

- **Public** (no auth): `/health`, `/auth/login`.
- **Gateway** (existing `x-gateway-token` check inside handlers, unchanged):
  `/chat/whatsapp/inbound`, `/whatsapp/state`, `/whatsapp/commands`.
- **Protected** (JWT middleware via `.route_layer`): everything else — accounts,
  categories, instruments, transactions, prices, fx, portfolio, goals, chat,
  `/whatsapp/status`, `/whatsapp/connect`, `/whatsapp/disconnect`, `/auth/me`.

Implementation: build the protected `Router` and apply `route_layer(from_fn(require_auth))`,
then `.merge()` the public and gateway routers.

## Frontend

- **`api/client.ts`**: attach `Authorization: Bearer <token>` (from `localStorage`
  key `pt-auth-token`) to every request. On `401`, clear the token and trigger a
  lock (so the app returns to the login screen).
- **`auth/AuthContext.tsx`**: replace the SHA-256/localStorage mock.
  - `isUnlocked` = a token is present.
  - `unlock(pw)` → `POST /auth/login`; store token + unlock, or return an error.
  - `lock()` → remove token + lock.
  - Remove `setup`, `loginDemo`, `resetPassword`, `hasPassword`, `isDemo`.
  - On load with a stored token, optionally call `GET /auth/me`; a `401` locks.
- **`pages/LoginPage.tsx`**: collapse to a single login screen — one master-password
  field calling `unlock`. Keep the existing visual design / aside. Remove first-run
  setup + confirm, "Lupa sandi?", and the demo button.
- **`App.tsx`**: unchanged — still gates on `isUnlocked`.

## Deployment

- Add `AUTH_PASSWORD` and `JWT_SECRET` to the `portfolio-secrets` k8s secret.
- Add both env vars to the backend Deployment (`k8s/10-backend.yaml`).
- **Remove** the ingress basic-auth annotations + the `portfolio-basic-auth` secret
  (`k8s/40-ingress.yaml`). This also fixes `ERR_TOO_MANY_RETRIES`.
- Rebuild images via the Actions workflow → `kubectl rollout restart`.
- Order: deploy the auth-enabled backend (with the secret + public `/auth/login`)
  first, then remove basic-auth, so login is reachable throughout.

## Data flow

1. App opens → no token → `LoginPage`.
2. User submits master password → `POST /api/auth/login` → token stored.
3. SPA requests carry `Bearer` token → middleware verifies → `200`.
4. Token expires (30d) or is invalid → `401` → frontend clears token → `LoginPage`.

## Error handling

- Wrong password → `401 { error: "Sandi salah" }` → login form shows the message.
- Missing/expired/invalid token on a protected route → `401` → frontend logs out.
- Gateway endpoints continue to use `x-gateway-token` (unchanged).

## Security notes

- JWT in `localStorage` is XSS-exposed; acceptable for a single-user self-hosted app
  with no third-party scripts. httpOnly cookie is the future hardening path.
- `JWT_SECRET` is random ≥32 bytes; rotating it invalidates all tokens (forces
  re-login).
- `AUTH_PASSWORD` is compared in constant time.

## Testing (TDD)

- **Backend**: login success/failure; JWT issue + verify; middleware rejects
  missing / malformed / expired tokens; protected route returns `401` without a
  token and `200` with one; gateway endpoints still work via their token; public
  routes stay open; auth-disabled (dev) mode allows requests.
- **Frontend**: `AuthContext.unlock` against an MSW-mocked `/auth/login` (success +
  failure); client attaches the `Bearer` header and locks on `401`; `LoginPage`
  renders the single-field form and surfaces errors.
