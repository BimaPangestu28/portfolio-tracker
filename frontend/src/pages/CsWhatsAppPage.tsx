import { QRCodeSVG } from "qrcode.react";
import { toast } from "sonner";
import { useCsWhatsappStatus, useConnectCsWhatsapp, useDisconnectCsWhatsapp } from "../api/hooks";

/**
 * CS WhatsApp connection control. Polls the backend for live status and renders
 * the pairing QR / connect / disconnect controls accordingly.
 * Mirrors WhatsAppPage.tsx but targets the CS WhatsApp gateway (/cs/whatsapp/*).
 */
export default function CsWhatsAppPage() {
  const statusQuery = useCsWhatsappStatus();
  const connect = useConnectCsWhatsapp();
  const disconnect = useDisconnectCsWhatsapp();

  const state = statusQuery.data?.status ?? "disconnected";
  const number = statusQuery.data?.number;
  const qr = statusQuery.data?.qr;

  const handleConnect = () =>
    connect.mutate(undefined, {
      onSuccess: () => toast.success("Memulai koneksi CS WhatsApp…"),
      onError: (err) => toast.error((err as Error).message),
    });

  const handleDisconnect = () =>
    disconnect.mutate(undefined, {
      onSuccess: () => toast.success("CS WhatsApp diputuskan"),
      onError: (err) => toast.error((err as Error).message),
    });

  return (
    <div>
      <h1 className="t-h1">CS WhatsApp</h1>
      <div className="t-sm t-muted" style={{ marginBottom: 12 }}>Hubungkan gateway CS WhatsApp</div>

      <div className="card" style={{ padding: 22, maxWidth: 420 }}>
        {state === "connected" && (
          <div className="col gap-3">
            <p className="t-sm">
              Terhubung sebagai <strong>{number ?? "-"}</strong>
            </p>
            <button
              type="button"
              className="btn btn-danger"
              disabled={disconnect.isPending}
              onClick={handleDisconnect}
            >
              Putuskan
            </button>
          </div>
        )}

        {state === "qr" && qr && (
          <div style={{ textAlign: "center" }}>
            <QRCodeSVG value={qr} size={240} style={{ width: "100%", height: "auto", maxWidth: 240 }} />
            <p className="t-sm t-muted" style={{ marginTop: 12 }}>
              Buka WhatsApp → Perangkat Tertaut → Tautkan Perangkat, lalu scan kode ini.
            </p>
          </div>
        )}

        {state === "connecting" && <p className="t-sm t-muted">Menyambungkan…</p>}

        {state === "disconnected" && (
          <button
            type="button"
            className="btn btn-primary"
            disabled={connect.isPending}
            onClick={handleConnect}
          >
            Hubungkan CS WhatsApp
          </button>
        )}
      </div>
    </div>
  );
}
