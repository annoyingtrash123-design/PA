// Assistant texts: ask the core what to write (prompts + template), ask Gemini if a key is saved,
// guard the reply (no raw stats), retry once, and fall back to the template. Replies are cached in `pa-ai-cache`.
import { api, assistantName, getState, hasStats, today } from "../core";
import type { BriefingPrep, DigestPrep, Weather } from "../types";
import { hasKey } from "./cache";
import { DAILY_BUDGET, isPaused, LOW_BUDGET, remaining, restingNotice, usedToday } from "./ai-budget";
import { shouldAutoGenerate, showUpdateButton, showWriteButton, storeKey, type StoredText } from "./ai-policy";
import { mondayOf } from "../util/dates";
import { generate, GeminiError, parseMorningJson, setDateProvider } from "./gemini";
import { weatherSettled } from "./weather";
import { kvGet, kvSet } from "./idb";

const DB = "pa-ai-cache";
setDateProvider(() => today());
const RETRY_NOTE =
  "\n\nReminder: do not mention counts, fractions or percentages (like '3 of 5' or '60 percent'). " +
  "Talk about the specific things done or coming up instead. Plain spoken prose only.";
const failedAt = new Map<string, number>();
const FAIL_BACKOFF_MS = 5 * 60 * 1000;
const inflight = new Map<string, Promise<unknown>>();

function once<T>(key: string, fn: () => Promise<T>): Promise<T> {
  const p = inflight.get(key) as Promise<T> | undefined;
  if (p) return p;
  const np = fn().finally(() => inflight.delete(key));
  inflight.set(key, np);
  return np;
}

const recentlyFailed = (key: string) => Date.now() - (failedAt.get(key) ?? 0) < FAIL_BACKOFF_MS;

export interface Briefing {
  text: string;
  source: "gemini" | "template";
  prep: BriefingPrep;
  /** AI weather comment (morning only, from the same request as the briefing) */
  weather_comment?: string | null;
}

type Kind = "morning" | "evening" | "weekly";

const MORNING_JSON_NOTE =
  "\n\nOUTPUT FORMAT OVERRIDE: reply with ONLY a JSON object {\"weather_comment\": string or null, \"briefing\": string}. " +
  "No markdown, no code fences. \"briefing\" is the full spoken script described above. " +
  "\"weather_comment\" is 1 to 2 sentences interpreting the forecast through the person's weather preferences and, where natural, relating it to the day's plans; " +
  "temperatures as degrees, never a rain percentage; null if no weather is given.";

interface StoredEntry {
  briefing: string;
  weather_comment: string | null;
  fingerprint?: string;
}
interface StoredFull extends StoredText {
  weather_comment: string | null;
}

/** The stored AI text for a (kind,date)/week key; falls back to an old cache_key entry (no fingerprint) from earlier versions. */
async function readStored(skey: string, legacyKey: string): Promise<StoredFull | null> {
  for (const k of [skey, legacyKey]) {
    const v = await kvGet<string | StoredEntry>(DB, k);
    if (!v) continue;
    if (typeof v === "string") return { text: v, weather_comment: null };
    return { text: v.briefing, weather_comment: v.weather_comment, fingerprint: v.fingerprint };
  }
  return null;
}

/** The AI weather comment already written today (by the morning briefing), if any. No network. */
export async function cachedWeatherComment(date: string): Promise<string | null> {
  try {
    const prep = api<BriefingPrep>("GET", `/api/briefing?kind=morning&date=${date}`);
    return (await readStored(storeKey("morning", date, mondayOf(date)), prep.cache_key))?.weather_comment ?? null;
  } catch {
    return null;
  }
}

/** Why the AI is not being used right now (resting / allowance used), as a friendly line, or null. */
export function aiNotice(): string | null {
  if (!hasKey()) return null;
  return restingNotice(assistantName(), today(), Date.now(), getState().settings.timezone);
}

/** AI uses so far today and the daily limit, for Settings. */
export const aiUsage = () => ({ used: usedToday(today()), limit: DAILY_BUDGET });

export interface BriefingResult extends Briefing {
  /** an AI text exists but the plan changed since: offer "Plans changed — update summary" */
  outdated: boolean;
  /** weekly, no AI text, not the current week: offer "Write overview" */
  canWrite: boolean;
  /** why a requested (forced) write did not happen, for a toast */
  notice?: string;
}

