import { describe, it, expect } from "vitest";
import * as hooks from "./hooks";

describe("assistant mutation hooks", () => {
  it("exports the four write hooks", () => {
    expect(typeof hooks.useCreateTodo).toBe("function");
    expect(typeof hooks.useCompleteTodo).toBe("function");
    expect(typeof hooks.useCancelReminder).toBe("function");
    expect(typeof hooks.useResolveInbox).toBe("function");
  });
});
