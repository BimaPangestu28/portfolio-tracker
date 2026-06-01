import { useState } from "react";
import { toast } from "sonner";
import {
  useAccounts,
  useCreateAccount,
  useDeleteAccount,
  useInstruments,
  useCreateInstrument,
  useDeleteInstrument,
  useManualPrice,
  useManualFx,
} from "../api/hooks";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";

const today = () => new Date().toISOString().slice(0, 10);
const ACCOUNT_TYPES = ["manual", "exchange", "broker", "bank", "wallet"];
const INSTRUMENT_TYPES = ["crypto", "stock_id", "stock_us", "etf", "mutual_fund", "cash", "bond", "gold", "other"];

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
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle>Accounts</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              createAccount.mutate(
                { ...acc, institution: null, note: null },
                { onSuccess: () => toast.success("Account added"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="Account name" className="w-40" placeholder="Name" value={acc.name} onChange={(e) => setAcc({ ...acc, name: e.target.value })} required />
            <Select value={acc.account_type} onValueChange={(v) => setAcc({ ...acc, account_type: v })}>
              <SelectTrigger aria-label="Account type" className="w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {ACCOUNT_TYPES.map((t) => (
                  <SelectItem key={t} value={t}>
                    {t}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input aria-label="Account currency" className="w-28" placeholder="Currency" value={acc.native_currency} onChange={(e) => setAcc({ ...acc, native_currency: e.target.value })} />
            <Button type="submit">Add</Button>
          </form>
          <ul className="text-sm">
            {(accounts.data ?? []).map((a) => (
              <li key={a.id} className="flex justify-between border-b py-1.5">
                <span>
                  {a.name} · {a.account_type} · {a.native_currency}
                </span>
                <button type="button" onClick={() => delAccount.mutate(a.id)} className="text-xs text-destructive hover:underline">
                  delete
                </button>
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Instruments</CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              createInstrument.mutate(
                {
                  symbol: ins.symbol,
                  name: ins.name,
                  instrument_type: ins.instrument_type,
                  native_currency: ins.native_currency,
                  category_id: ins.category_id ? Number(ins.category_id) : null,
                  price_source: ins.price_source,
                  decimals: 8,
                  note: null,
                },
                { onSuccess: () => toast.success("Instrument added"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="Instrument symbol" className="w-32" placeholder="Symbol" value={ins.symbol} onChange={(e) => setIns({ ...ins, symbol: e.target.value })} required />
            <Input aria-label="Instrument name" className="w-40" placeholder="Name" value={ins.name} onChange={(e) => setIns({ ...ins, name: e.target.value })} required />
            <Select value={ins.instrument_type} onValueChange={(v) => setIns({ ...ins, instrument_type: v })}>
              <SelectTrigger aria-label="Instrument type" className="w-36">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {INSTRUMENT_TYPES.map((t) => (
                  <SelectItem key={t} value={t}>
                    {t}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <Input aria-label="Instrument currency" className="w-28" placeholder="Currency" value={ins.native_currency} onChange={(e) => setIns({ ...ins, native_currency: e.target.value })} />
            <Input aria-label="Category id" className="w-40" placeholder="category_id (optional)" value={ins.category_id} onChange={(e) => setIns({ ...ins, category_id: e.target.value })} />
            <Input aria-label="Price source" className="w-72" placeholder="price_source (e.g. coingecko:bitcoin, yahoo:BBCA.JK, manual)" value={ins.price_source} onChange={(e) => setIns({ ...ins, price_source: e.target.value })} />
            <Button type="submit">Add</Button>
          </form>
          <ul className="text-sm">
            {(instruments.data ?? []).map((i) => (
              <li key={i.id} className="flex justify-between border-b py-1.5">
                <span>
                  {i.symbol} · {i.instrument_type} · {i.price_source}
                </span>
                <button type="button" onClick={() => delInstrument.mutate(i.id)} className="text-xs text-destructive hover:underline">
                  delete
                </button>
              </li>
            ))}
          </ul>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Manual price (for reksadana NAV / manual instruments)</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              manualPrice.mutate(
                { instrument_id: Number(price.instrument_id), price: price.price, currency: price.currency, as_of: today() },
                { onSuccess: () => toast.success("Price set"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="Price instrument id" className="w-36" placeholder="instrument_id" value={price.instrument_id} onChange={(e) => setPrice({ ...price, instrument_id: e.target.value })} required />
            <Input aria-label="Price" className="w-32" placeholder="price" value={price.price} onChange={(e) => setPrice({ ...price, price: e.target.value })} required />
            <Input aria-label="Price currency" className="w-28" placeholder="currency" value={price.currency} onChange={(e) => setPrice({ ...price, currency: e.target.value })} />
            <Button type="submit">Set price</Button>
          </form>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>USD → IDR FX rate</CardTitle>
        </CardHeader>
        <CardContent>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              manualFx.mutate(
                { base: "USD", quote: "IDR", rate: fx.rate, as_of: today() },
                { onSuccess: () => toast.success("FX set"), onError: (err) => toast.error((err as Error).message) },
              );
            }}
            className="flex flex-wrap items-end gap-2"
          >
            <Input aria-label="USD to IDR rate" className="w-40" placeholder="e.g. 16250" value={fx.rate} onChange={(e) => setFx({ rate: e.target.value })} required />
            <Button type="submit">Set FX</Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
