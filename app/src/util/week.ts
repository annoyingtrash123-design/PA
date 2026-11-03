// Which week the Home "Overview of the week" card shows first, and whether it starts open.
import { addDays, mondayOf } from "./dates";

/** Sunday, and Monday before noon (for the week just finished), open the card by default. */
export function defaultWeek(todayIso: string, hhmm: string): { week: string; open: boolean } {
  const monday = mondayOf(todayIso);
  const isSunday = addDays(monday, 6) === todayIso;
  const mondayMorning = monday === todayIso && hhmm < "12:00";
  if (mondayMorning) return { week: addDays(monday, -7), open: true };
  return { week: monday, open: isSunday };
}

export const weekOpenKey = (week: string) => `pa-week-open:${week}`;

/** The remembered choice for a week, or the default when nothing was stored. */
export function resolveOpen(stored: string | null, fallback: boolean): boolean {
  return stored === "1" ? true : stored === "0" ? false : fallback;
}
