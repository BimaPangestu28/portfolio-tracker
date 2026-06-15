import { describe, it, expect } from "vitest";
import { InvoiceSchema, ClientSchema, InvoiceLineItemSchema } from "./schemas";

describe("invoice schemas", () => {
  it("parses an invoice row", () => {
    const inv = InvoiceSchema.parse({
      id: 1, number: "INV/2026/VI/001", client_id: 3, issue_date: "2026-06-11", due_date: "2026-06-25",
      subtotal: "Rp 12.000.000", total: "Rp 12.000.000",
      line_items_json: '[{"title":"Landing","body":null,"qty":1,"amount":12000000}]',
      created_at: "2026-06-11T08:00:00Z",
    });
    expect(inv.number).toBe("INV/2026/VI/001");
    const items = InvoiceLineItemSchema.array().parse(JSON.parse(inv.line_items_json));
    expect(items[0].amount).toBe(12000000);
  });

  it("parses a client row", () => {
    const c = ClientSchema.parse({ id: 3, name: "PT AIS", sub_name: null, website: null, created_at: "2026-06-01T00:00:00Z" });
    expect(c.name).toBe("PT AIS");
  });
});
