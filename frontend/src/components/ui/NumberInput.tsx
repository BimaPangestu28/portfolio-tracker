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
