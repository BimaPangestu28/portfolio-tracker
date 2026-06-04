import { useState } from "react";
import { useConfirmReview, useRejectReview, useCreateInstrument, useCreateAccount } from "../api/hooks";
import { ExtractedEntrySchema, type ReviewItem, type Instrument, type Account } from "../api/schemas";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { TableCell, TableRow } from "@/components/ui/table";

const ENTRY_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];
const CREATE_NEW = "__new__";

const nativeSelect =
  "flex h-7 w-full rounded-md border border-input bg-transparent px-2 py-0.5 text-xs shadow-sm focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring";

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
        // Dedup: if an instrument with this symbol already exists (e.g. created
        // when a prior review row for the same symbol was confirmed), reuse it
        // instead of creating a duplicate. Match is case-insensitive, mirroring
        // the backend's suggest_instrument / find_or_create.
        const symbol = form.new_symbol.trim();
        const existing = instruments.find(
          (i) => i.symbol.toLowerCase() === symbol.toLowerCase(),
        );
        if (existing) {
          instrumentId = existing.id;
        } else {
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

  return (
    <TableRow className="align-top">
      <TableCell className="p-1 text-xs">
        <Badge variant="secondary" className="px-1">{item.doc_type}</Badge>
        {item.needs_attention === 1 && (
          <div className="mt-1 rounded bg-amber-100 px-1 text-xs text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">needs attention</div>
        )}
        <div className="mt-1 text-muted-foreground">{item.source_filename}</div>
      </TableCell>
      <TableCell className="p-1">
        <select aria-label="Entry type" className={nativeSelect} value={form.entry_type} onChange={set("entry_type")}>
          {ENTRY_TYPES.map((t) => <option key={t}>{t}</option>)}
        </select>
      </TableCell>
      <TableCell className="p-1">
        <select aria-label="Instrument" className={nativeSelect} value={form.instrument_id} onChange={set("instrument_id")}>
          <option value="">Instrument…</option>
          {instruments.map((i) => <option key={i.id} value={i.id}>{i.symbol}</option>)}
          <option value={CREATE_NEW}>➕ create new…</option>
        </select>
        {form.instrument_id === CREATE_NEW && (
          <Input
            aria-label="New instrument symbol"
            className="mt-1 h-7 text-xs"
            placeholder="symbol"
            value={form.new_symbol}
            onChange={set("new_symbol")}
          />
        )}
      </TableCell>
      <TableCell className="p-1">
        <select aria-label="Account" className={nativeSelect} value={form.account_id} onChange={set("account_id")}>
          <option value="">Account…</option>
          {accounts.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
          <option value={CREATE_NEW}>➕ create new…</option>
        </select>
        {form.account_id === CREATE_NEW && (
          <Input
            aria-label="New account name"
            className="mt-1 h-7 text-xs"
            placeholder="account name"
            value={form.new_account_name}
            onChange={set("new_account_name")}
          />
        )}
      </TableCell>
      <TableCell className="p-1">
        <Input aria-label="Quantity" className="h-7 text-xs" value={form.quantity} onChange={set("quantity")} />
      </TableCell>
      <TableCell className="p-1">
        <Input aria-label="Price" className="h-7 text-xs" value={form.price_native} onChange={set("price_native")} />
      </TableCell>
      <TableCell className="p-1">
        <Input aria-label="Currency" className="h-7 text-xs" value={form.currency} onChange={set("currency")} />
      </TableCell>
      <TableCell className="p-1">
        <Input aria-label="Executed at" type="datetime-local" className="h-7 text-xs" value={form.executed_at} onChange={set("executed_at")} />
      </TableCell>
      <TableCell className="p-1 whitespace-nowrap">
        <Button
          type="button"
          size="sm"
          onClick={onConfirm}
          disabled={confirm.isPending}
          className="h-6 px-2 text-xs"
        >
          confirm
        </Button>
        <Button
          type="button"
          variant="secondary"
          size="sm"
          onClick={() => reject.mutate(item.id)}
          disabled={reject.isPending}
          className="ml-1 h-6 px-2 text-xs"
        >
          reject
        </Button>
        {actionError && <div className="text-xs text-destructive">{actionError}</div>}
      </TableCell>
    </TableRow>
  );
}
