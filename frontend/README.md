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
