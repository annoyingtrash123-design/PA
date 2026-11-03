import type { Occurrence } from "../types";

export const PART_ORDER = ["morning", "afternoon", "evening", "anytime"] as const;
export const PART_LABELS: Record<string, string> = { morning: "Morning", afternoon: "Afternoon", evening: "Evening", anytime: "Anytime" };

/**
 * First not-done item still upcoming; else an untimed item for the current/later part of day;
 * else any remaining one. `now` is "HH:MM" in the user's timezone (port of views/home.py pick_next_up).
 */
export function pickNextUp<T extends Pick<Occurrence, "done" | "time" | "part_of_day">>(items: T[], now: string): T | null {
  const left = items.filter((o) => !o.done);
  for (const o of left) if (o.time && o.time >= now) return o;
  const hour = Number(now.slice(0, 2));
  const cur = hour < 12 ? 0 : hour < 17 ? 1 : 2;
  for (const o of left) {
    if (!o.time) {
      const part = o.part_of_day || "anytime";
      if (part === "anytime" || PART_ORDER.indexOf(part) >= cur) return o;
    }
  }
  const untimed = left.filter((o) => !o.time);
  return untimed[0] ?? left[0] ?? null;
}
