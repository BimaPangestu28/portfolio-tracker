import { useState } from "react";
import { Download } from "lucide-react";
import { useInvoices, useClients, useInvoice } from "../api/hooks";
import { InvoiceLineItemSchema } from "../api/schemas";
import { api } from "../api/client";
import { formatIDR } from "../lib/format";

function parseLineItems(json: string) {
  try {
    return InvoiceLineItemSchema.array().parse(JSON.parse(json));
  } catch {
    return [];
  }
}

export default function InvoicesPage() {
  const invoices = useInvoices();
  const clients = useClients();
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = useInvoice(selectedId);
  const [downloading, setDownloading] = useState(false);

  const clientName = (id: number) => clients.data?.find((c) => c.id === id)?.name ?? "—";

  async function handleDownload(id: number, number: string) {
    setDownloading(true);
    try {
      const blob = await api.getBlob(`/invoices/${id}/pdf`);
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${number.replace(/\//g, "-")}.pdf`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } finally {
      setDownloading(false);
    }
  }

  const rows = invoices.data ?? [];
  const detail = selectedId === null ? undefined : selected.data;
  const items = detail ? parseLineItems(detail.line_items_json) : [];

  return (
    <div className="flex col gap-5">
      <div>
        <h1 className="t-h1">Invoice</h1>
        <div className="t-sm t-muted">Daftar invoice & unduh PDF</div>
      </div>

      <div className="grid gap-5 lay-2-13">
        <div className="card">
          <div className="card-head"><div className="card-title">Semua invoice</div></div>
          <div className="card-pad flex col gap-1" style={{ paddingTop: 12 }}>
            {rows.length === 0 && <p className="text-sm text-muted-foreground">Belum ada invoice.</p>}
            {rows.map((inv) => (
              <button
                key={inv.id}
                className="flex items-center gap-2 text-sm"
                style={{ justifyContent: "space-between", padding: "8px 6px", borderRadius: 8, textAlign: "left", background: selectedId === inv.id ? "hsl(var(--muted))" : "transparent" }}
                onClick={() => setSelectedId(inv.id)}
              >
                <span className="flex-1 truncate">{inv.number}</span>
                <span className="text-muted-foreground truncate" style={{ maxWidth: 120 }}>{clientName(inv.client_id)}</span>
                <span className="num">{inv.total}</span>
              </button>
            ))}
          </div>
        </div>

        <div className="card">
          <div className="card-head"><div className="card-title">Detail</div></div>
          <div className="card-pad flex col gap-2" style={{ paddingTop: 12 }}>
            {!detail && <p className="text-sm text-muted-foreground">Pilih invoice.</p>}
            {detail && (
              <>
                <div className="flex items-center justify-between">
                  <div>
                    <div style={{ fontWeight: 600 }}>{detail.number}</div>
                    <div className="t-xs t-muted">{clientName(detail.client_id)}</div>
                  </div>
                  <button className="btn btn-primary" disabled={downloading} onClick={() => handleDownload(detail.id, detail.number)}>
                    <Download size={15} /> Download PDF
                  </button>
                </div>
                <div className="t-xs t-muted">Terbit {detail.issue_date} · Jatuh tempo {detail.due_date}</div>
                <div className="flex col gap-1" style={{ marginTop: 8 }}>
                  {items.map((it, idx) => (
                    <div key={idx} className="flex items-center gap-2 text-sm">
                      <span className="flex-1 truncate">{it.title}</span>
                      <span className="text-muted-foreground">×{it.qty}</span>
                      <span className="num">{formatIDR(it.amount)}</span>
                    </div>
                  ))}
                </div>
                <div className="flex items-center justify-between text-sm" style={{ borderTop: "1px solid hsl(var(--border))", paddingTop: 8, marginTop: 4, fontWeight: 600 }}>
                  <span>Total</span>
                  <span className="num">{detail.total}</span>
                </div>
              </>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
