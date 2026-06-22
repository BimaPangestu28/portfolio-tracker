export function parseNum(s: string | number | null | undefined): number {
  if (typeof s === "number") return Number.isFinite(s) ? s : 0;
  if (!s) return 0;
  const n = Number(s);
  return Number.isFinite(n) ? n : 0;
}

const idr = new Intl.NumberFormat("id-ID", { style: "currency", currency: "IDR", maximumFractionDigits: 0 });
const usd = new Intl.NumberFormat("en-US", { style: "currency", currency: "USD", minimumFractionDigits: 2, maximumFractionDigits: 2 });

export function formatIDR(v: string | number): string {
  return idr.format(parseNum(v));
}

const idrShortNum = new Intl.NumberFormat("id-ID", { maximumFractionDigits: 1 });

/**
 * Abbreviated Rupiah for space-tight UI (allocation chips, drift rows):
 * miliar→"M" and juta→"jt" with one decimal, ribu→"rb" rounded whole. Values
 * under a thousand fall back to the full formatIDR. Pair with formatIDR in a
 * `title`/tooltip when the exact figure still matters.
 */
export function formatIDRShort(v: string | number): string {
  const n = parseNum(v);
  const abs = Math.abs(n);
  if (abs >= 1e9) return `Rp ${idrShortNum.format(n / 1e9)} M`;
  if (abs >= 1e6) return `Rp ${idrShortNum.format(n / 1e6)} jt`;
  if (abs >= 1e3) return `Rp ${idrShortNum.format(Math.round(n / 1e3))} rb`;
  return formatIDR(n);
}

export function formatUSD(v: string | number): string {
  return usd.format(parseNum(v));
}

export function formatPct(v: string | number): string {
  return `${parseNum(v).toFixed(1)}%`;
}

const qty = new Intl.NumberFormat("id-ID", { maximumFractionDigits: 8 });

/**
 * Format a unit quantity (shares, lots, coins): thousands grouped id-ID
 * style, fractions kept up to 8 digits for fractional crypto amounts.
 */
export function formatQty(v: string | number): string {
  return qty.format(parseNum(v));
}

/**
 * Format a value in its native currency. IDR and USD use the dedicated
 * formatters; any other ISO code is formatted via Intl with a graceful
 * fallback to "<CODE> <number>" when the runtime rejects the currency.
 */
export function formatCurrency(v: string | number, currency: string): string {
  const code = currency.toUpperCase();
  if (code === "IDR") return formatIDR(v);
  if (code === "USD") return formatUSD(v);
  const n = parseNum(v);
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency: code }).format(n);
  } catch {
    return `${code} ${n.toLocaleString()}`;
  }
}
