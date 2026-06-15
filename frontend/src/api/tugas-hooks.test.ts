import { describe, it, expect } from "vitest";
import * as hooks from "./hooks";

describe("tugas hooks", () => {
  it("exports the new mutation hooks", () => {
    expect(typeof hooks.useUpdateTodo).toBe("function");
    expect(typeof hooks.useReopenTodo).toBe("function");
    expect(typeof hooks.useUnresolveInbox).toBe("function");
    expect(typeof hooks.useCreateReminder).toBe("function");
  });
});
