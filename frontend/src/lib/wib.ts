// WIB (Asia/Jakarta) is a fixed UTC+7 offset (no DST). Events are stored as UTC
// "...Z"; the UI groups and displays them in WIB and converts form input back to UTC.
const WIB_OFFSET_MS = 7 * 60 * 60 * 1000;

/** UTC "...Z" -> "YYYY-MM-DD" of the instant in the WIB calendar. */
export function wibDayKey(utcZ: string): string {
  const d = new Date(new Date(utcZ).getTime() + WIB_OFFSET_MS);
  return d.toISOString().slice(0, 10);
}

/** UTC "...Z" -> "HH:MM" in WIB. */
export function formatWibTime(utcZ: string): string {
  const d = new Date(new Date(utcZ).getTime() + WIB_OFFSET_MS);
  return d.toISOString().slice(11, 16);
}

/** WIB wall-clock (date "YYYY-MM-DD", time "HH:MM") -> UTC "...:SSZ" (no millis). */
export function wibDateTimeToUtcZ(dateStr: string, timeStr: string): string {
  const [y, m, d] = dateStr.split("-").map(Number);
  const [hh, mm] = timeStr.split(":").map(Number);
  const utcMs = Date.UTC(y, m - 1, d, hh, mm) - WIB_OFFSET_MS;
  return new Date(utcMs).toISOString().replace(/\.\d{3}Z$/, "Z");
}

/** "YYYY-MM-DD" (WIB) -> UTC "...Z" for 00:00 WIB that day. */
function wibDayStartUtcZ(dayKey: string): string {
  return wibDateTimeToUtcZ(dayKey, "00:00");
}

/** Add `n` days to a "YYYY-MM-DD" key (calendar math, TZ-agnostic). */
function addDays(dayKey: string, n: number): string {
  const [y, m, d] = dayKey.split("-").map(Number);
  const dt = new Date(Date.UTC(y, m - 1, d + n));
  return dt.toISOString().slice(0, 10);
}

/**
 * The 42 day keys ("YYYY-MM-DD") of a Monday-started 6-week grid containing the
 * given WIB month. `month` is 1-12.
 */
export function monthGridDays(year: number, month: number): string[] {
  const first = new Date(Date.UTC(year, month - 1, 1));
  const mondayOffset = (first.getUTCDay() + 6) % 7;
  const start = new Date(Date.UTC(year, month - 1, 1 - mondayOffset));
  const days: string[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(start.getTime() + i * 24 * 60 * 60 * 1000);
    days.push(d.toISOString().slice(0, 10));
  }
  return days;
}

/** UTC range [first day 00:00 WIB, (last day + 1) 00:00 WIB) for a list of day keys. */
export function gridRangeUtc(days: string[]): { fromZ: string; toZ: string } {
  const first = days[0];
  const last = days[days.length - 1];
  return { fromZ: wibDayStartUtcZ(first), toZ: wibDayStartUtcZ(addDays(last, 1)) };
}

/** UTC range covering today 00:00 WIB through +n days (exclusive end), WIB. */
export function nextDaysRangeUtc(todayKey: string, n: number): { fromZ: string; toZ: string } {
  return { fromZ: wibDayStartUtcZ(todayKey), toZ: wibDayStartUtcZ(addDays(todayKey, n)) };
}

/** Today's WIB day key, from the current instant. */
export function todayWibKey(): string {
  return wibDayKey(new Date().toISOString());
}
