import { Plus, Pencil, X } from "lucide-react";
import type { EventItem } from "../api/schemas";
import { formatWibTime, wibDayKey } from "../lib/wib";
import { Badge } from "@/components/ui/badge";

interface DayEventsPanelProps {
  day: string; // "YYYY-MM-DD" WIB
  events: EventItem[]; // all loaded events; this panel filters to `day`
  onAdd: () => void;
  onEdit: (e: EventItem) => void;
  onCancel: (e: EventItem) => void;
}

export function DayEventsPanel({ day, events, onAdd, onEdit, onCancel }: DayEventsPanelProps) {
  const dayEvents = events
    .filter((e) => wibDayKey(e.start_at) === day)
    .sort((a, b) => a.start_at.localeCompare(b.start_at));

  return (
    <div className="rounded-lg border p-3 space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">{day}</h3>
        <button type="button" className="btn btn-primary btn-sm" onClick={onAdd}>
          <Plus size={14} /> Tambah
        </button>
      </div>

      {dayEvents.length === 0 && (
        <p className="text-sm text-muted-foreground">Tidak ada agenda hari ini.</p>
      )}

      <ul className="space-y-1">
        {dayEvents.map((e) => {
          const isGoogle = e.source === "google";
          return (
            <li key={e.id} className="flex items-center gap-2 rounded-md border px-2 py-1.5">
              <span className="text-sm tabular-nums w-12">{formatWibTime(e.start_at)}</span>
              <span className="flex-1 text-sm truncate">
                {e.title}
                {e.location ? <span className="text-muted-foreground"> · {e.location}</span> : null}
              </span>
              {isGoogle && <Badge variant="secondary">Google</Badge>}
              {!isGoogle && (
                <>
                  <button
                    type="button"
                    className="btn btn-ghost btn-icon"
                    aria-label="Edit"
                    onClick={() => onEdit(e)}
                  >
                    <Pencil size={14} />
                  </button>
                  <button
                    type="button"
                    className="btn btn-ghost btn-icon"
                    aria-label="Batalkan"
                    onClick={() => onCancel(e)}
                  >
                    <X size={14} />
                  </button>
                </>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
