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
