import { QRCodeSVG } from "qrcode.react";
import { toast } from "sonner";
import { useWhatsappStatus, useConnectWhatsapp, useDisconnectWhatsapp } from "../api/hooks";

/**
 * WhatsApp connection control. Polls the backend for live status and renders
 * the pairing QR / connect / disconnect controls accordingly.
 */
export default function WhatsAppPage() {
  const statusQuery = useWhatsappStatus();
  const connect = useConnectWhatsapp();
  const disconnect = useDisconnectWhatsapp();

  const state = statusQuery.data?.status ?? "disconnected";
  const number = statusQuery.data?.number;
  const qr = statusQuery.data?.qr;

  const handleConnect = () =>
    connect.mutate(undefined, {
      onSuccess: () => toast.success("Memulai koneksi WhatsApp…"),
      onError: (err) => toast.error((err as Error).message),
    });

  const handleDisconnect = () =>
    disconnect.mutate(undefined, {
      onSuccess: () => toast.success("WhatsApp diputuskan"),
      onError: (err) => toast.error((err as Error).message),
    });

  return (
    <div>
      <h1 className="t-h1">WhatsApp</h1>
      <div className="t-sm t-muted" style={{ marginBottom: 12 }}>Hubungkan bot WhatsApp</div>

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
            <QRCodeSVG value={qr} size={240} />
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
            Hubungkan WhatsApp
          </button>
        )}
      </div>
    </div>
  );
}