const GEN_KEY = (k: string) => `gen:${k}`;

/**
 * The text for a kind/date. Reads the stored AI text first (never regenerates it on its own). A request is made only
 * when there is no text yet and shouldAutoGenerate() allows it, or when `force` is set (the person tapped a button).
 */
export function getBriefing(kind: Kind, date: string, opts: { force?: boolean } = {}): Promise<BriefingResult> {
  const early = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
  const week = mondayOf(date);
  const currentWeek = week === mondayOf(today());
  const skey = storeKey(kind, date, week);
  const flags = (stored: StoredText | null, prep: BriefingPrep) => ({ outdated: showUpdateButton(stored, prep.plan_fingerprint), canWrite: showWriteButton(kind, stored, { currentWeek }) });
  if (!hasKey()) return Promise.resolve({ text: early.template_text, source: "template", prep: early, outdated: false, canWrite: false });
  return once(GEN_KEY(`${skey}:${opts.force ? "f" : "a"}`), async () => {
    let stored = await readStored(skey, early.cache_key);
    if (stored && !opts.force) return { text: stored.text, source: "gemini" as const, prep: early, weather_comment: stored.weather_comment, ...flags(stored, early) };
    if (!stored && !opts.force && !shouldAutoGenerate(kind, null, { currentWeek })) {
      return { text: early.template_text, source: "template" as const, prep: early, ...flags(null, early) };
    }
    // Let the first forecast arrive so the first request is made once, with the weather in it. Later refreshes never trigger anything.
    if (!stored && kind !== "weekly") await weatherSettled();
    const prep = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
    const fallback = (notice?: string): BriefingResult =>
      stored
        ? { text: stored.text, source: "gemini", prep, weather_comment: stored.weather_comment, ...flags(stored, prep), notice }
        : { text: prep.template_text, source: "template", prep, ...flags(null, prep), notice };
    if (!opts.force && recentlyFailed(skey)) return fallback();
    const d = today();
    if (isPaused(Date.now()) || remaining(d) <= 0) return fallback(aiNotice() ?? "AI writing isn't available right now.");
    try {
      const morning = kind === "morning";
      const system = morning ? prep.system_prompt + MORNING_JSON_NOTE : prep.system_prompt;
      const ask = async (note: string) => {
        const raw = await generate({ system, user: prep.user_prompt + note, json: morning });
        if (!morning) return { text: raw, weather_comment: null as string | null };
        const parsed = parseMorningJson(raw);
        return parsed ? { text: parsed.briefing, weather_comment: parsed.weather_comment } : null;
      };
      let r = await ask("");
      // one stats-guard retry, only while the budget is comfortable
      if (r && hasStats(r.text) && remaining(d) > LOW_BUDGET) r = await ask(RETRY_NOTE);
      if (r && r.text && !hasStats(r.text)) {
        const entry: StoredEntry = { briefing: r.text, weather_comment: r.weather_comment, fingerprint: prep.plan_fingerprint };
        await kvSet(DB, skey, entry);
        stored = { text: r.text, weather_comment: r.weather_comment, fingerprint: prep.plan_fingerprint };
        return { text: r.text, source: "gemini" as const, prep, weather_comment: r.weather_comment, outdated: false, canWrite: false };
      }
    } catch (e) {
      if (e instanceof GeminiError && e.blocked) return fallback(aiNotice() ?? "AI writing isn't available right now.");
    }
    failedAt.set(skey, Date.now());
    return fallback("Couldn't write that just now. Try again later.");
  });
}

/** The instant template version of a briefing (no network), shown while the AI one is written. */
export function templateBriefing(kind: Kind, date: string): string {
  return api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`).template_text;
}

export interface Digest {
  weather: Weather | null;
  weather_comment: string | null;
  overview: string;
  source: "gemini" | "template";
}

/** The instant template version of the digest (no network). */
export function templateDigest(date: string): Digest {
  const prep = api<DigestPrep>("GET", `/api/digest?date=${date}`);
  return { weather: prep.weather, weather_comment: prep.template.weather_comment, overview: prep.template.overview, source: "template" };
}

export async function testKey(key: string, model: string): Promise<string> {
  return generate({ system: "You are a friendly assistant.", user: "Reply with exactly: Hello, your key works.", key, model, maxTokens: 512, bypassBudget: true });
}
