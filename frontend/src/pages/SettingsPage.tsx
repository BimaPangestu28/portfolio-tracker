import { useState } from "react";
import {
  useAccounts, useCreateAccount, useDeleteAccount,
  useInstruments, useCreateInstrument, useDeleteInstrument,
  useManualPrice, useManualFx,
} from "../api/hooks";

const input = "rounded border px-2 py-1 text-sm";
const today = () => new Date().toISOString().slice(0, 10);

export default function SettingsPage() {
  const accounts = useAccounts();
  const instruments = useInstruments();
  const createAccount = useCreateAccount();
  const delAccount = useDeleteAccount();
  const createInstrument = useCreateInstrument();
  const delInstrument = useDeleteInstrument();
  const manualPrice = useManualPrice();
  const manualFx = useManualFx();

  const [acc, setAcc] = useState({ name: "", account_type: "manual", native_currency: "IDR" });
  const [ins, setIns] = useState({ symbol: "", name: "", instrument_type: "crypto", native_currency: "USD", category_id: "", price_source: "manual" });
  const [price, setPrice] = useState({ instrument_id: "", price: "", currency: "USD" });
  const [fx, setFx] = useState({ rate: "" });

  return (
    <div className="space-y-8">
      <h1 className="text-xl font-semibold">Settings</h1>

      <section className="space-y-2">
        <h2 className="font-semibold">Accounts</h2>
        <form onSubmit={(e) => { e.preventDefault(); createAccount.mutate({ ...acc, institution: null, note: null }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input aria-label="Account name" className={input} placeholder="Name" value={acc.name} onChange={(e) => setAcc({ ...acc, name: e.target.value })} required />
          <select aria-label="Account type" className={input} value={acc.account_type} onChange={(e) => setAcc({ ...acc, account_type: e.target.value })}>
            {["manual", "exchange", "broker", "bank", "wallet"].map((t) => <option key={t}>{t}</option>)}
          </select>
          <input aria-label="Account currency" className={input} placeholder="Currency" value={acc.native_currency} onChange={(e) => setAcc({ ...acc, native_currency: e.target.value })} />
          <button type="submit" className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Add</button>
        </form>
        <ul className="text-sm">
          {(accounts.data ?? []).map((a) => (
            <li key={a.id} className="flex justify-between border-b py-1">
              <span>{a.name} · {a.account_type} · {a.native_currency}</span>
              <button type="button" onClick={() => delAccount.mutate(a.id)} className="text-xs text-red-600">delete</button>
            </li>
          ))}
        </ul>
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">Instruments</h2>
        <form onSubmit={(e) => { e.preventDefault(); createInstrument.mutate({ symbol: ins.symbol, name: ins.name, instrument_type: ins.instrument_type, native_currency: ins.native_currency, category_id: ins.category_id ? Number(ins.category_id) : null, price_source: ins.price_source, decimals: 8, note: null }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input aria-label="Instrument symbol" className={input} placeholder="Symbol" value={ins.symbol} onChange={(e) => setIns({ ...ins, symbol: e.target.value })} required />
          <input aria-label="Instrument name" className={input} placeholder="Name" value={ins.name} onChange={(e) => setIns({ ...ins, name: e.target.value })} required />
          <select aria-label="Instrument type" className={input} value={ins.instrument_type} onChange={(e) => setIns({ ...ins, instrument_type: e.target.value })}>
            {["crypto", "stock_id", "stock_us", "etf", "mutual_fund", "cash", "bond", "gold", "other"].map((t) => <option key={t}>{t}</option>)}
          </select>
          <input aria-label="Instrument currency" className={input} placeholder="Currency" value={ins.native_currency} onChange={(e) => setIns({ ...ins, native_currency: e.target.value })} />
          <input aria-label="Category id" className={input} placeholder="category_id (optional)" value={ins.category_id} onChange={(e) => setIns({ ...ins, category_id: e.target.value })} />
          <input aria-label="Price source" className={input} placeholder="price_source (e.g. coingecko:bitcoin, yahoo:BBCA.JK, manual)" value={ins.price_source} onChange={(e) => setIns({ ...ins, price_source: e.target.value })} />
          <button type="submit" className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Add</button>
        </form>
        <ul className="text-sm">
          {(instruments.data ?? []).map((i) => (
            <li key={i.id} className="flex justify-between border-b py-1">
              <span>{i.symbol} · {i.instrument_type} · {i.price_source}</span>
              <button type="button" onClick={() => delInstrument.mutate(i.id)} className="text-xs text-red-600">delete</button>
            </li>
          ))}
        </ul>
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">Manual price (for reksadana NAV / manual instruments)</h2>
        <form onSubmit={(e) => { e.preventDefault(); manualPrice.mutate({ instrument_id: Number(price.instrument_id), price: price.price, currency: price.currency, as_of: today() }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input aria-label="Price instrument id" className={input} placeholder="instrument_id" value={price.instrument_id} onChange={(e) => setPrice({ ...price, instrument_id: e.target.value })} required />
          <input aria-label="Price" className={input} placeholder="price" value={price.price} onChange={(e) => setPrice({ ...price, price: e.target.value })} required />
          <input aria-label="Price currency" className={input} placeholder="currency" value={price.currency} onChange={(e) => setPrice({ ...price, currency: e.target.value })} />
          <button type="submit" className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Set price</button>
        </form>
      </section>

      <section className="space-y-2">
        <h2 className="font-semibold">USD → IDR FX rate</h2>
        <form onSubmit={(e) => { e.preventDefault(); manualFx.mutate({ base: "USD", quote: "IDR", rate: fx.rate, as_of: today() }); }} className="flex flex-wrap gap-2 rounded border bg-white p-3">
          <input aria-label="USD to IDR rate" className={input} placeholder="e.g. 16250" value={fx.rate} onChange={(e) => setFx({ rate: e.target.value })} required />
          <button type="submit" className="rounded bg-blue-600 px-3 py-1 text-sm text-white">Set FX</button>
        </form>
      </section>
    </div>
  );
}
