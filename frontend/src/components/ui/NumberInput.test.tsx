import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { NumberInput } from "./NumberInput";

function Harness({ allowDecimals = true }: { allowDecimals?: boolean }) {
  const [v, setV] = useState("");
  return (
    <>
      <NumberInput aria-label="amount" value={v} onChange={setV} allowDecimals={allowDecimals} />
      <output>{v}</output>
    </>
  );
}

describe("NumberInput", () => {
  it("formats grouping as you type and emits the clean value", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByLabelText("amount") as HTMLInputElement;
    await user.type(input, "1000000");
    expect(input.value).toBe("1.000.000");
    expect(screen.getByText("1000000")).toBeInTheDocument();
  });

  it("accepts a comma decimal", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByLabelText("amount") as HTMLInputElement;
    await user.type(input, "1250000,5");
    expect(input.value).toBe("1.250.000,5");
    expect(screen.getByText("1250000.5")).toBeInTheDocument();
  });

  it("ignores comma when decimals are disabled", async () => {
    const user = userEvent.setup();
    render(<Harness allowDecimals={false} />);
    const input = screen.getByLabelText("amount") as HTMLInputElement;
    await user.type(input, "1000,5"); // comma stripped -> "10005"
    expect(input.value).toBe("10.005");
    expect(screen.getByText("10005")).toBeInTheDocument();
  });

  it("keeps the caret on the same digit when typing in the middle", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByLabelText("amount") as HTMLInputElement;
    await user.type(input, "100000");          // value "100.000"
    input.focus();
    input.setSelectionRange(1, 1);             // caret right after the leading "1"
    await user.keyboard("9");                  // insert -> "1900000" -> "1.900.000"
    expect(input.value).toBe("1.900.000");
    // caret should sit just after the "9" (2 digits from the left), i.e. index 3 ("1.9|00.000")
    expect(input.selectionStart).toBe(3);
  });
});
