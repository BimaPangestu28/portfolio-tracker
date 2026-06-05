import { useState } from "react";
import { toast } from "sonner";
import { useTelegramStatus, useTelegramLinkCode, useUnlinkTelegram } from "../api/hooks";

/**
 * Telegram linking control. The bot token lives in the backend env; this page
 * only drives the one-time link-code handshake and shows the current status.
 */
export default function TelegramPage() {
  const statusQuery = useTelegramStatus();
  const linkCode = useTelegramLinkCode();
  const unlink = useUnlinkTelegram();
  const [code, setCode] = useState<string | null>(null);

  const configured = statusQuery.data?.configured ?? true;
  const linked = statusQuery.data?.linked ?? false;
  const username = statusQuery.data?.username;

  const handleGenerate = () =>
    linkCode.mutate(undefined, {
      onSuccess: (out) => setCode(out.code),
      onError: (err) => toast.error((err as Error).message),
    });

  const handleUnlink = () =>
    unlink.mutate(undefined, {
      onSuccess: () => {
        setCode(null);
        toast.success("Tautan Telegram diputus");
      },
      onError: (err) => toast.error((err as Error).message),
    });

  return (
    <div>
      <h1 className="t-h1">Telegram</h1>
      <div className="t-sm t-muted" style={{ marginBottom: 12 }}>Hubungkan bot Telegram</div>

      <div className="card" style={{ padding: 22, maxWidth: 420 }}>
        {!configured && (
          <p className="t-sm t-muted">
            Bot Telegram belum dikonfigurasi. Buat bot lewat @BotFather, lalu set
            env <code>TELEGRAM_BOT_TOKEN</code> di backend dan restart.
          </p>
        )}

        {configured && linked && (
          <div className="col gap-3">
            <p className="t-sm">
              Tertaut sebagai <strong>@{username ?? "(tanpa username)"}</strong>
            </p>
            <button
              type="button"
              className="btn btn-danger"
              disabled={unlink.isPending}
              onClick={handleUnlink}
            >
              Putus Tautan
            </button>
          </div>
        )}

        {configured && !linked && code && (
          <div style={{ textAlign: "center" }}>
            <div style={{ fontSize: 36, fontWeight: 700, letterSpacing: 6 }}>{code}</div>
            <p className="t-sm t-muted" style={{ marginTop: 12 }}>
              Kirim kode ini sebagai pesan ke bot Telegram kamu. Kode berlaku 10 menit.
              Halaman ini akan terbarui otomatis setelah tertaut.
            </p>
          </div>
        )}

        {configured && !linked && !code && (
          <button
            type="button"
            className="btn btn-primary"
            disabled={linkCode.isPending}
            onClick={handleGenerate}
          >
            Buat Kode Tautan
          </button>
        )}
      </div>
    </div>
  );
}
