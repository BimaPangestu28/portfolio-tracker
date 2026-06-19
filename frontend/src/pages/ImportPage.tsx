import { useState } from "react";
import { toast } from "sonner";
import {
  useReviewItems,
  useIngest,
  useInstruments,
  useAccounts,
  useConfirmReview,
  useRejectReview,
  useCreateInstrument,
  useCreateAccount,
} from "../api/hooks";
import { readFileAsUpload, ACCEPTED_TYPES, type UploadFileIn } from "../lib/upload";
import { QueryState } from "../components/QueryState";
import { CsvImport } from "../components/CsvImport";
import { ExtractedEntrySchema, type ReviewItem, type Instrument, type Account } from "../api/schemas";
import { Upload, FileText, Image as ImageIcon, CheckCircle, AlertTriangle, Check, X, Info } from "lucide-react";
import { NumberInput } from "@/components/ui/NumberInput";

const DOC_ICON: Record<string, React.ReactNode> = {
  screenshot: <ImageIcon size={18} />,
  pdf: <FileText size={18} />,
  csv: <FileText size={18} />,
};
const DOC_LABEL: Record<string, string> = {
  screenshot: "Screenshot",
  pdf: "PDF",
  csv: "CSV",
};

const ENTRY_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];
const CREATE_NEW = "__new__";

function parsePayload(json: string) {
  try {
    return ExtractedEntrySchema.partial().parse(JSON.parse(json));
  } catch {
    return {};
  }
}

