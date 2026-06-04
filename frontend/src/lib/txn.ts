export const TXN_TYPES = [
  "buy", "sell", "dividend", "interest",
  "fee", "deposit", "withdrawal", "opening_balance",
] as const;

export type TxnType = (typeof TXN_TYPES)[number];

// Record<TxnType, string> enforces full coverage at compile time —
// adding a new type to TXN_TYPES without a tone/label won't compile.
export const TX_TONE: Record<TxnType, string> = {
  buy: "badge-gain",
  sell: "badge-loss",
  dividend: "badge-primary",
  interest: "badge-primary",
  fee: "badge-warn",
  deposit: "badge-gain",
  withdrawal: "badge-loss",
  opening_balance: "badge-neutral",
};

export const TX_LABEL: Record<TxnType, string> = {
  buy: "Beli",
  sell: "Jual",
  dividend: "Dividen",
  interest: "Bunga",
  fee: "Biaya",
  deposit: "Deposit",
  withdrawal: "Tarik",
  opening_balance: "Saldo Awal",
};

const isTxnType = (t: string): t is TxnType => (TXN_TYPES as readonly string[]).includes(t);

// API-nya masih ngirim txn_type sebagai string bebas (z.string()) — helper ini
// degradasi gracefully buat tipe yang gak dikenal frontend.
export const txTone = (t: string): string => (isTxnType(t) ? TX_TONE[t] : "badge-neutral");
export const txLabel = (t: string): string => (isTxnType(t) ? TX_LABEL[t] : t);
