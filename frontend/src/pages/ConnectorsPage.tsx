import { useState, useEffect } from "react";
import ReactDOM from "react-dom";
import { Plus, RefreshCw, Check, X, Coins, Landmark, Plug } from "lucide-react";
import {
  useConnectors,
  useCreateConnector,
  useDeleteConnector,
  useSyncConnector,
} from "../api/hooks";
import { QueryState } from "../components/QueryState";
import type { SyncReport } from "../api/schemas";

/* relative-time helper */
function relTime(iso: string | null): string {
  if (!iso) return "belum pernah";
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "baru saja";
  if (mins < 60) return `${mins} mnt lalu`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} jam lalu`;
  const days = Math.floor(hrs / 24);
  return `${days} hari lalu`;
}

const KIND_ICON: Record<string, React.ReactNode> = {
  exchange: <Coins size={20} />,
  bank: <Landmark size={20} />,
};

const STATUS_CFG: Record<string, { tone: string; label: string }> = {
  ok: { tone: "badge-gain", label: "Tersinkron" },
  stale: { tone: "badge-warn", label: "Perlu sync" },
  error: { tone: "badge-loss", label: "Error" },
};

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  sub?: string;
  children: React.ReactNode;
  footer: React.ReactNode;
}

function Dialog({ open, onClose, title, sub, children, footer }: DialogProps) {
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  return ReactDOM.createPortal(
    <div
      className="dialog-scrim"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
      role="presentation"
    >
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="conn-dialog-title">
        <div className="dialog-head">
          <div>
            <div className="t-h2" id="conn-dialog-title">{title}</div>
            {sub && <div className="card-sub" style={{ marginTop: 3 }}>{sub}</div>}
          </div>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Tutup dialog" style={{ width: 32, height: 32 }}>
            <X size={18} />
          </button>
        </div>
        <div className="dialog-body">{children}</div>
        <div className="dialog-foot">{footer}</div>
      </div>
    </div>,
    document.body,
  );
}

const EMPTY_FORM = { kind: "exchange", label: "", address: "", base_url: "", api_key: "" };

export default function ConnectorsPage() {
  const connectors = useConnectors();
  const createConnector = useCreateConnector();
  const deleteConnector = useDeleteConnector();
  const syncConnector = useSyncConnector();

  const [open, setOpen] = useState(false);
  const [form, setForm] = useState(EMPTY_FORM);
  const [syncResults, setSyncResults] = useState<Record<number, SyncReport>>({});
  const [syncing, setSyncing] = useState<number | null>(null);

  const set =
    (k: keyof typeof EMPTY_FORM) =>
    (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
      setForm({ ...form, [k]: e.target.value });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const config = JSON.stringify({
      address: form.address || undefined,
      base_url: form.base_url || undefined,
      api_key: form.api_key || undefined,
      native_symbol: "ETH",
    });
    createConnector.mutate(
      { account_id: 0, kind: form.kind, label: form.label, config_json: config },
      {
        onSuccess: () => {
          setForm(EMPTY_FORM);
          setOpen(false);
        },
      },
    );
  };

  const handleSync = (id: number) => {
    setSyncing(id);
    syncConnector.mutate(id, {
      onSuccess: (report) => {
        setSyncResults((prev) => ({ ...prev, [id]: report }));
        setSyncing(null);
      },
      onError: () => setSyncing(null),
    });
  };

  const conns = connectors.data ?? [];

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">Connectors</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Sinkronisasi on-chain &amp; exchange</div>
        </div>
        <button
          type="button"
          className="btn btn-primary btn-sm"
          onClick={() => setOpen(true)}
          aria-label="Tambah konektor"
        >
          <Plus size={15} />
          Tambah Konektor
        </button>
      </div>

      <QueryState isLoading={connectors.isLoading} error={connectors.error}>
        {conns.length === 0 ? (
          <div className="card">
            <div className="empty">
              <div className="empty-icon">
                <Plug size={26} />
              </div>
              <div>
                <div className="t-h3">No connectors yet.</div>
                <div className="t-sm t-muted" style={{ marginTop: 4 }}>
                  Add an EVM wallet or exchange connector.
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(300px, 1fr))", gap: 16 }}>
            {conns.map((c) => {
              const st = STATUS_CFG[c.last_synced_at ? "ok" : "stale"] ?? STATUS_CFG["stale"];
              const isSyncing = syncing === c.id;
              const report = syncResults[c.id];

              return (
                <div key={c.id} className="card card-pad">
                  <div className="flex items-center" style={{ gap: 12, marginBottom: 16 }}>
                    <span
                      style={{
                        width: 42,
                        height: 42,
                        borderRadius: 11,
                        display: "grid",
                        placeItems: "center",
                        background: "hsl(var(--muted))",
                        color: "hsl(var(--foreground))",
                        flexShrink: 0,
                      }}
                    >
                      {KIND_ICON[c.kind] ?? <Plug size={20} />}
                    </span>
                    <div className="flex-1" style={{ minWidth: 0 }}>
                      <div style={{ fontWeight: 600 }} className="truncate">{c.label}</div>
                      <div className="t-xs t-muted">disinkron {relTime(c.last_synced_at)}</div>
                    </div>
                    <span className={"badge " + st.tone}>
                      <span className="badge-dot" style={{ background: "currentColor" }} />
                      {st.label}
                    </span>
                  </div>

                  {report && (
                    <div className="t-xs t-muted num" style={{ marginBottom: 8 }}>
                      inserted: {report.inserted} · staged: {report.staged} · skipped: {report.skipped}
                    </div>
                  )}

                  <div className="flex items-center" style={{ gap: 8 }}>
                    <button
                      type="button"
                      className="btn btn-outline btn-sm"
                      style={{ flex: 1 }}
                      disabled={isSyncing}
                      onClick={() => handleSync(c.id)}
                      aria-label={`Sync ${c.label}`}
                    >
                      <RefreshCw size={14} className={isSyncing ? "animate-spin" : ""} />
                      {isSyncing ? "Menyinkron…" : "Sync sekarang"}
                    </button>
                    <button
                      type="button"
                      className="icon-btn"
                      style={{ width: 32, height: 32 }}
                      onClick={() => deleteConnector.mutate(c.id)}
                      aria-label={`Hapus konektor ${c.label}`}
                    >
                      <X size={15} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </QueryState>

      {/* Add Connector Dialog */}
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Tambah Konektor"
        sub="Hubungkan exchange atau wallet on-chain"
        footer={
          <>
            <button type="button" className="btn btn-outline" onClick={() => setOpen(false)}>
              Batal
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={handleSubmit as unknown as React.MouseEventHandler}
              disabled={createConnector.isPending}
              aria-label="Hubungkan konektor"
            >
              <Check size={16} />
              Hubungkan
            </button>
          </>
        }
      >
        <form id="conn-form" onSubmit={handleSubmit}>
          <label className="field">
            <span className="field-label">Jenis</span>
            <select
              className="select"
              value={form.kind}
              onChange={set("kind")}
              aria-label="Jenis konektor"
            >
              <option value="exchange">Exchange</option>
              <option value="evm_wallet">Wallet EVM</option>
              <option value="bank">Bank</option>
            </select>
          </label>
          <label className="field">
            <span className="field-label">Label</span>
            <input
              className="input"
              placeholder="mis. Pintu, Ledger ETH…"
              value={form.label}
              onChange={set("label")}
              aria-label="Connector label"
              required
            />
          </label>
          <label className="field">
            <span className="field-label">Alamat Wallet (opsional)</span>
            <input
              className="input"
              placeholder="0x…"
              value={form.address}
              onChange={set("address")}
              aria-label="Wallet address"
            />
          </label>
          <label className="field">
            <span className="field-label">API Key</span>
            <span className="t-xs t-muted">Disimpan terenkripsi, hanya akses baca.</span>
            <input
              type="password"
              className="input"
              placeholder="••••••••••••"
              value={form.api_key}
              onChange={set("api_key")}
              aria-label="API key"
            />
          </label>
          {createConnector.error && (
            <div className="t-sm loss">{(createConnector.error as Error).message}</div>
          )}
        </form>
      </Dialog>
    </div>
  );
}
