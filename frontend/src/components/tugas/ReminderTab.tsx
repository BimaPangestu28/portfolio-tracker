import { useState } from "react";
import { X } from "lucide-react";
import { toast } from "sonner";
import { useReminders, useCancelReminder, useCreateReminder } from "../../api/hooks";
import { wibDayKey, formatWibTime } from "../../lib/wib";

const STATUSES: { key: string; label: string }[] = [
  { key: "pending", label: "Aktif" },
  { key: "sent", label: "Terkirim" },
  { key: "cancelled", label: "Dibatalkan" },
  { key: "all", label: "Semua" },
];

export function ReminderTab() {
  const [status, setStatus] = useState("pending");
  const reminders = useReminders(status);
  const cancel = useCancelReminder();
  const create = useCreateReminder();
  const [message, setMessage] = useState("");
  const [when, setWhen] = useState("");

  const rows = (reminders.data ?? []).slice().sort((a, b) => a.remind_at.localeCompare(b.remind_at));

  function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!message.trim() || !when) return;
    // datetime-local has no timezone; treat as WIB (+07:00).
    const remind_at = `${when}:00+07:00`;
    create.mutate({ message: message.trim(), remind_at }, {
      onSuccess: () => { setMessage(""); setWhen(""); toast.success("Reminder dibuat"); },
      onError: (err) => toast.error((err as Error).message),
    });
  }

  return (
    <div className="flex col gap-3">
      <div className="ptabs" role="group" aria-label="Filter status">
        {STATUSES.map((st) => (
          <button key={st.key} className={`ptab${status === st.key ? " active" : ""}`} onClick={() => setStatus(st.key)}>{st.label}</button>
        ))}
      </div>

      <form onSubmit={handleCreate} className="flex items-center gap-2 flex-wrap">
        <input className="input flex-1" placeholder="Pesan reminder…" aria-label="Pesan reminder" value={message} onChange={(e) => setMessage(e.target.value)} />
        <input className="input" type="datetime-local" aria-label="Waktu reminder" value={when} onChange={(e) => setWhen(e.target.value)} />
        <button type="submit" className="btn btn-primary" disabled={create.isPending}>Buat reminder</button>
      </form>

      {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada reminder.</p>}
      {rows.map((r) => (
        <div key={r.id} className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground w-28 shrink-0">{wibDayKey(r.remind_at).slice(5)} · {formatWibTime(r.remind_at)}</span>
          <span className="flex-1 truncate">{r.message}</span>
          {r.status !== "pending" && <span className="text-xs text-muted-foreground">{r.status}</span>}
          {r.status === "pending" && (
            <button type="button" aria-label={`Batalkan ${r.message}`} className="pt-icon-btn shrink-0" disabled={cancel.isPending}
              onClick={() => cancel.mutate(r.id, { onSuccess: () => toast.success("Reminder dibatalkan"), onError: (e) => toast.error((e as Error).message) })}>
              <X size={15} />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
