import { useState } from "react";
import { Check, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import { useAccounts, useInstruments, useCreateTransaction } from "../api/hooks";
import { TXN_TYPES, TX_LABEL } from "../lib/txn";
import { Dialog } from "./Dialog";

interface AddTransactionDialogProps {
  open: boolean;
  onClose: () => void;
}

export function AddTransactionDialog({ open, onClose }: AddTransactionDialogProps) {
  const accounts = useAccounts();
  const instruments = useInstruments();
  const create = useCreateTransaction();

  const defaultForm = {
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
  };
  const [form, setForm] = useState(defaultForm);

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm({ ...form, [k]: e.target.value });

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
        onSuccess: () => {
          toast.success("Transaksi ditambahkan");
          setForm(defaultForm);
          onClose();
        },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  const accounts_data = accounts.data ?? [];
  const instruments_data = instruments.data ?? [];

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title="Tambah Transaksi"
      sub="Catat aktivitas portofolio secara manual"
      footer={
        <>
          <button type="button" className="btn btn-outline" onClick={onClose}>
            Batal
          </button>
          <button
            type="submit"
            form="tx-form"
            className="btn btn-primary"
            disabled={create.isPending}
            aria-label={create.isPending ? "Menyimpan transaksi" : "Simpan transaksi"}
          >
            {create.isPending ? <RefreshCw size={16} className="animate-spin" /> : <Check size={16} />}
            {create.isPending ? "Menyimpan…" : "Simpan"}
          </button>
        </>
      }
    >
      <form id="tx-form" onSubmit={submit}>
        <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
          <label className="field">
            <span className="field-label">Tipe</span>
            <select
              className="select"
              value={form.txn_type}
              onChange={set("txn_type")}
              aria-label="Tipe transaksi"
            >
              {TXN_TYPES.map((t) => (
                <option key={t} value={t}>{TX_LABEL[t]}</option>
              ))}
            </select>
          </label>
          <label className="field">
            <span className="field-label">Akun</span>
            <select
              className="select"
              value={form.account_id}
              onChange={set("account_id")}
              aria-label="Akun"
            >
              <option value="">— pilih akun —</option>
              {accounts_data.map((a) => (
                <option key={a.id} value={a.id}>{a.name}</option>
              ))}
            </select>
          </label>
        </div>
        <label className="field">
          <span className="field-label">Instrumen</span>
          <select
            className="select"
            value={form.instrument_id}
            onChange={set("instrument_id")}
            aria-label="Instrumen"
          >
            <option value="">— pilih instrumen —</option>
            {instruments_data.map((i) => (
              <option key={i.id} value={i.id}>{i.symbol}</option>
            ))}
          </select>
        </label>
        <div className="grid" style={{ gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>
          <label className="field">
            <span className="field-label">Jumlah</span>
            <input
              type="number"
              className="input"
              placeholder="0"
              value={form.quantity}
              onChange={set("quantity")}
              aria-label="Jumlah"
            />
          </label>
          <label className="field">
            <span className="field-label">Harga</span>
            <input
              type="number"
              className="input"
              placeholder="0"
              value={form.price_native}
              onChange={set("price_native")}
              aria-label="Harga"
            />
          </label>
          <label className="field">
            <span className="field-label">Biaya</span>
            <input
              type="number"
              className="input"
              placeholder="0"
              value={form.fee_native}
              onChange={set("fee_native")}
              aria-label="Biaya"
            />
          </label>
        </div>
        <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
          <label className="field">
            <span className="field-label">Mata Uang</span>
            <input
              className="input"
              placeholder="USD"
              value={form.currency}
              onChange={set("currency")}
              aria-label="Mata uang"
            />
          </label>
          <label className="field">
            <span className="field-label">Tanggal & Waktu</span>
            <input
              type="datetime-local"
              className="input"
              value={form.executed_at}
              onChange={set("executed_at")}
              aria-label="Tanggal dan waktu"
            />
          </label>
        </div>
        {create.error && (
          <div className="t-sm loss">{(create.error as Error).message}</div>
        )}
      </form>
    </Dialog>
  );
}
