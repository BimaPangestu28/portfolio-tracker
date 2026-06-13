import { describe, it, expect } from "vitest";
import { wibDayKey, formatWibTime, wibDateTimeToUtcZ, monthGridDays, gridRangeUtc, nextDaysRangeUtc } from "./wib";

describe("wib util", () => {
  it("wibDayKey shifts UTC into the WIB calendar day", () => {
    expect(wibDayKey("2026-06-12T19:00:00Z")).toBe("2026-06-13");
    expect(wibDayKey("2026-06-13T07:00:00Z")).toBe("2026-06-13");
  });

  it("formatWibTime renders HH:MM in WIB", () => {
    expect(formatWibTime("2026-06-13T00:00:00Z")).toBe("07:00");
    expect(formatWibTime("2026-06-12T19:30:00Z")).toBe("02:30");
  });

  it("wibDateTimeToUtcZ converts a WIB wall-clock to UTC Z (no millis)", () => {
    expect(wibDateTimeToUtcZ("2026-06-13", "07:00")).toBe("2026-06-13T00:00:00Z");
    expect(wibDateTimeToUtcZ("2026-06-13", "02:30")).toBe("2026-06-12T19:30:00Z");
  });

  it("monthGridDays returns 42 Mon-started day keys covering the month", () => {
    const days = monthGridDays(2026, 6);
    expect(days.length).toBe(42);
    expect(days[0]).toBe("2026-06-01");
    expect(days).toContain("2026-06-30");
    expect(days[7]).toBe("2026-06-08");
  });

  it("gridRangeUtc spans first day 00:00 WIB to day-after-last 00:00 WIB", () => {
    const r = gridRangeUtc(["2026-06-01", "2026-06-02"]);
    expect(r.fromZ).toBe("2026-05-31T17:00:00Z");
    expect(r.toZ).toBe("2026-06-02T17:00:00Z");
  });

  it("nextDaysRangeUtc covers today 00:00 WIB through +n days", () => {
    const r = nextDaysRangeUtc("2026-06-13", 7);
    expect(r.fromZ).toBe("2026-06-12T17:00:00Z");
    expect(r.toZ).toBe("2026-06-19T17:00:00Z");
  });
});
