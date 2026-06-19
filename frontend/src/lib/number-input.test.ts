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
  it("advances past a trailing comma after the last digit", () => {
    // "1.250.000," has 7 digits; the 7th digit '0' is at index 8, next char is ',' at index 9,
    // so the caret advances past the comma to index 10
    expect(caretFromDigitCount("1.250.000,", 7)).toBe(10);
  });
});
