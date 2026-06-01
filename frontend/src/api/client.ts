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
  patch: <T>(path: string, schema: z.ZodType<T>, body: unknown) =>
    request(path, schema, { method: "PATCH", body: JSON.stringify(body) }),
  del: (path: string) => request(path, z.unknown(), { method: "DELETE" }),
};
