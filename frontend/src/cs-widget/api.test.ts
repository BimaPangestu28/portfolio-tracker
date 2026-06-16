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