function ReviewCard({
  item,
  instruments,
  accounts,
}: {
  item: ReviewItem;
  instruments: Instrument[];
  accounts: Account[];
}) {
  const p = parsePayload(item.payload_json);
  const confirm = useConfirmReview();
  const reject = useRejectReview();
  const createInstrument = useCreateInstrument();
  const createAccount = useCreateAccount();
  const [actionError, setActionError] = useState<string | null>(null);

  // Pre-select inline-create when something identifiable was extracted but no
  // instrument matched: a symbol (stocks) or a bare name (mutual funds).
  const defaultInstrumentId = item.suggested_instrument_id
    ? String(item.suggested_instrument_id)
    : p.symbol || p.instrument_name
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
    amount_native: p.amount_native ?? "",
    new_symbol: p.symbol ?? p.instrument_name ?? "",
    new_account_name: p.account_hint ?? "",
  });

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm({ ...form, [k]: e.target.value });

  const needs = item.needs_attention === 1;
  // Bibit-style mutual fund entries: an IDR amount, no units/NAV. The backend
  // records these as quantity = amount at price 1 (value-based tracking).
  const isAmountOnly = !form.quantity && !form.price_native && !!form.amount_native;

  const onConfirm = async () => {
    setActionError(null);
    try {
      let instrumentId = form.instrument_id ? Number(form.instrument_id) : 0;
      if (form.instrument_id === CREATE_NEW) {
        // Dedup: reuse an existing instrument with the same symbol (case-insensitive)
        // instead of creating a duplicate — e.g. two ASII rows from one screenshot.
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
        setActionError("Pilih atau buat instrumen dan akun terlebih dahulu.");
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
          amount_native: form.amount_native,
        },
      });
      toast.success("Transaksi dikonfirmasi");
    } catch (err) {
      setActionError(err instanceof Error ? err.message : "Konfirmasi gagal");
      toast.error(err instanceof Error ? err.message : "Konfirmasi gagal");
    }
  };

  return (
    <div className="card card-pad" style={needs ? { borderColor: "hsl(var(--warn) / 0.4)" } : undefined}>
      {/* Card header: source info + badges */}
      <div className="flex items-center" style={{ gap: 12, marginBottom: 14 }}>
        <span
          style={{
            width: 38,
            height: 38,
            borderRadius: 9,
            display: "grid",
            placeItems: "center",
            background: "hsl(var(--muted))",
            color: "hsl(var(--muted-foreground))",
            flexShrink: 0,
          }}
        >
          {DOC_ICON[item.doc_type] ?? <FileText size={18} />}
        </span>
        <div className="flex-1" style={{ minWidth: 0 }}>
          <div className="flex items-center" style={{ gap: 8 }}>
            <span style={{ fontWeight: 600, fontFamily: "monospace" }} className="truncate t-sm">
              {item.source_filename}
            </span>
            <span className="badge badge-neutral">{DOC_LABEL[item.doc_type] ?? item.doc_type}</span>
          </div>
          <div className="t-xs t-muted">keyakinan ekstraksi {Math.round(item.payload_json ? 90 : 70)}%</div>
        </div>
        {needs ? (
          <span className="badge badge-warn">
            <AlertTriangle size={11} />
            perlu perhatian
          </span>
        ) : (
          <span className="badge badge-gain">
            <Check size={11} />
            siap
          </span>
        )}
      </div>

      {/* Editable fields grid */}
      <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: 12 }}>
        <label className="field">
          <span className="field-label">Tipe</span>
          <select className="select" value={form.entry_type} onChange={set("entry_type")} aria-label="Entry type">
            {ENTRY_TYPES.map((t) => <option key={t} value={t}>{t}</option>)}
          </select>
        </label>
        <label className="field">
          <span className="field-label">Instrumen</span>
          <select className="select" value={form.instrument_id} onChange={set("instrument_id")} aria-label="Instrument">
            <option value="">— pilih —</option>
            {instruments.map((i) => <option key={i.id} value={i.id}>{i.symbol}</option>)}
            <option value={CREATE_NEW}>+ buat baru…</option>
          </select>
          {form.instrument_id === CREATE_NEW && (
            <input
              className="input"
              placeholder="symbol"
              value={form.new_symbol}
              onChange={set("new_symbol")}
              aria-label="New instrument symbol"
              style={{ marginTop: 6 }}
            />
          )}
        </label>
        <label className="field">
          <span className="field-label">Jumlah</span>
          <NumberInput className="input" value={form.quantity} onChange={v => setForm({ ...form, quantity: v })} aria-label="Quantity" placeholder={isAmountOnly ? "amount-only" : undefined} />
        </label>
        <label className="field">
          <span className="field-label">Harga</span>
          <NumberInput className="input" value={form.price_native} onChange={v => setForm({ ...form, price_native: v })} aria-label="Price" placeholder={isAmountOnly ? "amount-only" : undefined} />
        </label>
        <label className="field">
          <span className="field-label">Nominal</span>
          <NumberInput className="input" value={form.amount_native} onChange={v => setForm({ ...form, amount_native: v })} aria-label="Amount" />
          {isAmountOnly && (
            <span className="t-xs t-muted">amount-only — unit dihitung otomatis dari NAV (tanpa NAV: nominal di harga 1)</span>
          )}
        </label>
        <label className="field">
          <span className="field-label">Akun</span>
          <select
            className="select"
            value={form.account_id}
            onChange={set("account_id")}
            aria-label="Account"
            style={!form.account_id ? { borderColor: "hsl(var(--warn))" } : undefined}
          >
            <option value="">— pilih akun —</option>
            {accounts.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}
            <option value={CREATE_NEW}>+ buat baru…</option>
          </select>
          {form.account_id === CREATE_NEW && (
            <input
              className="input"
              placeholder="nama akun"
              value={form.new_account_name}
              onChange={set("new_account_name")}
              aria-label="New account name"
              style={{ marginTop: 6 }}
            />
          )}
        </label>
        <label className="field">
          <span className="field-label">Tanggal</span>
          <input
            type="datetime-local"
            className="input"
            value={form.executed_at}
            onChange={set("executed_at")}
            aria-label="Executed at"
          />
        </label>
      </div>

      {actionError && (
        <div className="t-sm loss" style={{ marginTop: 10 }}>{actionError}</div>
      )}

      <div className="flex items-center justify-end" style={{ gap: 8, marginTop: 14 }}>
        <button
          type="button"
          className="btn btn-ghost btn-sm"
          onClick={() =>
            reject.mutate(item.id, {
              onSuccess: () => toast.success("Item ditolak"),
              onError: (err) => toast.error((err as Error).message),
            })
          }
          disabled={reject.isPending}
          aria-label="Tolak item ini"
        >
          <X size={14} />
          Tolak
        </button>
        <button
          type="button"
          className="btn btn-primary btn-sm"
          onClick={onConfirm}
          disabled={confirm.isPending}
          aria-label="Konfirmasi item ini"
        >
          <Check size={14} />
          Konfirmasi
        </button>
      </div>
    </div>
  );
}

