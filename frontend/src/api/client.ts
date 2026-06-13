import { z } from "zod";

const BASE = import.meta.env.VITE_API_BASE ?? "/api";
const TOKEN_KEY = "pt-auth-token";

function authHeader(): Record<string, string> {
  const token = localStorage.getItem(TOKEN_KEY);
  return token ? { authorization: `Bearer ${token}` } : {};
}

async function request<T>(path: string, schema: z.ZodType<T>, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { "content-type": "application/json", ...authHeader() },
    ...init,
  });
  if (res.status === 401) {
    // Token missing/expired/invalid — drop it and tell the app to lock.
    localStorage.removeItem(TOKEN_KEY);
    window.dispatchEvent(new Event("pt-unauthorized"));
  }
  if (!res.ok) {
    let msg = `HTTP ${res.status}`;
    try { const body = await res.json(); if (body?.error) msg = body.error; } catch { /* keep default */ }
    throw new Error(msg);
  }
  const json = await res.json();
  return schema.parse(json);
}

// ── Google Calendar schemas ──────────────────────────────────────────────────

const googleStatusSchema = z.object({
  status: z.enum(["connected", "disconnected", "error"]),
  last_error: z.string().nullable(),
  last_synced_at: z.string().nullable(),
});

const googleStartSchema = z.object({ consent_url: z.string() });

const googleSyncSchema = z.object({
  status: z.enum(["connected", "disconnected", "error"]),
  last_error: z.string().nullable(),
  last_synced_at: z.string().nullable(),
  pushed: z.number(),
  imported: z.number(),
});

// ── API client ───────────────────────────────────────────────────────────────

export const api = {
  get: <T>(path: string, schema: z.ZodType<T>) => request(path, schema, { method: "GET" }),
  post: <T>(path: string, schema: z.ZodType<T>, body: unknown) =>
    request(path, schema, { method: "POST", body: JSON.stringify(body) }),
  patch: <T>(path: string, schema: z.ZodType<T>, body: unknown) =>
    request(path, schema, { method: "PATCH", body: JSON.stringify(body) }),
  del: (path: string) => request(path, z.unknown(), { method: "DELETE" }),

  // Google Calendar integration
  googleStatus: () => request("/google/status", googleStatusSchema),
  googleStart: () => request("/google/oauth/start", googleStartSchema),
  googleSync: () => request("/google/sync", googleSyncSchema, { method: "POST" }),
  googleDisconnect: () => request("/google/disconnect", googleStatusSchema, { method: "POST" }),
};
