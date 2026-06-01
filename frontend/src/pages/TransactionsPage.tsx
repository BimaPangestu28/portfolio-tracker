import { useState } from "react";
import { Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useAccounts, useInstruments, useTransactions, useCreateTransaction, useDeleteTransaction } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";

const TXN_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];

export default function TransactionsPage() {
  const txns = useTransactions();
  const accounts = useAccounts();
  const instruments = useInstruments();
  const create = useCreateTransaction();
  const del = useDeleteTransaction();

  const [form, setForm] = useState({
    account_id: "",
    instrument_id: "",
    txn_type: "buy",
    executed_at: new Date().toISOString().slice(0, 16),
    quantity: "",
    price_native: "",
    fee_native: "0",
    currency: "USD",
    fx_to_idr: "16000",
    fx_to_usd: "1",
  });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
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
      },
      {
        onSuccess: () => toast.success("Transaction added"),
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  const setField = (k: string) => (e: React.ChangeEvent<HTMLInputElement>) => setForm({ ...form, [k]: e.target.value });
  const txns_data = txns.data ?? [];

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Transactions</h1>

      <Card>
        <CardContent className="pt-6">
          <form onSubmit={submit} className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <div className="space-y-1">
              <Label>Account</Label>
              <Select value={form.account_id} onValueChange={(v) => setForm({ ...form, account_id: v })}>
                <SelectTrigger aria-label="Account">
                  <SelectValue placeholder="Account…" />
                </SelectTrigger>
                <SelectContent>
                  {(accounts.data ?? []).map((a) => (
                    <SelectItem key={a.id} value={String(a.id)}>
                      {a.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Instrument</Label>
              <Select value={form.instrument_id} onValueChange={(v) => setForm({ ...form, instrument_id: v })}>
                <SelectTrigger aria-label="Instrument">
                  <SelectValue placeholder="Instrument…" />
                </SelectTrigger>
                <SelectContent>
                  {(instruments.data ?? []).map((i) => (
                    <SelectItem key={i.id} value={String(i.id)}>
                      {i.symbol}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Type</Label>
              <Select value={form.txn_type} onValueChange={(v) => setForm({ ...form, txn_type: v })}>
                <SelectTrigger aria-label="Transaction type">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {TXN_TYPES.map((t) => (
                    <SelectItem key={t} value={t}>
                      {t}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="space-y-1">
              <Label>Executed at</Label>
              <Input aria-label="Executed at" type="datetime-local" value={form.executed_at} onChange={setField("executed_at")} />
            </div>
            <div className="space-y-1">
              <Label>Quantity</Label>
              <Input aria-label="Quantity" placeholder="Quantity" value={form.quantity} onChange={setField("quantity")} required />
            </div>
            <div className="space-y-1">
              <Label>Price (native)</Label>
              <Input aria-label="Price (native)" placeholder="Price (native)" value={form.price_native} onChange={setField("price_native")} required />
            </div>
            <div className="space-y-1">
              <Label>Fee</Label>
              <Input aria-label="Fee" placeholder="Fee" value={form.fee_native} onChange={setField("fee_native")} />
            </div>
            <div className="space-y-1">
              <Label>Currency</Label>
              <Input aria-label="Currency" placeholder="Currency" value={form.currency} onChange={setField("currency")} />
            </div>
            <div className="space-y-1">
              <Label>FX → IDR</Label>
              <Input aria-label="FX to IDR" placeholder="FX→IDR" value={form.fx_to_idr} onChange={setField("fx_to_idr")} />
            </div>
            <div className="space-y-1">
              <Label>FX → USD</Label>
              <Input aria-label="FX to USD" placeholder="FX→USD" value={form.fx_to_usd} onChange={setField("fx_to_usd")} />
            </div>
            <Button type="submit" className="col-span-2 sm:col-span-4" disabled={create.isPending}>
              {create.isPending ? "Adding…" : "Add transaction"}
            </Button>
            {create.error && (
              <div className="col-span-2 text-sm text-destructive sm:col-span-4">{(create.error as Error).message}</div>
            )}
          </form>
        </CardContent>
      </Card>

      <QueryState isLoading={txns.isLoading} error={txns.error}>
        <Card className="overflow-hidden">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Date</TableHead>
                <TableHead>Type</TableHead>
                <TableHead>Instr</TableHead>
                <TableHead>Qty</TableHead>
                <TableHead>Price</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {txns_data.map((t) => (
                <TableRow key={t.id}>
                  <TableCell>{t.executed_at.slice(0, 10)}</TableCell>
                  <TableCell>{t.txn_type}</TableCell>
                  <TableCell>#{t.instrument_id}</TableCell>
                  <TableCell>{t.quantity}</TableCell>
                  <TableCell>
                    {t.price_native} {t.currency}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      type="button"
                      variant="ghost"
                      size="icon"
                      aria-label="delete"
                      onClick={() => del.mutate(t.id)}
                      className="text-destructive hover:text-destructive"
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
              {txns_data.length === 0 && (
                <TableRow>
                  <TableCell colSpan={6} className="text-muted-foreground">
                    No transactions yet.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </Card>
      </QueryState>
    </div>
  );
}
