// Assistant texts: ask the core what to write (prompts + template), ask Gemini if a key is saved,
// guard the reply (no raw stats), retry once, and fall back to the template. Replies are cached in `pa-ai-cache`.
import { api, assistantName, getState, hasStats, today } from "../core";
import type { BriefingPrep, DigestPrep, Weather } from "../types";
import { hasKey } from "./cache";
import { canReuseLast, DAILY_BUDGET, isPaused, LOW_BUDGET, lsStore, remaining, restingNotice, usedToday, type LastEntry } from "./ai-budget";
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
interface MorningCache {
  briefing: string;
  weather_comment: string | null;
}

const MORNING_JSON_NOTE =
  "\n\nOUTPUT FORMAT OVERRIDE: reply with ONLY a JSON object {\"weather_comment\": string or null, \"briefing\": string}. " +
  "No markdown, no code fences. \"briefing\" is the full spoken script described above. " +
  "\"weather_comment\" is 1 to 2 sentences interpreting the forecast through the person's weather preferences and, where natural, relating it to the day's plans; " +
  "temperatures as degrees, never a rain percentage; null if no weather is given.";

const LAST_KEY = "pa-ai-last";
const readLast = (): Record<string, LastEntry> => {
  try {
    return JSON.parse(lsStore.get(LAST_KEY) || "{}") ?? {};
  } catch {
    return {};
  }
};
const writeLast = (kind: Kind, date: string, entry: LastEntry) => {
  const all = readLast();
  all[`${kind}:${date}`] = entry;
  for (const k of Object.keys(all)) if (k.split(":")[1] < date) delete all[k]; // forget earlier days
  lsStore.set(LAST_KEY, JSON.stringify(all));
};

async function readCached(key: string): Promise<{ text: string; weather_comment: string | null } | null> {
  const v = await kvGet<string | MorningCache>(DB, key);
  if (!v) return null;
  if (typeof v === "string") return { text: v, weather_comment: null };
  return { text: v.briefing, weather_comment: v.weather_comment };
}

/** The AI weather comment already written today (by the morning briefing), if any. No network. */
export async function cachedWeatherComment(date: string): Promise<string | null> {
  try {
    const prep = api<BriefingPrep>("GET", `/api/briefing?kind=morning&date=${date}`);
    return (await readCached(prep.cache_key))?.weather_comment ?? null;
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

export function getBriefing(kind: Kind, date: string): Promise<Briefing> {
  const early = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
  const earlyTemplate: Briefing = { text: early.template_text, source: "template", prep: early };
  if (!hasKey()) return Promise.resolve(earlyTemplate);
  // Let the first forecast arrive so the request is made once, with the weather in it (not before and after).
  const ready = kind === "weekly" ? Promise.resolve() : weatherSettled();
  return ready.then(() => {
    const prep = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
    const template: Briefing = { text: prep.template_text, source: "template", prep };
    return once(`b:${prep.cache_key}`, async () => {
      const cached = await readCached(prep.cache_key);
      if (cached) return { text: cached.text, source: "gemini" as const, prep, weather_comment: cached.weather_comment };
      // evening / weekly texts include done states: reuse the last one for this date for 3 hours
      const last = readLast()[`${kind}:${date}`];
      const lastText = kind !== "morning" && last ? await readCached(last.key) : null;
      if (lastText && canReuseLast(last, Date.now())) return { text: lastText.text, source: "gemini" as const, prep };
      const stale: Briefing | null = lastText ? { text: lastText.text, source: "gemini", prep } : null;
      if (recentlyFailed(prep.cache_key)) return stale ?? template;
      const d = today();
      if (isPaused(Date.now()) || remaining(d) <= 0) return stale ?? template;
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
          await kvSet(DB, prep.cache_key, morning ? ({ briefing: r.text, weather_comment: r.weather_comment } satisfies MorningCache) : r.text);
          writeLast(kind, date, { at: Date.now(), key: prep.cache_key });
          return { text: r.text, source: "gemini" as const, prep, weather_comment: r.weather_comment };
        }
      } catch (e) {
        if (e instanceof GeminiError && e.blocked) return stale ?? template; // not a failure: just not allowed right now
      }
      failedAt.set(prep.cache_key, Date.now());
      return stale ?? template;
    });
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
