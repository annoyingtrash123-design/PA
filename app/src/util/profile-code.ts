/** Private profile codes: `MIRANDA2:` + base64url(deflate-raw(UTF-8 JSON)); legacy `MIRANDA1:` + base64url(UTF-8 JSON). Kept in sync with scripts/make-profile-code.mjs. */
export const PROFILE_PREFIX = "MIRANDA2:";
export const PROFILE_PREFIX_V1 = "MIRANDA1:";

export interface ProfileWork {
  enabled: boolean;
  days: number[];
  start: string;
  end: string;
  lunch_minutes: number;
  lunch_window_start: string;
  lunch_window_end: string;
  holiday_region: string;
  /** weekday index as string ("0" = Mon ... "6" = Sun) -> hours override */
  weekday_hours?: Record<string, { start: string; end: string }>;
  /** nth (1-5, or -1 = last) occurrence of a weekday (0 = Mon) in the month -> hours override */
  monthly_rules?: { nth: number; weekday: number; start: string; end: string }[];
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
  add_tasks?: { title: string; date: string; time?: string; part_of_day?: string; category?: string; notes?: string; recurrence?: unknown; checklist?: string[] }[];
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
  if (x.add_tasks !== undefined && (!Array.isArray(x.add_tasks) || x.add_tasks.some((t) => !isObj(t) || typeof t.title !== "string" || (t.date !== undefined && typeof t.date !== "string") || (t.checklist !== undefined && (!Array.isArray(t.checklist) || t.checklist.some((c: unknown) => typeof c !== "string")))))) bad("\"add_tasks\" items need a title and a YYYY-MM-DD date");
  return x as unknown as Profile;
}

function bytesToB64Url(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function b64UrlToBytes(s: string): Uint8Array {
  const std = s.replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(std + "=".repeat((4 - (std.length % 4)) % 4));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

const utf8 = (bytes: Uint8Array): string => new TextDecoder("utf-8", { fatal: true }).decode(bytes);

/** Legacy MIRANDA1 encoder (sync). */
export function encodeProfile(p: Profile): string {
  return PROFILE_PREFIX_V1 + bytesToB64Url(new TextEncoder().encode(JSON.stringify(validateProfile(p))));
}

async function pipeBytes(bytes: Uint8Array, stream: { writable: WritableStream<BufferSource>; readable: ReadableStream<Uint8Array> }): Promise<Uint8Array> {
  const w = stream.writable.getWriter();
  void w.write(bytes as BufferSource).catch(() => {});
  void w.close().catch(() => {});
  return new Uint8Array(await new Response(stream.readable).arrayBuffer());
}

/** MIRANDA2 encoder (the script is the normal producer; this is used for tests). */
export async function encodeProfileV2(p: Profile): Promise<string> {
  if (typeof CompressionStream === "undefined") throw new ProfileCodeError("This browser can't compress profile codes.");
  const raw = new TextEncoder().encode(JSON.stringify(validateProfile(p)));
  return PROFILE_PREFIX + bytesToB64Url(await pipeBytes(raw, new CompressionStream("deflate-raw")));
}

const BODY_RE = /^[A-Za-z0-9_-]+$/;

async function decodeBody(version: 1 | 2, body: string): Promise<Profile> {
  if (!body || !BODY_RE.test(body)) throw new ProfileCodeError("The profile code has unexpected characters. Try copying it again.");
  let json: unknown;
  try {
    const bytes = b64UrlToBytes(body);
    if (version === 1) json = JSON.parse(utf8(bytes));
    else {
      if (typeof DecompressionStream === "undefined") throw new ProfileCodeError("This browser can't read the newer compressed profile codes (MIRANDA2). Update your browser, or ask Claude for an older MIRANDA1 code.");
      json = JSON.parse(utf8(await pipeBytes(bytes, new DecompressionStream("deflate-raw"))));
    }
  } catch (e) {
    if (e instanceof ProfileCodeError) throw e;
    throw new ProfileCodeError("The profile code is damaged or incomplete. Try copying it again.");
  }
  return validateProfile(json);
}

/**
 * Finds the first MIRANDA1:/MIRANDA2: code in free text (extra words around it are fine, whitespace/line
 * wraps inside the code are ignored) and decodes it. Trailing words are dropped until the code decodes.
 */
export async function decodeProfile(text: string): Promise<Profile> {
  const m = /MIRANDA([12]):/.exec(text);
  if (!m) throw new ProfileCodeError("That doesn't look like a profile code (it should start with MIRANDA1: or MIRANDA2:)");
  const version = m[1] === "1" ? 1 : 2;
  const tokens = text.slice(m.index + m[0].length).split(/\s+/).filter(Boolean);
  const run: string[] = [];
  for (const t of tokens) {
    if (!BODY_RE.test(t)) {
      // a token may be a code tail glued to punctuation, e.g. "abc." : keep the clean prefix
      const head = /^[A-Za-z0-9_-]+/.exec(t)?.[0];
      if (head) run.push(head);
      break;
    }
    run.push(t);
  }
  if (!run.length) throw new ProfileCodeError("The profile code is empty. Try copying it again.");
  let first: unknown;
  for (let n = run.length; n >= 1; n--) {
    try {
      return await decodeBody(version, run.slice(0, n).join(""));
    } catch (e) {
      if (n === run.length) first = e;
      if (e instanceof ProfileCodeError && /newer compressed/.test(e.message)) throw e;
    }
  }
  throw first;
}

export interface ApplySummary { interests: boolean; notes: number; work: boolean; weather: boolean; goals_added: number; leisure_added: number; tasks_added?: number }

export function summarize(r: ApplySummary): string {
  const parts: string[] = [];
  if (r.goals_added) parts.push(`${r.goals_added} goal${r.goals_added === 1 ? "" : "s"} added`);
  if (r.leisure_added) parts.push(`${r.leisure_added} leisure item${r.leisure_added === 1 ? "" : "s"} added`);
  if (r.tasks_added) parts.push(`${r.tasks_added} task${r.tasks_added === 1 ? "" : "s"} added`);
  return ["Profile updated", ...parts].join(" · ");
}
