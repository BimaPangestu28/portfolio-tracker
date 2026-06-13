import { useEffect, useState } from "react";
import { toast } from "sonner";
import { api } from "../api/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

function formatSyncedAt(ts: string | null): string {
  if (!ts) return "belum pernah";
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleString("id-ID", { dateStyle: "medium", timeStyle: "short" });
}

/**
 * GoogleCalendarCard — Settings card for connecting/disconnecting Google Calendar.
 *
 * Fetches current OAuth status on mount and exposes connect (OAuth redirect)
 * and disconnect (POST) actions. Renders inside the SettingsPage integration tab.
 */

type GoogleSyncStatus = "connected" | "disconnected" | "error" | "loading";

export default function GoogleCalendarCard() {
  const [status, setStatus] = useState<GoogleSyncStatus>("loading");
  const [lastError, setLastError] = useState<string | null>(null);
  const [lastSyncedAt, setLastSyncedAt] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);

  async function fetchStatus() {
    try {
      const response = await api.googleStatus();
      setStatus(response.status);
      setLastError(response.last_error);
      setLastSyncedAt(response.last_synced_at);
    } catch {
      // Surface the failure rather than silently showing a disconnected state —
      // a connected user hitting a transient error should not be nudged to re-auth.
      setStatus("error");
      setLastError("gagal memuat status");
    }
  }

  async function handleSyncNow() {
    setSyncing(true);
    try {
      const r = await api.googleSync();
      setStatus(r.status);
      setLastError(r.last_error);
      setLastSyncedAt(r.last_synced_at);
      if (r.status === "error") {
        toast.error(`Sync gagal: ${r.last_error ?? "tidak diketahui"}`);
      } else {
        toast.success(`Sync selesai — ${r.pushed} dikirim, ${r.imported} diimpor`);
      }
    } catch (err) {
      toast.error((err as Error).message);
    } finally {
      setSyncing(false);
    }
  }

  useEffect(() => {
    fetchStatus();
  }, []);

  async function handleConnect() {
    try {
      const { consent_url } = await api.googleStart();
      window.location.href = consent_url;
    } catch {
      setStatus("error");
      setLastError("gagal memulai koneksi");
    }
  }

  async function handleDisconnect() {
    try {
      await api.googleDisconnect();
      await fetchStatus();
    } catch {
      setStatus("error");
      setLastError("gagal memutuskan koneksi");
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Google Calendar</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          Sinkronkan agenda asisten dua arah dengan Google Calendar utamamu.
        </p>

        {status === "loading" && (
          <p className="text-sm text-muted-foreground">Memuat…</p>
        )}

        {status === "connected" && (
          <div className="text-sm">
            <p className="text-green-600">Terhubung ✓</p>
            <p className="text-muted-foreground">Terakhir sync: {formatSyncedAt(lastSyncedAt)}</p>
          </div>
        )}

        {status === "error" && (
          <p className="text-sm text-red-600">
            Bermasalah{lastError ? `: ${lastError}` : ""} — hubungkan ulang.
          </p>
        )}

        <div className="flex flex-wrap gap-2">
          {(status === "disconnected" || status === "error") && (
            <Button variant="outline" size="sm" onClick={handleConnect}>
              Hubungkan Google
            </Button>
          )}

          {status === "connected" && (
            <Button variant="outline" size="sm" onClick={handleSyncNow} disabled={syncing}>
              {syncing ? "Menyinkronkan…" : "Sync sekarang"}
            </Button>
          )}

          {(status === "connected" || status === "error") && (
            <Button variant="outline" size="sm" onClick={handleDisconnect}>
              Putuskan
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}
