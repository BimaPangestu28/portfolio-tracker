import { useEffect, useState } from "react";
import { api } from "../api/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

/**
 * UpworkCard — Settings card for connecting/disconnecting Upwork.
 *
 * Fetches current OAuth status on mount and exposes connect (OAuth redirect),
 * sync (POST), and disconnect (POST) actions. Renders inside the SettingsPage
 * integrations tab alongside GoogleCalendarCard.
 */

type UpworkSyncStatus = "connected" | "disconnected" | "error" | "loading";

export default function UpworkCard() {
  const [status, setStatus] = useState<UpworkSyncStatus>("loading");
  const [lastError, setLastError] = useState<string | null>(null);
  const [syncMessage, setSyncMessage] = useState<string | null>(null);

  async function fetchStatus() {
    try {
      const response = await api.upworkStatus();
      setStatus(response.status);
      setLastError(response.last_error);
    } catch {
      // Surface the failure rather than silently showing a disconnected state —
      // a connected user hitting a transient error should not be nudged to re-auth.
      setStatus("error");
      setLastError("gagal memuat status");
    }
  }

  useEffect(() => {
    fetchStatus();
  }, []);

  async function handleConnect() {
    try {
      const { consent_url } = await api.upworkStart();
      window.location.href = consent_url;
    } catch {
      setStatus("error");
      setLastError("gagal memulai koneksi");
    }
  }

  async function handleSync() {
    try {
      setSyncMessage(null);
      const { inserted } = await api.upworkSync();
      setSyncMessage(`Imported ${inserted} earnings`);
    } catch {
      setStatus("error");
      setLastError("gagal sinkronisasi");
    }
  }

  async function handleDisconnect() {
    try {
      setSyncMessage(null);
      await api.upworkDisconnect();
      await fetchStatus();
    } catch {
      setStatus("error");
      setLastError("gagal memutuskan koneksi");
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Upwork</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <p className="text-sm text-muted-foreground">
          Earnings → income. Sinkronkan pemasukan freelance dari Upwork secara otomatis.
        </p>

        {status === "loading" && (
          <p className="text-sm text-muted-foreground">Memuat…</p>
        )}

        {status === "connected" && (
          <p className="text-sm text-green-600">Terhubung ✓</p>
        )}

        {status === "error" && (
          <p className="text-sm text-red-600">
            Bermasalah{lastError ? `: ${lastError}` : ""} — hubungkan ulang.
          </p>
        )}

        {syncMessage && (
          <p className="text-sm text-blue-600">{syncMessage}</p>
        )}

        <div className="flex gap-2">
          {(status === "disconnected" || status === "error") && (
            <Button variant="outline" size="sm" onClick={handleConnect}>
              Hubungkan Upwork
            </Button>
          )}

          {status === "connected" && (
            <Button variant="outline" size="sm" onClick={handleSync}>
              Sync now
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
