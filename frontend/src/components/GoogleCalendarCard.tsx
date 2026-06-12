import { useEffect, useState } from "react";
import { api } from "../api/client";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";

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

  async function fetchStatus() {
    try {
      const response = await api.googleStatus();
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
          <p className="text-sm text-green-600">Terhubung ✓</p>
        )}

        {status === "error" && (
          <p className="text-sm text-red-600">
            Bermasalah{lastError ? `: ${lastError}` : ""} — hubungkan ulang.
          </p>
        )}

        <div className="flex gap-2">
          {(status === "disconnected" || status === "error") && (
            <Button variant="outline" size="sm" onClick={handleConnect}>
              Hubungkan Google
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
