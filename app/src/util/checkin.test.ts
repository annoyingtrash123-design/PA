import { describe, expect, it } from "vitest";
import { needsSlot, showCheckin } from "./checkin";
import type { Day, Occurrence } from "../types";

const occ = (over: Partial<Occurrence>): Occurrence => ({ task_id: 1, date: "2026-11-10", title: "x", time: null, part_of_day: "anytime", category: null, notes: "", recurring: false, rolled_over_from: null, done: false, checklist: [], ...over });
const day = (items: Occurrence[], work = true): Day => ({ date: "2026-11-10", weekday: "Tuesday", items, workout: null, work: work ? { start: "10:00", end: "20:00" } : null, holiday: null, day_off: false });

describe("morning check-in", () => {
  it("asks only about unplaced, unfinished one-offs (work tasks go to work on a work day)", () => {
    const work = { id: 2, name: "Work", color: "#000" };
    const d = day([
      occ({ task_id: 1, title: "Haircut" }),
      occ({ task_id: 2, title: "Call Mum", time: "18:00" }),
      occ({ task_id: 3, title: "Laundry", part_of_day: "evening" }),
      occ({ task_id: 4, title: "Excel notes", category: work }),
      occ({ task_id: 5, title: "Plan tomorrow", recurring: true }),
      occ({ task_id: 6, title: "Pay rent", done: true }),
    ]);
    expect(needsSlot(d).map((o) => o.task_id)).toEqual([1]);
    expect(needsSlot(day([occ({ task_id: 4, title: "Excel notes", category: work })], false)).map((o) => o.task_id)).toEqual([4]);
  });
  it("shows in the morning, once a day", () => {
    const d = day([occ({ title: "Haircut" })]);
    expect(showCheckin(d, "07:30", null)).toBe(true);
    expect(showCheckin(d, "07:30", "2026-11-10")).toBe(false);
    expect(showCheckin(d, "15:00", null)).toBe(false);
    expect(showCheckin(day([]), "07:30", null)).toBe(false);
  });
});
