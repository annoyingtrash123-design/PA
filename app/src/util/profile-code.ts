/** Private profile codes: `MIRANDA1:` + base64url(UTF-8 JSON). Kept in sync with scripts/make-profile-code.mjs. */
export const PROFILE_PREFIX = "MIRANDA1:";

export interface ProfileWork {
  enabled: boolean;
  days: number[];
  start: string;
  end: string;
  lunch_minutes: number;
  lunch_window_start: string;
  lunch_window_end: string;
  holiday_region: string;
}
export interface ProfileWeather {
  comfortable_max: number;
  hot_above: number;
  too_hot_above: number;
  likes_rain: boolean;
  avoid_sunburn: boolean;
}
export interface Profile {
  v: 1;
  updated: string;
  interests?: string;
  notes?: string[];
  work?: ProfileWork;
  weather?: ProfileWeather;
  add_goals?: { title: string; horizon: string; why?: string; deadline?: string }[];
  add_leisure?: { kind: string; title: string; note?: string; status?: string }[];
}

export class ProfileCodeError extends Error {}

const isObj = (x: unknown): x is Record<string, unknown> => typeof x === "object" && x !== null && !Array.isArray(x);

/** Minimal shape check (the core does the detailed validation). Returns the profile or throws ProfileCodeError. */
export function validateProfile(x: unknown): Profile {
  const bad = (m: string): never => { throw new ProfileCodeError(m); };
  if (!isObj(x)) return bad("The profile must be a JSON object");
  if (x.v !== 1) bad("Unsupported profile version");
  if (typeof x.updated !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(x.updated)) bad("\"updated\" must be YYYY-MM-DD");
  if (x.interests !== undefined && typeof x.interests !== "string") bad("\"interests\" must be text");
  if (x.notes !== undefined && (!Array.isArray(x.notes) || x.notes.some((n) => typeof n !== "string"))) bad("\"notes\" must be a list of text");
  if (x.work !== undefined && !isObj(x.work)) bad("\"work\" must be an object");
  if (x.weather !== undefined && !isObj(x.weather)) bad("\"weather\" must be an object");
  if (x.add_goals !== undefined && (!Array.isArray(x.add_goals) || x.add_goals.some((g) => !isObj(g) || typeof g.title !== "string" || typeof g.horizon !== "string"))) bad("\"add_goals\" items need a title and horizon");
  if (x.add_leisure !== undefined && (!Array.isArray(x.add_leisure) || x.add_leisure.some((l) => !isObj(l) || typeof l.title !== "string" || typeof l.kind !== "string"))) bad("\"add_leisure\" items need a kind and title");
  return x as unknown as Profile;
}

function toB64Url(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromB64Url(s: string): string {
  const std = s.replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(std + "=".repeat((4 - (std.length % 4)) % 4));
  return new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(bin, (c) => c.charCodeAt(0)));
}

export function encodeProfile(p: Profile): string {
  return PROFILE_PREFIX + toB64Url(JSON.stringify(validateProfile(p)));
}

/** Tolerates whitespace/newlines anywhere in the code. */
export function decodeProfile(code: string): Profile {
  const c = code.replace(/\s+/g, "");
  if (!c.startsWith(PROFILE_PREFIX)) throw new ProfileCodeError("That doesn't look like a profile code (it should start with MIRANDA1:)");
  const body = c.slice(PROFILE_PREFIX.length);
  if (!body || !/^[A-Za-z0-9_-]+$/.test(body)) throw new ProfileCodeError("The profile code has unexpected characters. Try copying it again.");
  let json: unknown;
  try {
    json = JSON.parse(fromB64Url(body));
  } catch {
    throw new ProfileCodeError("The profile code is damaged or incomplete. Try copying it again.");
  }
  return validateProfile(json);
}

export interface ApplySummary { interests: boolean; notes: number; work: boolean; weather: boolean; goals_added: number; leisure_added: number }

export function summarize(r: ApplySummary): string {
  const parts: string[] = [];
  if (r.goals_added) parts.push(`${r.goals_added} goal${r.goals_added === 1 ? "" : "s"} added`);
  if (r.leisure_added) parts.push(`${r.leisure_added} leisure item${r.leisure_added === 1 ? "" : "s"} added`);
  return ["Profile updated", ...parts].join(" · ");
}
