import type { WidgetConfig } from "./config";

export interface Lead { name: string; email: string; phone: string; }
export interface HistoryMessage { role: string; content: string; created_at: string; }

export class CsApi {
  constructor(private cfg: WidgetConfig) {}

  private async post<T>(path: string, body: unknown): Promise<T> {
    let res: Response;
    try {
      res = await fetch(`${this.cfg.apiBase}${path}`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
    } catch {
      throw new Error("Tidak dapat terhubung ke server. Periksa koneksi internet kamu.");
    }
    if (!res.ok) {
      let msg = `HTTP ${res.status}`;
      try { const b = await res.json(); if (b?.error) msg = b.error; } catch { /* keep default */ }
      throw new Error(msg);
    }
    return (await res.json()) as T;
  }

  async startSession(lead: Lead): Promise<string> {
    const r = await this.post<{ session_token: string }>("/public/cs/session", {
      site_key: this.cfg.siteKey,
      name: lead.name,
      email: lead.email || undefined,
      phone: lead.phone || undefined,
    });
    return r.session_token;
  }

  async sendMessage(token: string, message: string): Promise<string> {
    const r = await this.post<{ reply: string }>("/public/cs/message", {
      site_key: this.cfg.siteKey,
      session_token: token,
      message,
    });
    return r.reply;
  }

  async history(token: string): Promise<HistoryMessage[]> {
    return this.post<HistoryMessage[]>("/public/cs/history", {
      site_key: this.cfg.siteKey,
      session_token: token,
    });
  }
}
