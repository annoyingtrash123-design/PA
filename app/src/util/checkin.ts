// The morning check-in: which of today's tasks still have no slot (no time, "anytime"), so Miranda can ask once.
// Pure, so it can be tested without the app.
import type { Day, Occurrence } from "../types";

const KEY = "pa-checkin";

/** Today's tasks that could use a slot: unfinished one-offs with no time and no part of the day. On a work day,
 *  Work tasks are left out: they simply happen at work. */
export function needsSlot(day: Day): Occurrence[] {
  return day.items.filter((o) => !o.done && !o.recurring && !o.time && (!o.part_of_day || o.part_of_day === "anytime") && !(day.work && o.category?.name === "Work"));
}

/** Whether to show the check-in: morning view, before 2 pm, not answered or skipped today, something to place. */
export function showCheckin(day: Day, clockNow: string, answeredOn: string | null): boolean {
  return clockNow < "14:00" && answeredOn !== day.date && needsSlot(day).length > 0;
}

export function checkinAnsweredOn(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function markCheckin(date: string): void {
  try {
    localStorage.setItem(KEY, date);
  } catch {
    // fine: it may ask again this morning
  }
}
