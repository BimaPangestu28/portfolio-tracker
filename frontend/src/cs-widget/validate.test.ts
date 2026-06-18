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
