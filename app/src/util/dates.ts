// Date helpers on ISO "YYYY-MM-DD" strings (calendar maths in UTC so DST never shifts a day).

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
export const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

const toUTC = (iso: string) => {
  const [y, m, d] = iso.split("-").map(Number);
  return new Date(Date.UTC(y, m - 1, d));
};
const fromUTC = (d: Date) => d.toISOString().slice(0, 10);

export function addDays(iso: string, n: number): string {
  const d = toUTC(iso);
  d.setUTCDate(d.getUTCDate() + n);
  return fromUTC(d);
}

/** 0 = Monday .. 6 = Sunday. */
export function weekdayIndex(iso: string): number {
  return (toUTC(iso).getUTCDay() + 6) % 7;
}

export function mondayOf(iso: string): string {
  return addDays(iso, -weekdayIndex(iso));
}

export function diffDays(a: string, b: string): number {
  return Math.round((toUTC(a).getTime() - toUTC(b).getTime()) / 86_400_000);
}

export const dayNum = (iso: string) => Number(iso.slice(8, 10));
export const monthShort = (iso: string) => MONTHS[Number(iso.slice(5, 7)) - 1];

/** "3 Nov" */
export const dayMonth = (iso: string) => `${dayNum(iso)} ${monthShort(iso)}`;
/** "3 Nov 2026" */
export const prettyDate = (iso: string) => `${dayNum(iso)} ${monthShort(iso)} ${iso.slice(0, 4)}`;
/** "Tuesday 3 Nov" */
export function longDay(iso: string): string {
  const names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
  return `${names[weekdayIndex(iso)]} ${dayMonth(iso)}`;
}

/** "in 4 months", "3 days ago", "today" (port of the Flet helper). */
export function relativeDate(iso: string, today: string): string {
  const days = diffDays(iso, today);
  if (days === 0) return "today";
  const n = Math.abs(days);
  let txt: string;
  if (n < 14) txt = `${n} day${n !== 1 ? "s" : ""}`;
  else if (n < 60) txt = `${Math.round(n / 7)} weeks`;
  else if (n < 700) txt = `${Math.round(n / 30.44)} months`;
  else txt = `${Math.round(n / 365.25)} years`;
  return days < 0 ? `${txt} ago` : `in ${txt}`;
}

export interface ZonedNow {
  today: string;
  now: string; // "HH:MM"
}

/** Today's date and the clock time in an IANA timezone (falls back to the device zone if it is invalid). */
export function zonedNow(timezone: string | undefined, date: Date = new Date()): ZonedNow {
  const opts: Intl.DateTimeFormatOptions = {
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", hourCycle: "h23",
  };
  let parts: Intl.DateTimeFormatPart[];
  try {
    parts = new Intl.DateTimeFormat("en-CA", { ...opts, timeZone: timezone || undefined }).formatToParts(date);
  } catch {
    parts = new Intl.DateTimeFormat("en-CA", opts).formatToParts(date);
  }
  const get = (t: string) => parts.find((p) => p.type === t)?.value ?? "00";
  const hour = get("hour") === "24" ? "00" : get("hour");
  return { today: `${get("year")}-${get("month")}-${get("day")}`, now: `${hour}:${get("minute")}` };
}

export const isValidTimezone = (tz: string): boolean => {
  try {
    new Intl.DateTimeFormat("en", { timeZone: tz });
    return true;
  } catch {
    return false;
  }
};

export function greeting(hhmm: string): string {
  const h = Number(hhmm.slice(0, 2));
  return h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening";
}

export const TIME_RE = /^([01]?\d|2[0-3]):([0-5]\d)$/;

/** Part of day for a clock time (used when a task has a time). */
export function derivePart(hhmm: string): "morning" | "afternoon" | "evening" {
  const h = Number(hhmm.slice(0, 2));
  return h < 12 ? "morning" : h < 17 ? "afternoon" : "evening";
}
