import { useMemo } from "react";
import { Link } from "react-router-dom";
import { Badge } from "@/components/ui/badge";
import { useEvents } from "../api/hooks";
import { nextDaysRangeUtc, todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const UPCOMING_DAYS = 7;
const MAX_ROWS = 5;

export function DashboardAgendaCard() {
  const range = useMemo(() => nextDaysRangeUtc(todayWibKey(), UPCOMING_DAYS), []);
  const events = useEvents(range.fromZ, range.toZ);

  const rows = (events.data ?? [])
    .slice()
    .sort((a, b) => a.start_at.localeCompare(b.start_at))
    .slice(0, MAX_ROWS);

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Agenda</div>
        <Link to="/agenda" className="text-sm text-primary hover:underline">Lihat semua →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Belum ada agenda.</p>}
        {rows.map((e) => (
          <div key={e.id} className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground w-24 shrink-0">
              {wibDayKey(e.start_at) === todayWibKey() ? "Hari ini" : wibDayKey(e.start_at).slice(5)} · {formatWibTime(e.start_at)}
            </span>
            <span className="flex-1 truncate">{e.title}</span>
            {e.source === "google" && <Badge variant="secondary">Google</Badge>}
          </div>
        ))}
      </div>
    </div>
  );
}
