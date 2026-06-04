export const TXN_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];

export const TX_TONE: Record<string, string> = {
  buy: "badge-gain",
  sell: "badge-loss",
  dividend: "badge-primary",
  interest: "badge-primary",
  fee: "badge-warn",
  deposit: "badge-gain",
  withdrawal: "badge-loss",
  opening_balance: "badge-neutral",
};

export const TX_LABEL: Record<string, string> = {
  buy: "Beli",
  sell: "Jual",
  dividend: "Dividen",
  interest: "Bunga",
  fee: "Biaya",
  deposit: "Deposit",
  withdrawal: "Tarik",
  opening_balance: "Saldo Awal",
};
