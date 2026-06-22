import { formatIDR, formatIDRShort, formatUSD, formatPct, formatQty, parseNum } from "./format";

test("formatQty groups thousands and keeps fractions", () => {
  expect(formatQty("15625")).toBe("15.625");
  expect(formatQty("0.00052")).toBe("0,00052");
  expect(formatQty("100")).toBe("100");
});

test("parseNum parses decimal strings", () => {
  expect(parseNum("1234.5")).toBe(1234.5);
  expect(parseNum("")).toBe(0);
  expect(parseNum("nope")).toBe(0);
});

test("formatIDR groups with Rp prefix and no decimals", () => {
  expect(formatIDR("4875000")).toBe("Rp 4.875.000");
});

test("formatIDRShort abbreviates by magnitude with id-ID decimals", () => {
  expect(formatIDRShort("205372443")).toBe("Rp 205,4 jt");
  expect(formatIDRShort("1878406")).toBe("Rp 1,9 jt");
  expect(formatIDRShort("626135497")).toBe("Rp 626,1 jt");
  expect(formatIDRShort("2500000000")).toBe("Rp 2,5 M");
  expect(formatIDRShort("850000")).toBe("Rp 850 rb");
  // sub-thousand falls back to the full Rupiah formatter (NBSP and all)
  expect(formatIDRShort("500")).toBe(formatIDR("500"));
});

test("formatUSD shows two decimals", () => {
  expect(formatUSD("300")).toBe("$300.00");
});

test("formatPct shows one decimal and sign", () => {
  expect(formatPct("-10")).toBe("-10.0%");
  expect(formatPct("40")).toBe("40.0%");
});
