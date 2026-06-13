import { useMemo, useState } from "react";
import { toast } from "sonner";
import { MonthGrid } from "../components/MonthGrid";
import { DayEventsPanel } from "../components/DayEventsPanel";
import { EventDialog } from "../components/EventDialog";
import { QueryState } from "../components/QueryState";
import { useEvents, useCancelEvent } from "../api/hooks";
import { monthGridDays, gridRangeUtc, todayWibKey } from "../lib/wib";
import type { EventItem } from "../api/schemas";

interface AgendaPageProps {
  /** "YYYY-MM-DD" WIB key; defaults to today (enables test determinism). */
  initialDay?: string;
}

export default function AgendaPage({ initialDay }: AgendaPageProps) {
  const start = initialDay ?? todayWibKey();
  const [year, setYear] = useState(Number(start.slice(0, 4)));
  const [month, setMonth] = useState(Number(start.slice(5, 7)));
  const [selectedDay, setSelectedDay] = useState<string>(start);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<EventItem | null>(null);

  const range = useMemo(() => gridRangeUtc(monthGridDays(year, month)), [year, month]);
  const events = useEvents(range.fromZ, range.toZ);
  const cancel = useCancelEvent();

  const prevMonth = () => {
    if (month === 1) { setYear(year - 1); setMonth(12); } else setMonth(month - 1);
  };

  const nextMonth = () => {
    if (month === 12) { setYear(year + 1); setMonth(1); } else setMonth(month + 1);
  };

  const openCreate = () => { setEditing(null); setDialogOpen(true); };
  const openEdit = (eventItem: EventItem) => { setEditing(eventItem); setDialogOpen(true); };

  const onCancel = (eventItem: EventItem) => {
    if (!confirm(`Batalkan "${eventItem.title}"?`)) return;
    cancel.mutate(eventItem.id, {
      onSuccess: () => toast.success("Agenda dibatalkan"),
      onError: (err) => toast.error((err as Error).message),
    });
  };

  const data = events.data ?? [];

  return (
    <div className="space-y-4">
      <h1 className="text-lg font-semibold">Agenda</h1>
      <QueryState isLoading={events.isLoading} error={events.error}>
        <div className="grid gap-4 md:grid-cols-2">
          <MonthGrid
            year={year}
            month={month}
            events={data}
            selectedDay={selectedDay}
            onSelectDay={setSelectedDay}
            onPrevMonth={prevMonth}
            onNextMonth={nextMonth}
          />
          <DayEventsPanel
            day={selectedDay}
            events={data}
            onAdd={openCreate}
            onEdit={openEdit}
            onCancel={onCancel}
          />
        </div>
      </QueryState>
      <EventDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        event={editing}
        defaultDay={selectedDay}
      />
    </div>
  );
}
