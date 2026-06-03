import { http, HttpResponse } from "msw";
import { z } from "zod";
import { server } from "../test/server";
import { api } from "./client";

const TOKEN_KEY = "pt-auth-token";

afterEach(() => localStorage.clear());

test("attaches Bearer token from localStorage", async () => {
  localStorage.setItem(TOKEN_KEY, "tok123");
  let seen: string | null = null;
  server.use(
    http.get("/api/ping", ({ request }) => {
      seen = request.headers.get("authorization");
      return HttpResponse.json({ ok: true });
    }),
  );
  await api.get("/ping", z.object({ ok: z.boolean() }));
  expect(seen).toBe("Bearer tok123");
});

test("on 401 clears token and dispatches pt-unauthorized", async () => {
  localStorage.setItem(TOKEN_KEY, "tok123");
  let fired = false;
  window.addEventListener("pt-unauthorized", () => { fired = true; });
  server.use(
    http.get("/api/secret", () => new HttpResponse(null, { status: 401 })),
  );
  await expect(api.get("/secret", z.object({}))).rejects.toThrow();
  expect(localStorage.getItem(TOKEN_KEY)).toBeNull();
  expect(fired).toBe(true);
});
