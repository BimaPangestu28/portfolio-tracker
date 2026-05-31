import { useState } from "react";
import { useAccounts, useInstruments, useTransactions, useCreateTransaction, useDeleteTransaction } from "../api/hooks";
import { QueryState } from "../components/QueryState";

const TXN_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];

export default function TransactionsPage() {
  const txns = useTransactions();
  const accounts = useAccounts();
  const instruments = useInstruments();
  const create = useCreateTransaction();
  const del = useDeleteTransaction();

  const [form, setForm] = useState({
    account_id: "", instrument_id: "", txn_type: "buy",
    executed_at: new Date().toISOString().slice(0, 16),
    quantity: "", price_native: "", fee_native: "0",
    currency: "USD", fx_to_idr: "16000", fx_to_usd: "1",
  });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate({
      account_id: Number(form.account_id),
      instrument_id: Number(form.instrument_id),
      txn_type: form.txn_type,
      executed_at: new Date(form.executed_at).toISOString(),
      quantity: form.quantity,
      price_native: form.price_native,
      fee_native: form.fee_native,
      currency: form.currency,
      fx_to_idr: form.fx_to_idr,
      fx_to_usd: form.fx_to_usd,
    });
  };

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => setForm({ ...form, [k]: e.target.value });
  const input = "rounded border px-2 py-1 text-sm";

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Transactions</h1>

      <form onSubmit={submit} className="grid grid-cols-2 gap-2 rounded border bg-white p-4 sm:grid-cols-4">
        <select className={input} value={form.account_id} onChange={set("account_id")} required>
          <option value="">Account…</option>
          {(accounts.data ?? []).map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
        </select>
        <select className={input} value={form.instrument_id} onChange={set("instrument_id")} required>
          <option value="">Instrument…</option>
          {(instruments.data ?? []).map((i) => <option key={i.id} value={i.id}>{i.symbol}</option>)}
        </select>
        <select className={input} value={form.txn_type} onChange={set("txn_type")}>
          {TXN_TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
        </select>
        <input className={input} type="datetime-local" value={form.executed_at} onChange={set("executed_at")} />
        <input className={input} placeholder="Quantity" value={form.quantity} onChange={set("quantity")} required />
        <input className={input} placeholder="Price (native)" value={form.price_native} onChange={set("price_native")} required />
        <input className={input} placeholder="Fee" value={form.fee_native} onChange={set("fee_native")} />
        <input className={input} placeholder="Currency" value={form.currency} onChange={set("currency")} />
        <input className={input} placeholder="FX→IDR" value={form.fx_to_idr} onChange={set("fx_to_idr")} />
        <input className={input} placeholder="FX→USD" value={form.fx_to_usd} onChange={set("fx_to_usd")} />
        <button className="col-span-2 rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50 sm:col-span-4" disabled={create.isPending}>
          {create.isPending ? "Adding…" : "Add transaction"}
        </button>
        {create.error && <div className="col-span-2 text-sm text-red-600 sm:col-span-4">{(create.error as Error).message}</div>}
      </form>

      <QueryState isLoading={txns.isLoading} error={txns.error}>
        <div className="overflow-x-auto rounded border bg-white">
          <table className="w-full text-sm">
            <thead className="bg-gray-50 text-left text-xs uppercase text-gray-500">
              <tr><th className="p-2">Date</th><th className="p-2">Type</th><th className="p-2">Instr</th><th className="p-2">Qty</th><th className="p-2">Price</th><th className="p-2"></th></tr>
            </thead>
            <tbody>
              {(txns.data ?? []).map((t) => (
                <tr key={t.id} className="border-t">
                  <td className="p-2">{t.executed_at.slice(0, 10)}</td>
                  <td className="p-2">{t.txn_type}</td>
                  <td className="p-2">#{t.instrument_id}</td>
                  <td className="p-2">{t.quantity}</td>
                  <td className="p-2">{t.price_native} {t.currency}</td>
                  <td className="p-2 text-right">
                    <button onClick={() => del.mutate(t.id)} className="text-xs text-red-600 hover:underline">delete</button>
                  </td>
                </tr>
              ))}
              {(txns.data ?? []).length === 0 && <tr><td colSpan={6} className="p-3 text-gray-500">No transactions yet.</td></tr>}
            </tbody>
          </table>
        </div>
      </QueryState>
    </div>
  );
}
