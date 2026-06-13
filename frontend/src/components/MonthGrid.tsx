import { useMemo } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { EventItem } from "../api/schemas";
import { monthGridDays, wibDayKey, todayWibKey } from "../lib/wib";
import { cn } from "@/lib/utils";

const MONTHS = [
  "Januari", "Februari", "Maret", "April", "Mei", "Juni",
  "Juli", "Agustus", "September", "Oktober", "November", "Desember",
];
const DOW = ["Sn", "Sl", "Rb", "Km", "Jm", "Sb", "Mg"];

interface MonthGridProps {
  year: number;
  month: number; // 1-12
  events: EventItem[];
  selectedDay: string | null; // "YYYY-MM-DD"
  onSelectDay: (day: string) => void;
  onPrevMonth: () => void;
  onNextMonth: () => void;
}

/**
 * Presentational monthly calendar grid (Monday-started, 42 cells).
 *
 * Each day cell shows the date number and an indicator dot when that WIB day
 * has ≥1 event. Today is highlighted with a primary ring; the selected day
 * gets an accent background. Out-of-month days are dimmed.
 *
 * @param year       - Full calendar year (e.g. 2026)
 * @param month      - 1-indexed month (1 = January, 12 = December)
 * @param events     - All EventItem objects to count against WIB day keys
 * @param selectedDay - Currently selected "YYYY-MM-DD" key, or null
 * @param onSelectDay - Called with the "YYYY-MM-DD" key when a day is clicked
 * @param onPrevMonth - Called when the "previous month" chevron is clicked
 * @param onNextMonth - Called when the "next month" chevron is clicked
 */
export function MonthGrid({
  year,
  month,
  events,
  selectedDay,
  onSelectDay,
  onPrevMonth,
  onNextMonth,
}: MonthGridProps) {
  const days = useMemo(() => monthGridDays(year, month), [year, month]);
  const today = todayWibKey();

  const countByDay = useMemo(() => {
    const dayCountMap = new Map<string, number>();
    for (const eventItem of events) {
      const dayKey = wibDayKey(eventItem.start_at);
      dayCountMap.set(dayKey, (dayCountMap.get(dayKey) ?? 0) + 1);
    }
    return dayCountMap;
  }, [events]);

  return (
    <div className="rounded-lg border p-3">
      {/* Month navigation header */}
      <div className="flex items-center justify-between mb-2">
        <button
          type="button"
          className="btn btn-outline btn-sm btn-icon"
          aria-label="Bulan sebelumnya"
          onClick={onPrevMonth}
        >
          <ChevronLeft size={16} />
        </button>
        <div className="font-medium text-sm">
          {MONTHS[month - 1]} {year}
        </div>
        <button
          type="button"
          className="btn btn-outline btn-sm btn-icon"
          aria-label="Bulan berikutnya"
          onClick={onNextMonth}
        >
          <ChevronRight size={16} />
        </button>
      </div>

      {/* Day-of-week header row */}
      <div className="grid grid-cols-7 gap-1 text-center text-xs text-muted-foreground mb-1">
        {DOW.map((dayLabel) => (
          <div key={dayLabel}>{dayLabel}</div>
        ))}
      </div>

      {/* 42-cell day grid */}
      <div className="grid grid-cols-7 gap-1">
        {days.map((day) => {
          const inMonth = Number(day.slice(5, 7)) === month;
          const eventCount = countByDay.get(day) ?? 0;
          const dayNumber = Number(day.slice(8, 10));

          return (
            <button
              key={day}
              type="button"
              onClick={() => onSelectDay(day)}
              className={cn(
                "aspect-square rounded-md border text-sm flex flex-col items-center justify-center gap-0.5",
                inMonth ? "" : "text-muted-foreground/50",
                day === today ? "ring-1 ring-primary" : "",
                day === selectedDay ? "bg-accent" : "hover:bg-accent/50",
              )}
            >
              <span>{dayNumber}</span>
              {eventCount > 0 && (
                <span
                  className="h-1.5 w-1.5 rounded-full bg-primary"
                  aria-label={`${eventCount} agenda`}
                />
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
