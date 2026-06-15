import { describe, it, expect } from "vitest";
import { TodoSchema, ReminderSchema, InboxItemSchema } from "./schemas";

describe("assistant schemas", () => {
  it("parses a todo row", () => {
    const t = TodoSchema.parse({
      id: 1, title: "Bayar internet", notes: null, due_at: "2026-06-15T10:00:00+07:00",
      status: "open", completed_at: null, priority: "high", estimate_minutes: 15,
      created_at: "2026-06-15T08:00:00+07:00",
    });
    expect(t.title).toBe("Bayar internet");
  });

  it("parses a reminder row", () => {
    const r = ReminderSchema.parse({
      id: 2, todo_id: null, message: "Meeting", remind_at: "2026-06-15T15:00:00+07:00",
      recurrence: "none", status: "pending", sent_at: null, event_id: null,
    });
    expect(r.message).toBe("Meeting");
  });

  it("parses an inbox row", () => {
    const i = InboxItemSchema.parse({
      id: 3, content: "Ide produk", status: "pending",
      created_at: "2026-06-15T08:00:00+07:00", resolved_at: null,
    });
    expect(i.content).toBe("Ide produk");
  });
});
