import { describe, expect, it } from "vitest";
import { defaultWeek, resolveOpen } from "./week";

describe("week card defaults", () => {
  it("is collapsed on ordinary days and shows this week", () => {
    expect(defaultWeek("2026-11-04", "09:00")).toEqual({ week: "2026-11-02", open: false });
  });
  it("opens on Sunday for the current week", () => {
    expect(defaultWeek("2026-11-08", "15:00")).toEqual({ week: "2026-11-02", open: true });
  });
  it("opens on Monday morning for the week just finished, then reverts at noon", () => {
    expect(defaultWeek("2026-11-09", "08:30")).toEqual({ week: "2026-11-02", open: true });
    expect(defaultWeek("2026-11-09", "12:00")).toEqual({ week: "2026-11-09", open: false });
  });
  it("prefers a remembered choice", () => {
    expect(resolveOpen("0", true)).toBe(false);
    expect(resolveOpen("1", false)).toBe(true);
    expect(resolveOpen(null, true)).toBe(true);
  });
});
