import { Link } from "react-router-dom";
import { useReminders } from "../api/hooks";
import { todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const MAX_ROWS = 5;

export function DashboardReminderCard() {
  const reminders = useReminders();
  const rows = (reminders.data ?? [])
    .slice()
    .sort((a, b) => a.remind_at.localeCompare(b.remind_at))
    .slice(0, MAX_ROWS);

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Reminder mendatang</div>
        <Link to="/chat" className="text-sm text-primary hover:underline">Tanya Noah →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada reminder.</p>}
        {rows.map((r) => (
          <div key={r.id} className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground w-24 shrink-0">
              {wibDayKey(r.remind_at) === todayWibKey() ? "Hari ini" : wibDayKey(r.remind_at).slice(5)} · {formatWibTime(r.remind_at)}
            </span>
            <span className="flex-1 truncate">{r.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
