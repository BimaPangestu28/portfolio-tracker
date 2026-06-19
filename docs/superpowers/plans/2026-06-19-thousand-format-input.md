# Thousand-Format Number Inputs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Display numeric/money inputs with Indonesian grouping (`.` thousands, `,` decimal) while keeping form state and the value sent to the backend as the clean canonical string (`1250000.50`).

**Architecture:** Two pure string helpers (`toDisplay`/`toClean` + caret helpers) in `src/lib/number-input.ts`, and a controlled `NumberInput` React component in `src/components/ui/NumberInput.tsx` that composes them and preserves the caret on reformat. Existing forms swap their `<input>` for `<NumberInput>`; state and submit logic are unchanged.

**Tech Stack:** React 18 + TypeScript (strict), Vite, vitest + @testing-library/react + user-event. Styling via the `.input` design-system class (`src/index.css`) and `cn()` from `@/lib/utils`.

## Global Constraints

- Display format is Indonesian: `.` thousands, `,` decimal (e.g. `1.250.000,50`).
- The value emitted by `onChange` and stored in state is the clean canonical string: digits with `.` as the decimal point and NO separators (e.g. `1250000.50`). Empty input → `""`.
- TypeScript strict; no `any`. Use `cn()` from `@/lib/utils` for class merging.
- Decimal separator on input is `,`; `.` and other non-digits are stripped. Max 8 fraction digits. No negative numbers.
- Scope: all numeric inputs EXCEPT `anchor_day` (DcaPage). Percentage fields are included.
- Tests run with `npm test` (vitest run); typecheck/build with `npm run build` (`tsc -b && vite build`). For a fast typecheck use `npx tsc -b`.
- Conventional commits (`feat:`, `test:`, `refactor:`).

---

## File Structure

- Create: `frontend/src/lib/number-input.ts` — pure helpers (`toClean`, `toDisplay`, `digitsBeforeCaret`, `caretFromDigitCount`).
- Create: `frontend/src/lib/number-input.test.ts` — unit tests for the helpers.
- Create: `frontend/src/components/ui/NumberInput.tsx` — controlled formatted input.
- Create: `frontend/src/components/ui/NumberInput.test.tsx` — component tests.
- Modify (migrate inputs): `AddTransactionDialog.tsx`, `ImportPage.tsx`, `SettingsPage.tsx`, `BudgetPage.tsx`, `DcaPage.tsx`, `PlannerPage.tsx`, `CsPricingPage.tsx`.

All paths are under `frontend/`. Run vitest/build from `frontend/`.

---

## Task 1: Pure formatting helpers

**Files:**
- Create: `frontend/src/lib/number-input.ts`
- Test: `frontend/src/lib/number-input.test.ts`

**Interfaces:**
- Produces:
  - `toClean(display: string): string`
  - `toDisplay(clean: string): string`
  - `digitsBeforeCaret(s: string, pos: number): number`
  - `caretFromDigitCount(s: string, n: number): number`

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/lib/number-input.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { toClean, toDisplay, digitsBeforeCaret, caretFromDigitCount } from "./number-input";

describe("toClean", () => {
  it("strips grouping and converts comma decimal to dot", () => {
    expect(toClean("1.250.000,50")).toBe("1250000.50");
    expect(toClean("1.000")).toBe("1000");
  });
  it("preserves in-progress trailing comma and zeros", () => {
    expect(toClean("1.000,")).toBe("1000.");
    expect(toClean("1.250,50")).toBe("1250.50");
  });
  it("collapses leading zeros but keeps a lone zero and 0,x", () => {
    expect(toClean("007")).toBe("7");
    expect(toClean("0")).toBe("0");
    expect(toClean(",5")).toBe("0.5");
    expect(toClean("0,")).toBe("0.");
  });
  it("keeps only the first comma and caps fraction at 8 digits", () => {
    expect(toClean("1,2,3")).toBe("1.23");
    expect(toClean("1,123456789")).toBe("1.12345678");
  });
  it("returns empty for empty / non-numeric", () => {
    expect(toClean("")).toBe("");
    expect(toClean("abc")).toBe("");
  });
});

