# Thousand-Separator Formatting for Number Inputs

**Date:** 2026-06-19
**Status:** Design approved, pending implementation plan

## Problem

Numeric/money inputs across the frontend are plain `<input type="number">` (or
untyped text inputs) with no thousand grouping. Entering large IDR amounts
(e.g. `49995500`) is error-prone and hard to read while typing. The user wants
all money/number inputs to display Indonesian-style grouping so values are
easier to enter and verify.

## Goal

Display numeric input values with Indonesian formatting — `.` as the thousands
separator and `,` as the decimal separator (e.g. `1.250.000,50`) — while the
value stored in form state and sent to the backend stays the clean canonical
string it is today (`1250000.50`, `.` decimal, no separators). Apply to all
numeric inputs except `anchor_day`.

## Key decisions (locked during brainstorming)

| Decision | Choice |
|---|---|
| Separator style | Indonesian: `.` thousands, `,` decimal (display `1.250.000,50`) |
| Value to API | Unchanged: clean string `1250000.50` (`.` decimal, no separators) |
| Scope | All 16 numeric inputs; **exclude** `anchor_day` (day 1–28 stays a plain number input) |
| Decimal key | `,` is the decimal separator; `.` and other non-digits are stripped (auto-grouping) |
| Max fraction digits | 8 (matches the existing `qty` display formatter) |
| Negative values | Not supported (all fields are non-negative amounts/prices/percentages) |

## Architecture

Two small, isolated units. Pure string helpers (fully unit-testable, zero React)
live in `lib/`; the React component composes them and handles caret/DOM concerns.

```
frontend/src/
  lib/number-input.ts            -- pure helpers: toDisplay / toClean (+ caret helper)
  components/ui/NumberInput.tsx  -- controlled <input> wrapping the helpers
```

`lib/format.ts` already holds id-ID *display* formatters (read-only rendering);
the new input helpers are the *editable* counterpart and live in a sibling file
to keep that file focused.

### Pure helpers — `lib/number-input.ts`

```
toDisplay(clean: string): string
  // "1250000.5" -> "1.250.000,5";  "" -> "";  "0" -> "0";  "1000." -> "1.000,"
  // Operates on the STRING (never Number()) so in-progress states survive:
  // trailing comma ("1.000,"), trailing zeros ("1.250,50").

toClean(display: string): string
  // Strip every char except digits and the FIRST comma; comma -> ".".
  // "1.250.000,50" -> "1250000.50";  "1.000," -> "1000.";  "" -> ""
  // Enforces: at most one decimal separator, at most 8 fraction digits,
  // strips leading zeros except a lone "0" or "0,<frac>".

digitsBeforeCaret / caretFromDigitCount  // small helpers for caret preservation (see §Caret)
```

Both helpers are the single source of truth for the format and are unit-tested
independently of the component.

### Component — `components/ui/NumberInput.tsx`

A controlled input. Public contract:

```ts
interface NumberInputProps
  extends Omit<React.ComponentProps<'input'>, 'value' | 'onChange' | 'type'> {
  value: string;                    // clean canonical, e.g. "1250000.5" (same as today's state)
  onChange: (clean: string) => void;// emits clean canonical, e.g. "1250000.5"
  allowDecimals?: boolean;          // default true; false -> integer only, inputMode="numeric"
}
```

- Renders `<input type="text" inputMode={allowDecimals ? "decimal" : "numeric"}>`.
- Defaults `className` to `"input"` (the design-system class) but a passed
  `className` overrides — so existing per-field sizing (`input w-32`, etc.) still works.
- Passes through `placeholder`, `required`, `disabled`, `id`, `name`, etc.
- Displays `toDisplay(value)`; on each keystroke computes the new clean value
  via `toClean`, calls `onChange(clean)`, and restores the caret (see §Caret).

## Data flow & integration

Form state continues to hold the **clean string** exactly as today; submit code
is unchanged (`Number("1250000.5")` still parses for the two call sites that
convert — CsPricingPage, and `anchor_day` which is excluded anyway). Only the
input element swaps:

```tsx
// before
<input type="number" value={form.amount} onChange={e => set('amount', e.target.value)} />
// after
<NumberInput value={form.amount} onChange={v => set('amount', v)} />
```

### Inputs to migrate (16; `anchor_day` excluded)

| File | Fields |
|---|---|
| `components/AddTransactionDialog.tsx` | quantity, price_native, fee_native |
| `pages/BudgetPage.tsx` | amount (cashflow), monthly_budget (category) |
| `pages/DcaPage.tsx` | monthly_budget, rounding_step  *(NOT anchor_day)* |
| `pages/PlannerPage.tsx` | target_pct (inline editor + dialog), tolerance_band_pct |
| `pages/ImportPage.tsx` | quantity, price_native, amount_native |
| `pages/CsPricingPage.tsx` | price *(still `Number()`-converted at submit)* |
| `pages/SettingsPage.tsx` | price (manual), rate (FX) |

Percentage fields use the same component (`allowDecimals` true); grouping is
invisible for values < 1000, and they gain consistent comma-decimal entry.

## Caret handling

Re-formatting on every keystroke shifts characters (inserted `.`), which would
jump the caret. Algorithm, applied in the component's change handler:

1. Before reformat: count digits to the left of the current caret in the raw
   input text (ignore separators) → `n`.
2. Compute the new display string via `toClean` → `toDisplay`.
3. After setting the new display, place the caret just after the `n`-th digit
   from the left (or end of string if fewer digits). This keeps the caret on the
   same logical digit regardless of inserted grouping dots.

This is done with an uncontrolled-display + `ref` + `useLayoutEffect` (or
`setSelectionRange` right after state update) so typing in the middle of a
number does not bounce the cursor to the end.

## Edge cases

- Empty string → empty (placeholder shows).
- `"0"` → `"0"`; first `,` typed → `"0,"`; leading zeros collapse (`"007"` → `"7"`)
  but `"0,"` / `"0,5"` are preserved.
- Second `,` ignored; fraction capped at 8 digits.
- `.` and other non-digits typed/pasted are stripped; pasting `"1.250.000,50"`
  normalizes to display `1.250.000,50` / clean `1250000.50`.
- Known behavior: a user habitually typing `.` for the decimal point gets it
  read as grouping (stripped), not decimal. Documented; revisit only if it proves
  annoying.

## Testing

- **Unit (vitest)** for `toDisplay`/`toClean`: round-trips, grouping ≥ 1000,
  comma decimal, in-progress trailing comma/zeros, leading-zero collapse, empty,
  pasted-value normalization, fraction-digit cap, second-comma rejection.
- **Component (@testing-library/react + user-event)**: typing `"1000000"` shows
  `"1.000.000"` and emits clean `"1000000"`; typing a decimal comma; one
  type-in-the-middle test asserting the caret does not jump to the end;
  `allowDecimals={false}` rejects a comma.
- Follow conventions: `npm test` (vitest run) green; strict TS, no `any`.

## Out of scope (YAGNI)

- `anchor_day` and any non-numeric input.
- Negative-number entry.
- Currency symbols inside the input (display-side `lib/format.ts` already handles
  rendering elsewhere).
- Locale switching (Indonesian format is fixed for now).
