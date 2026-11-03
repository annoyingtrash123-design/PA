// Pure decision rules for when assistant texts are written. No browser, network or clock needed.
//  - Write once per (kind, date); only the FIRST text of the day is automatic.
//  - Later changes to the plan only surface a button; regenerating needs a tap.
//  - Weather never matters here (the fingerprint excludes it).
//  - The weekly overview is automatic for the current week only; other weeks offer a "Write overview" button.

export type TextKind = "morning" | "evening" | "weekly";

export interface StoredText {
  text: string;
  /** plan fingerprint at the time of writing; undefined for old entries (then no update button is offered) */
  fingerprint?: string;
}

/** Storage key: one AI text per day for morning/evening, one per week (its Monday) for the weekly overview. */
export const storeKey = (kind: TextKind, date: string, weekStart: string): string => (kind === "weekly" ? `week:${weekStart}` : `${kind}:${date}`);

/** Should a text be requested without the person asking? Only when none exists yet (and, weekly, only this week). */
export function shouldAutoGenerate(kind: TextKind, cached: StoredText | null | undefined, opts: { currentWeek?: boolean } = {}): boolean {
  if (cached) return false;
  if (kind === "weekly") return !!opts.currentWeek;
  return true;
}

/** Show the "Plans changed — update summary" button: a text exists and the plan fingerprint differs from the one it was written for. */
export function showUpdateButton(cached: StoredText | null | undefined, currentFingerprint: string | undefined): boolean {
  if (!cached || !cached.fingerprint || !currentFingerprint) return false;
  return cached.fingerprint !== currentFingerprint;
}

/** Show the "Write overview" button: weekly, no AI text yet, and not an automatic (current) week. */
export function showWriteButton(kind: TextKind, cached: StoredText | null | undefined, opts: { currentWeek?: boolean } = {}): boolean {
  return kind === "weekly" && !cached && !opts.currentWeek;
}
