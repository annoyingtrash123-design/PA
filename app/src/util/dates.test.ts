import { describe, expect, it } from "vitest";
import { addDays, dayMonth, derivePart, diffDays, greeting, isValidTimezone, longDay, mondayOf, prettyDate, relativeDate, weekdayIndex, zonedNow } from "./dates";

describe("date maths", () => {
  it("adds days across month and year ends", () => {
    expect(addDays("2026-11-30", 1)).toBe("2026-12-01");
    expect(addDays("2026-01-01", -1)).toBe("2025-12-31");
    expect(addDays("2024-02-28", 2)).toBe("2024-03-01");
  });
  it("finds Monday-based weekdays", () => {
    expect(weekdayIndex("2026-11-02")).toBe(0); // Monday
    expect(weekdayIndex("2026-11-08")).toBe(6); // Sunday
    expect(mondayOf("2026-11-08")).toBe("2026-11-02");
    expect(mondayOf("2026-11-03")).toBe("2026-11-02");
  });
  it("formats dates", () => {
    expect(dayMonth("2026-11-03")).toBe("3 Nov");
    expect(prettyDate("2027-03-01")).toBe("1 Mar 2027");
    expect(longDay("2026-11-03")).toBe("Tuesday 3 Nov");
  });
  it("describes distance like the Flet app", () => {
    const t = "2026-11-03";
    expect(relativeDate(t, t)).toBe("today");
    expect(relativeDate("2026-11-04", t)).toBe("in 1 day");
    expect(relativeDate("2026-11-10", t)).toBe("in 7 days");
    expect(relativeDate("2026-12-01", t)).toBe("in 4 weeks");
    expect(relativeDate("2027-03-01", t)).toBe("in 4 months");
    expect(relativeDate("2026-10-14", t)).toBe("3 weeks ago");
    expect(relativeDate("2029-11-03", t)).toBe("in 3 years");
    expect(diffDays("2026-11-10", t)).toBe(7);
  });
});

describe("zonedNow", () => {
  const instant = new Date("2026-11-02T20:30:00Z");
  it("uses the given timezone for the date and the clock", () => {
    expect(zonedNow("Australia/Sydney", instant)).toEqual({ today: "2026-11-03", now: "07:30" });
    expect(zonedNow("America/Los_Angeles", instant)).toEqual({ today: "2026-11-02", now: "12:30" });
    expect(zonedNow("UTC", instant)).toEqual({ today: "2026-11-02", now: "20:30" });
  });
  it("shows midnight as 00:xx, never 24:xx", () => {
    expect(zonedNow("UTC", new Date("2026-11-02T00:05:00Z")).now).toBe("00:05");
  });
  it("falls back to the device zone for an invalid timezone", () => {
    expect(zonedNow("Not/AZone", instant).today).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(isValidTimezone("Not/AZone")).toBe(false);
    expect(isValidTimezone("Europe/London")).toBe(true);
  });
});

describe("small helpers", () => {
  it("greets by hour", () => {
    expect(greeting("07:00")).toBe("Good morning");
    expect(greeting("12:00")).toBe("Good afternoon");
    expect(greeting("18:00")).toBe("Good evening");
  });
  it("derives a part of day", () => {
    expect(derivePart("07:30")).toBe("morning");
    expect(derivePart("12:00")).toBe("afternoon");
    expect(derivePart("17:00")).toBe("evening");
  });
});