describe("toDisplay", () => {
  it("groups integers and uses comma decimal", () => {
    expect(toDisplay("1250000.5")).toBe("1.250.000,5");
    expect(toDisplay("1000")).toBe("1.000");
    expect(toDisplay("1250000.50")).toBe("1.250.000,50");
  });
  it("preserves in-progress trailing dot as trailing comma", () => {
    expect(toDisplay("1000.")).toBe("1.000,");
  });
  it("passes through small values and empty", () => {
    expect(toDisplay("0")).toBe("0");
    expect(toDisplay("0.5")).toBe("0,5");
    expect(toDisplay("")).toBe("");
  });
  it("round-trips with toClean for canonical values", () => {
    for (const v of ["0", "7", "1000", "1250000.5", "1250000.50"]) {
      expect(toClean(toDisplay(v))).toBe(v);
    }
  });
});

describe("caret helpers", () => {
  it("counts digits left of the caret ignoring separators", () => {
    // "1.250.000": indices 0='1' 1='.' 2='2' 3='5' 4='0' 5='.'; pos 5 has digits 1,2,5,0 => 4
    expect(digitsBeforeCaret("1.250.000", 5)).toBe(4);
  });
  it("finds the index just after the nth digit", () => {
    // 3rd digit is at index 3 ('5'); caret goes just after it => index 4
    expect(caretFromDigitCount("1.250.000", 3)).toBe(4);
    expect(caretFromDigitCount("1.250.000", 0)).toBe(0);
    expect(caretFromDigitCount("1.250", 99)).toBe(5);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run src/lib/number-input.test.ts`
Expected: FAIL — module `./number-input` not found / functions undefined.

- [ ] **Step 3: Implement the helpers**

Create `frontend/src/lib/number-input.ts`:

```ts
/**
 * Editable-input counterpart to lib/format.ts (which renders read-only values).
 * Indonesian convention: "." groups thousands, "," is the decimal separator.
 * The "clean" form is the canonical string stored in state / sent to the API:
 * digits with "." decimal and no separators (e.g. "1250000.5"). Both helpers
 * operate on strings (never Number()) so in-progress states — a trailing comma
 * or trailing zeros — survive editing.
 */

/** Display text -> clean canonical string. */
export function toClean(display: string): string {
  if (!display) return "";
  const commaIdx = display.indexOf(",");
  const hasComma = commaIdx !== -1;
  const intRaw = (hasComma ? display.slice(0, commaIdx) : display).replace(/\D/g, "");
  const fracRaw = hasComma ? display.slice(commaIdx + 1).replace(/\D/g, "").slice(0, 8) : "";
  if (intRaw === "" && !hasComma) return "";
  let intPart = intRaw.replace(/^0+(?=\d)/, "");
  if (intPart === "") intPart = "0";
  return hasComma ? `${intPart}.${fracRaw}` : intPart;
}

/** Clean canonical string -> Indonesian display text. */
export function toDisplay(clean: string): string {
  if (clean === "") return "";
  const dotIdx = clean.indexOf(".");
  const hasDot = dotIdx !== -1;
  const intPart = hasDot ? clean.slice(0, dotIdx) : clean;
  const fracPart = hasDot ? clean.slice(dotIdx + 1) : "";
  const intDigits = intPart === "" ? "0" : intPart;
  const grouped = intDigits.replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  return hasDot ? `${grouped},${fracPart}` : grouped;
}

/** Number of digit characters to the left of `pos` in `s` (separators ignored). */
export function digitsBeforeCaret(s: string, pos: number): number {
  let n = 0;
  for (let i = 0; i < pos && i < s.length; i++) {
    if (s[i] >= "0" && s[i] <= "9") n++;
  }
  return n;
}

/** Index in `s` just after the `n`-th digit (1-based). 0 -> start; overflow -> end. */
export function caretFromDigitCount(s: string, n: number): number {
  if (n <= 0) return 0;
  let count = 0;
  for (let i = 0; i < s.length; i++) {
    if (s[i] >= "0" && s[i] <= "9") {
      count++;
      if (count === n) return i + 1;
    }
  }
  return s.length;
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run src/lib/number-input.test.ts`
Expected: PASS (all assertions).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/number-input.ts frontend/src/lib/number-input.test.ts
git commit -m "feat(web): add Indonesian number-input format/parse helpers"
```

---

## Task 2: NumberInput component

**Files:**
- Create: `frontend/src/components/ui/NumberInput.tsx`
- Test: `frontend/src/components/ui/NumberInput.test.tsx`

**Interfaces:**
- Consumes: `toClean`, `toDisplay`, `digitsBeforeCaret`, `caretFromDigitCount` from `@/lib/number-input`; `cn` from `@/lib/utils`.
- Produces:
  - `NumberInputProps` (extends `Omit<React.ComponentProps<'input'>, 'value' | 'onChange' | 'type'>` with `value: string`, `onChange: (clean: string) => void`, `allowDecimals?: boolean`).
  - `NumberInput` (default export name `NumberInput`, a `forwardRef` component).

- [ ] **Step 1: Write the failing component tests**

Create `frontend/src/components/ui/NumberInput.test.tsx`:

```tsx
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
    input.setSelectionRange(1, 1);             // caret right after the leading "1"
    await user.type(input, "9");               // insert -> "1900000" -> "1.900.000"
    expect(input.value).toBe("1.900.000");
    // caret should sit just after the "9" (2 digits from the left), i.e. index 3 ("1.9|00.000")
    expect(input.selectionStart).toBe(3);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx vitest run src/components/ui/NumberInput.test.tsx`
Expected: FAIL — `./NumberInput` not found.

- [ ] **Step 3: Implement the component**

Create `frontend/src/components/ui/NumberInput.tsx`:

```tsx
import * as React from "react";
import { cn } from "@/lib/utils";
import {
  toClean,
  toDisplay,
  digitsBeforeCaret,
  caretFromDigitCount,
} from "@/lib/number-input";

export interface NumberInputProps
  extends Omit<React.ComponentProps<"input">, "value" | "onChange" | "type"> {
  /** Clean canonical string, e.g. "1250000.5". */
  value: string;
  /** Emits the clean canonical string. */
  onChange: (clean: string) => void;
  /** Allow a decimal part (comma). Default true. */
  allowDecimals?: boolean;
}

/**
 * Controlled numeric input that shows Indonesian grouping ("1.250.000,50")
 * while emitting the clean canonical string ("1250000.50"). The caret is kept
 * on the same logical digit across reformatting so typing mid-number does not
 * bounce the cursor to the end.
 */
export const NumberInput = React.forwardRef<HTMLInputElement, NumberInputProps>(
  ({ value, onChange, allowDecimals = true, className, ...props }, forwardedRef) => {
    const innerRef = React.useRef<HTMLInputElement | null>(null);
    const pendingCaret = React.useRef<number | null>(null);

    const setRefs = (el: HTMLInputElement | null) => {
      innerRef.current = el;
      if (typeof forwardedRef === "function") forwardedRef(el);
      else if (forwardedRef) forwardedRef.current = el;
    };

    React.useLayoutEffect(() => {
      const el = innerRef.current;
      if (el && pendingCaret.current != null) {
        const pos = caretFromDigitCount(el.value, pendingCaret.current);
        el.setSelectionRange(pos, pos);
        pendingCaret.current = null;
      }
    });

    const handleChange = (e: React.ChangeEvent<HTMLInputElement>) => {
      const raw = e.target.value;
      const caret = e.target.selectionStart ?? raw.length;
      const sanitized = allowDecimals ? raw : raw.replace(/,/g, "");
      pendingCaret.current = digitsBeforeCaret(sanitized, caret);
      onChange(toClean(sanitized));
    };

    return (
      <input
        {...props}
        ref={setRefs}
        type="text"
        inputMode={allowDecimals ? "decimal" : "numeric"}
        className={cn(className ?? "input")}
        value={toDisplay(value)}
        onChange={handleChange}
      />
    );
  }
);
NumberInput.displayName = "NumberInput";
```

Note: `className={cn(className ?? "input")}` — when a caller passes a className (existing forms pass `"input"`, `"input w-32"`, etc.) it is used verbatim; when none is passed the component defaults to the `.input` class. A caller that wants no base class (e.g. the inline % editor styled purely with inline `style`) passes `className=""`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx vitest run src/components/ui/NumberInput.test.tsx`
Expected: PASS. If the caret test is flaky under jsdom's `selectionStart`, keep the assertion on `input.value` and the emitted clean value (those are the behavioral guarantees); the caret index check may be relaxed to `>= 1` only if jsdom cannot report selection — but try the exact assertion first.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/ui/NumberInput.tsx frontend/src/components/ui/NumberInput.test.tsx
git commit -m "feat(web): add NumberInput component with caret-stable grouping"
```

---

## Task 3: Migrate transaction & settings inputs

**Files:**
- Modify: `frontend/src/components/AddTransactionDialog.tsx` (quantity, price_native, fee_native)
- Modify: `frontend/src/pages/ImportPage.tsx` (quantity, price_native, amount_native)
- Modify: `frontend/src/pages/SettingsPage.tsx` (manual price, FX rate)

**Interfaces:**
- Consumes: `NumberInput` from `@/components/ui/NumberInput` (Task 2).

**The swap pattern (apply to every field below):** replace the numeric `<input>` with `<NumberInput>`, keeping the SAME `value`, the SAME wrapper/label, and the SAME `className`/`style`. The `onChange` now receives the clean string directly instead of an event.

```tsx
// before — type="number" or untyped text input
<input type="number" className="input" value={form.quantity}
       onChange={e => setForm({ ...form, quantity: e.target.value })} />
// after
<NumberInput className="input" value={form.quantity}
       onChange={v => setForm({ ...form, quantity: v })} />
```

State shape and submit code stay exactly as they are (they already hold/send clean strings). Do NOT change how values are read at submit.

- [ ] **Step 1: Add the import to each of the three files**

At the top of `AddTransactionDialog.tsx`, `ImportPage.tsx`, and `SettingsPage.tsx`:

```tsx
import { NumberInput } from "@/components/ui/NumberInput";
```

- [ ] **Step 2: AddTransactionDialog — swap quantity, price_native, fee_native**

For each of the three `<input type="number" ...>` fields (quantity, price_native, fee_native), apply the swap pattern. Keep `required` where present; keep the existing `className`. Drop the `type="number"` attribute (NumberInput sets `type="text"`/`inputMode` itself). Convert each `onChange={e => ...e.target.value...}` to `onChange={v => ...v...}`.

- [ ] **Step 3: ImportPage — swap quantity, price_native, amount_native**

These use `className="input"` with no `type`. Apply the swap pattern to all three, converting the `onChange` from event to clean-string. Keep the surrounding markup.

- [ ] **Step 4: SettingsPage — swap manual price and FX rate**

Swap the `price` input (`className="input w-32"`) and the `rate` input (`className="input w-40"`). Preserve those exact classNames so widths are unchanged. Convert their `onChange` to clean-string.

- [ ] **Step 5: Typecheck and run the suite**

Run: `cd frontend && npx tsc -b && npm test`
Expected: tsc reports no errors; vitest suite passes (existing tests + Tasks 1-2 tests). If tsc complains that `onChange` no longer matches an event handler somewhere, fix that call site to use the clean-string signature.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/AddTransactionDialog.tsx frontend/src/pages/ImportPage.tsx frontend/src/pages/SettingsPage.tsx
git commit -m "feat(web): use NumberInput for transaction, import, and settings amounts"
```

---

## Task 4: Migrate budget, DCA, planner & pricing inputs

**Files:**
- Modify: `frontend/src/pages/BudgetPage.tsx` (cashflow amount, category monthly_budget)
- Modify: `frontend/src/pages/DcaPage.tsx` (monthly_budget, rounding_step — **NOT anchor_day**)
- Modify: `frontend/src/pages/PlannerPage.tsx` (target_pct inline editor + dialog, tolerance_band_pct)
- Modify: `frontend/src/pages/CsPricingPage.tsx` (price)

**Interfaces:**
- Consumes: `NumberInput` from `@/components/ui/NumberInput` (Task 2).

Apply the same swap pattern as Task 3 (keep value/className/style/required; convert `onChange` from event to clean string; drop `type="number"`).

- [ ] **Step 1: Add the import to each of the four files**

```tsx
import { NumberInput } from "@/components/ui/NumberInput";
```

- [ ] **Step 2: BudgetPage — swap cashflow `amount` and category `monthly_budget`**

Apply the swap pattern. For `monthly_budget`, the submit code is `monthly_budget: catForm.monthly_budget || null` — leave that untouched (NumberInput emits `""` when cleared, so `|| null` still works).

- [ ] **Step 3: DcaPage — swap `monthly_budget` and `rounding_step` only**

Apply the swap to `monthly_budget` and `rounding_step`. **Do NOT touch `anchor_day`** — leave it as its existing `<input type="number" min={1} max={28}>`. The submit code `monthly_budget: form.monthly_budget || "0"` and `rounding_step: form.rounding_step || "10000"` stays unchanged.

- [ ] **Step 4: PlannerPage — swap target_pct (inline editor + dialog) and tolerance_band_pct**

- Dialog fields `target_pct` and `tolerance_band_pct`: standard swap (they use `className="input"`).
- Inline `TargetEditor` input (the one styled with inline `style={{ width: 64, ... }}` and no `.input` class): swap to `<NumberInput value={target} onChange={setTarget} style={...} className="" .../>` — pass `className=""` so the base `.input` class is NOT forced and the existing inline-styled look is preserved. Keep its inline `style` exactly.

- [ ] **Step 5: CsPricingPage — swap `price`**

Standard swap. The submit converts with `price: form.price ? Number(form.price) : null` — leave it; `Number("1250000.5")` parses the clean string correctly, and an empty `""` is falsy so it still maps to `null`.

- [ ] **Step 6: Typecheck and run the suite**

Run: `cd frontend && npx tsc -b && npm test`
Expected: no tsc errors; vitest suite passes. Confirm `anchor_day` was left as a plain number input (grep): `grep -n "anchor_day" frontend/src/pages/DcaPage.tsx` should still show a plain `<input ... type="number"`.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/BudgetPage.tsx frontend/src/pages/DcaPage.tsx frontend/src/pages/PlannerPage.tsx frontend/src/pages/CsPricingPage.tsx
git commit -m "feat(web): use NumberInput for budget, DCA, planner, and pricing amounts"
```

---

## Task 5: Full build verification

**Files:** none (verification only).

- [ ] **Step 1: Production build (type-checks every touched file)**

Run: `cd frontend && npm run build`
Expected: `tsc -b` passes and both vite builds complete with no errors.

- [ ] **Step 2: Full test suite**

Run: `cd frontend && npm test`
Expected: all tests pass (helpers, component, and every pre-existing suite).

- [ ] **Step 3: Inventory check — every targeted input migrated, anchor_day untouched**

Run:
```bash
cd frontend && grep -rn 'type="number"' src/components/AddTransactionDialog.tsx src/pages/ImportPage.tsx src/pages/SettingsPage.tsx src/pages/BudgetPage.tsx src/pages/PlannerPage.tsx src/pages/CsPricingPage.tsx
```
Expected: NO matches (all migrated). Then:
```bash
grep -n 'anchor_day' src/pages/DcaPage.tsx
```
Expected: still a plain `<input ... type="number" min={1} max={28}>` (intentionally not migrated).

- [ ] **Step 4: Manual smoke (document result)**

With `npm run dev`, open Add Transaction and the Budget entry dialog: type a large amount (e.g. `49995500`) and confirm it shows `49.995.500`; type a decimal with `,` and confirm the fraction shows; submit one entry and confirm it persists (value reaches the API as the clean string). Record the result in the task report. (No code change in this task.)

---

## Self-Review Notes

- **Spec coverage:** Indonesian format `.`/`,` (Task 1 helpers) · clean canonical to API (helpers + unchanged submit code, Tasks 3-4) · scope 16 inputs incl. percentages, anchor_day excluded (Tasks 3-4 + Task 5 grep) · comma decimal / `.` stripped / 8-digit cap / no negatives (Task 1 tests) · caret stability (Task 2 component + test) · vitest unit + component tests (Tasks 1-2) · build/typecheck (Task 5). All spec sections mapped.
- **Type consistency:** `toClean`/`toDisplay`/`digitsBeforeCaret`/`caretFromDigitCount` (Task 1) are consumed by name in `NumberInput` (Task 2); `NumberInput` / `NumberInputProps` with `value: string` + `onChange: (clean: string) => void` are consumed identically in Tasks 3-4. Class merge uses `cn(className ?? "input")` consistently.
- **No new dependency:** uses existing react, `cn`, vitest, @testing-library — Task verified against package.json.
