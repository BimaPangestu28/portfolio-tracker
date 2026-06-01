import { useState } from "react";
import { useConfirmReview, useRejectReview, useCreateInstrument, useCreateAccount } from "../api/hooks";
import { ExtractedEntrySchema, type ReviewItem, type Instrument, type Account } from "../api/schemas";

const ENTRY_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];
const CREATE_NEW = "__new__";

function parsePayload(json: string) {
  try {
    const parsed = ExtractedEntrySchema.partial().parse(JSON.parse(json));
    return parsed;
  } catch {
    return {};
  }
}

export function ReviewRow({ item, instruments, accounts }: { item: ReviewItem; instruments: Instrument[]; accounts: Account[] }) {
  const p = parsePayload(item.payload_json);
  const confirm = useConfirmReview();
  const reject = useRejectReview();
  const createInstrument = useCreateInstrument();
  const createAccount = useCreateAccount();
  const [actionError, setActionError] = useState<string | null>(null);

  // If a symbol is extracted but no instrument is matched, pre-select inline-create so the symbol is visible
  const defaultInstrumentId = item.suggested_instrument_id
    ? String(item.suggested_instrument_id)
    : p.symbol
    ? CREATE_NEW
    : "";

  const [form, setForm] = useState({
    entry_type: p.entry_type ?? "buy",
    instrument_id: defaultInstrumentId,
    account_id: item.suggested_account_id ? String(item.suggested_account_id) : "",
    quantity: p.quantity ?? "",
    price_native: p.price_native ?? "",
    fee_native: p.fee_native ?? "0",
    currency: p.currency ?? "USD",
    executed_at: (p.executed_at ?? new Date().toISOString()).slice(0, 16),
    // inline-create scratch fields
    new_symbol: p.symbol ?? "",
    new_account_name: p.account_hint ?? "",
  });
  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) => setForm({ ...form, [k]: e.target.value });

  const onConfirm = async () => {
    setActionError(null);
    try {
      let instrumentId = form.instrument_id ? Number(form.instrument_id) : 0;
      if (form.instrument_id === CREATE_NEW) {
        const created = await createInstrument.mutateAsync({
          symbol: form.new_symbol,
          name: p.instrument_name ?? form.new_symbol,
          instrument_type: "other",
          native_currency: form.currency,
          category_id: null,
          price_source: "manual",
          decimals: 8,
          note: null,
        });
        instrumentId = created.id;
      }
      let accountId = form.account_id ? Number(form.account_id) : 0;
      if (form.account_id === CREATE_NEW) {
        const created = await createAccount.mutateAsync({
          name: form.new_account_name || "Imported",
          account_type: "manual",
          institution: null,
          native_currency: form.currency,
          note: null,
        });
        accountId = created.id;
      }
      if (!instrumentId || !accountId) {
        setActionError("Select or create both an instrument and an account first.");
        return;
      }
      await confirm.mutateAsync({
        id: item.id,
        payload: {
          account_id: accountId,
          instrument_id: instrumentId,
          entry_type: form.entry_type,
          executed_at: new Date(form.executed_at).toISOString(),
          quantity: form.quantity,
          price_native: form.price_native,
          fee_native: form.fee_native,
          currency: form.currency,
        },
      });
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Confirm failed");
    }
  };

  const input = "w-full rounded border px-1 py-0.5 text-xs";
  return (
    <tr className="border-t align-top">
      <td className="p-1 text-xs">
        <span className="rounded bg-gray-100 px-1">{item.doc_type}</span>
        {item.needs_attention === 1 && <div className="mt-1 rounded bg-amber-100 px-1 text-amber-700">needs attention</div>}
        <div className="mt-1 text-gray-400">{item.source_filename}</div>
      </td>
      <td className="p-1">
        <select aria-label="Entry type" className={input} value={form.entry_type} onChange={set("entry_type")}>
          {ENTRY_TYPES.map((t) => <option key={t}>{t}</option>)}
        </select>
      </td>
      <td className="p-1">
        <select aria-label="Instrument" className={input} value={form.instrument_id} onChange={set("instrument_id")}>
          <option value="">Instrument…</option>
          {instruments.map((i) => <option key={i.id} value={i.id}>{i.symbol}</option>)}
          <option value={CREATE_NEW}>➕ create new…</option>
        </select>
        {form.instrument_id === CREATE_NEW && (
          <input
            aria-label="New instrument symbol"
            className={`${input} mt-1`}
            placeholder="symbol"
            value={form.new_symbol}
            onChange={set("new_symbol")}
          />
        )}
      </td>
      <td className="p-1">
        <select aria-label="Account" className={input} value={form.account_id} onChange={set("account_id")}>
          <option value="">Account…</option>
          {accounts.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
          <option value={CREATE_NEW}>➕ create new…</option>
        </select>
        {form.account_id === CREATE_NEW && (
          <input
            aria-label="New account name"
            className={`${input} mt-1`}
            placeholder="account name"
            value={form.new_account_name}
            onChange={set("new_account_name")}
          />
        )}
      </td>
      <td className="p-1"><input aria-label="Quantity" className={input} value={form.quantity} onChange={set("quantity")} /></td>
      <td className="p-1"><input aria-label="Price" className={input} value={form.price_native} onChange={set("price_native")} /></td>
      <td className="p-1"><input aria-label="Currency" className={input} value={form.currency} onChange={set("currency")} /></td>
      <td className="p-1"><input aria-label="Executed at" type="datetime-local" className={input} value={form.executed_at} onChange={set("executed_at")} /></td>
      <td className="p-1 whitespace-nowrap">
        <button
          type="button"
          onClick={onConfirm}
          disabled={confirm.isPending}
          className="rounded bg-green-600 px-2 py-0.5 text-xs text-white disabled:opacity-50"
        >
          confirm
        </button>
        <button
          type="button"
          onClick={() => reject.mutate(item.id)}
          disabled={reject.isPending}
          className="ml-1 rounded bg-gray-200 px-2 py-0.5 text-xs"
        >
          reject
        </button>
        {actionError && <div className="text-xs text-red-600">{actionError}</div>}
      </td>
    </tr>
  );
}
