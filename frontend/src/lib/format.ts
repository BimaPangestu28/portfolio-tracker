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
