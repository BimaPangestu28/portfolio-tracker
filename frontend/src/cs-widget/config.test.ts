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

  it("throws when data-key is present but empty", () => {
    expect(() => readConfig(scriptEl({ "data-key": "" }))).toThrow(/data-key/);
  });
});
