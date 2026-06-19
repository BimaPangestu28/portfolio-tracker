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

/**
 * Index in `s` just after the `n`-th digit (1-based). 0 -> start; overflow -> end.
 * If the character immediately after the nth digit is a comma (Indonesian decimal
 * separator), the caret advances past it so subsequent typing stays on the correct
 * side of the separator.
 */
export function caretFromDigitCount(s: string, n: number): number {
  if (n <= 0) return 0;
  let count = 0;
  for (let i = 0; i < s.length; i++) {
    if (s[i] >= "0" && s[i] <= "9") {
      count++;
      if (count === n) {
        const next = i + 1;
        return next < s.length && s[next] === "," ? next + 1 : next;
      }
    }
  }
  return s.length;
}