export default function ImportPage() {
  const review = useReviewItems("pending");
  const ingest = useIngest();
  const instruments = useInstruments();
  const accounts = useAccounts();
  const [busy, setBusy] = useState(false);
  const [drag, setDrag] = useState(false);

  const onFiles = async (fileList: FileList | null) => {
    if (!fileList || fileList.length === 0) return;
    setBusy(true);
    try {
      const uploads: UploadFileIn[] = [];
      for (const f of Array.from(fileList)) uploads.push(await readFileAsUpload(f));
      const result = await ingest.mutateAsync(uploads);
      toast.success(`${result.items.length} item diantrekan untuk review`);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : "Upload gagal");
    } finally {
      setBusy(false);
    }
  };

  const items = review.data ?? [];
  const needsCount = items.filter((i) => i.needs_attention === 1).length;

  return (
    <div>
      {/* Header */}
      <div style={{ marginBottom: 18 }}>
        <h1 className="t-h1">Import</h1>
        <div className="t-sm t-muted" style={{ marginTop: 2 }}>Antrian review — tinjau sebelum masuk portofolio</div>
      </div>

      {/* Info banner */}
      <div
        className="card card-pad flex items-center"
        style={{ marginBottom: 18, gap: 12, background: "hsl(var(--accent-soft))", borderColor: "hsl(var(--primary) / 0.25)" }}
      >
        <Info size={18} style={{ color: "hsl(var(--primary))", flexShrink: 0 }} />
        <span className="t-sm" style={{ color: "hsl(var(--primary))" }}>
          Tidak ada yang otomatis tercatat. Setiap item harus kamu{" "}
          <strong>Konfirmasi</strong> atau <strong>Tolak</strong> dulu.
        </span>
      </div>

      {/* Upload + CSV panel */}
      <div className="lay-2" style={{ gap: 16, marginBottom: 22 }}>
        {/* Dropzone */}
        <div
          className="card"
          style={{
            borderStyle: "dashed",
            borderColor: drag ? "hsl(var(--primary))" : "hsl(var(--border-strong))",
            background: drag ? "hsl(var(--accent-soft))" : "hsl(var(--card))",
            transition: "all 0.15s",
          }}
          onDragOver={(e) => { e.preventDefault(); setDrag(true); }}
          onDragLeave={() => setDrag(false)}
          onDrop={(e) => { e.preventDefault(); setDrag(false); onFiles(e.dataTransfer.files); }}
        >
          <div className="empty" style={{ padding: "34px 24px" }}>
            <div className="empty-icon" style={{ background: "hsl(var(--accent-soft))", color: "hsl(var(--primary))" }}>
              <Upload size={26} />
            </div>
            <div>
              <div className="t-h3">Tarik screenshot / PDF ke sini</div>
              <div className="t-sm t-muted" style={{ marginTop: 4 }}>Bukti transaksi diekstrak otomatis dengan LLM</div>
            </div>
            <label style={{ cursor: "pointer" }}>
              <span className="btn btn-outline btn-sm">
                <ImageIcon size={15} />
                Pilih berkas
              </span>
              <input
                type="file"
                multiple
                accept={ACCEPTED_TYPES.join(",")}
                style={{ display: "none" }}
                disabled={busy || ingest.isPending}
                onChange={(e) => onFiles(e.target.files)}
                aria-label="Pilih berkas screenshot atau PDF"
              />
            </label>
            {(busy || ingest.isPending) && (
              <div className="t-sm t-muted">Mengekstrak…</div>
            )}
            {ingest.error && (
              <div className="t-sm loss">{(ingest.error as Error).message}</div>
            )}
          </div>
        </div>
        {/* CSV panel */}
        <CsvImport />
      </div>

      {/* Review queue header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 12 }}>
        <h2 className="t-h2">Antrian Review</h2>
        <span className={"badge " + (needsCount ? "badge-warn" : "badge-neutral")}>
          {needsCount} perlu perhatian · {items.length} total
        </span>
      </div>

      <QueryState isLoading={review.isLoading} error={review.error}>
        {items.length === 0 ? (
          <div className="card">
            <div className="empty">
              <div className="empty-icon">
                <CheckCircle size={26} />
              </div>
              <div>
                <div className="t-h3">No pending items.</div>
                <div className="t-sm t-muted" style={{ marginTop: 4 }}>
                  Upload a document to extract transactions.
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            {items.map((it) => (
              <ReviewCard
                key={it.id}
                item={it}
                instruments={instruments.data ?? []}
                accounts={accounts.data ?? []}
              />
            ))}
          </div>
        )}
      </QueryState>
    </div>
  );
}
