// Assistant texts: ask the core what to write (prompts + template), ask Gemini if a key is saved,
// guard the reply (no raw stats), retry once, and fall back to the template. Replies are cached in `pa-ai-cache`.
import { api, assistantName, getState, hasStats, today } from "../core";
import type { BriefingPrep, DigestPrep, MorningParts, Weather } from "../types";
import { hasKey } from "./cache";
import { DAILY_BUDGET, isPaused, LOW_BUDGET, remaining, restingNotice, usedToday, VOICE_BUDGET, voiceUsedToday } from "./ai-budget";
import { shouldAutoGenerate, showUpdateButton, showWriteButton, storeKey, type StoredText } from "./ai-policy";
import { mondayOf } from "../util/dates";
import { generate, GeminiError, parseMorningJson, setDateProvider } from "./gemini";
import { weatherSettled } from "./weather";
import { mergeParts, morningScript, readEntry, type AiParts } from "./morning-script";
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
  /** morning: the briefing body (no hello, no weather); evening/weekly: the whole text */
  text: string;
  source: "gemini" | "template";
  prep: BriefingPrep;
  /** morning only: the four parts (AI where written, else template) */
  parts?: MorningParts;
  /** exactly what "Read it to me" says: morning = greeting, weather, affirmation, briefing; otherwise `text` */
  script: string;
  /** AI weather comment (morning only, from the same request as the briefing) */
  weather_comment?: string | null;
}

type Kind = "morning" | "evening" | "weekly";

const MORNING_JSON_NOTE =
  "\n\nOUTPUT FORMAT OVERRIDE: reply with ONLY a JSON object " +
  "{\"greeting\": string, \"weather_comment\": string or null, \"affirmation\": string, \"briefing\": string}. " +
  "No markdown, no code fences. Follow the part descriptions above exactly: \"briefing\" has no greeting and no weather, " +
  "\"weather_comment\" is null if no weather is given, and no part may contain a rain percentage or any count.";

interface StoredEntry {
  briefing: string;
  greeting?: string | null;
  weather_comment: string | null;
  affirmation?: string | null;
  fingerprint?: string;
}
interface StoredFull extends StoredText {
  ai: AiParts;
}

const templateParts = (prep: BriefingPrep): MorningParts | undefined => (prep.template_parts ? prep.template_parts : undefined);

/** Build the result for a kind: morning merges the four parts (template for whatever the AI did not give). */
function result(kind: Kind, prep: BriefingPrep, source: "gemini" | "template", ai: AiParts | null, extra: Partial<BriefingResult> = {}): BriefingResult {
  const tpl = kind === "morning" ? templateParts(prep) : undefined;
  const base = { source, prep, outdated: false, canWrite: false, ...extra };
  if (!tpl) {
    const text = ai?.briefing || prep.template_text;
    return { ...base, text, script: text };
  }
  const parts = mergeParts(ai, tpl);
  return { ...base, text: parts.briefing, parts, script: morningScript(parts), weather_comment: parts.weather_comment };
}

/** The stored AI text for a (kind,date)/week key; falls back to an old cache_key entry (no fingerprint) from earlier versions. */
async function readStored(skey: string, legacyKey: string): Promise<StoredFull | null> {
  for (const k of [skey, legacyKey]) {
    const e = readEntry(await kvGet<unknown>(DB, k));
    if (!e) continue;
    const { fingerprint, ...ai } = e;
    return { text: ai.briefing ?? "", ai, fingerprint };
  }
  return null;
}

/** The affirmation for a date without any network: the one the morning request wrote (cached), else the template one. */
export async function affirmationFor(date: string): Promise<string> {
  const prep = api<BriefingPrep>("GET", `/api/briefing?kind=morning&date=${date}`);
  const tpl = prep.template_parts?.affirmation || prep.affirmation || "";
  try {
    const stored = await readStored(storeKey("morning", date, mondayOf(date)), prep.cache_key);
    return (stored?.ai.affirmation || "").trim() || tpl;
  } catch {
    return tpl;
  }
}

/** Why the AI is not being used right now (resting / allowance used), as a friendly line, or null. */
export function aiNotice(): string | null {
  if (!hasKey()) return null;
  return restingNotice(assistantName(), today(), Date.now(), getState().settings.timezone);
}

/** AI uses so far today and the daily limit, for Settings. */
export const aiUsage = () => ({ used: usedToday(today()), limit: DAILY_BUDGET, voiceUsed: voiceUsedToday(today()), voiceLimit: VOICE_BUDGET });

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
  if (!hasKey()) return Promise.resolve(result(kind, early, "template", null));
  return once(GEN_KEY(`${skey}:${opts.force ? "f" : "a"}`), async () => {
    let stored = await readStored(skey, early.cache_key);
    if (stored && !opts.force) return result(kind, early, "gemini", stored.ai, flags(stored, early));
    if (!stored && !opts.force && !shouldAutoGenerate(kind, null, { currentWeek })) {
      return result(kind, early, "template", null, flags(null, early));
    }
    // Let the first forecast arrive so the first request is made once, with the weather in it. Later refreshes never trigger anything.
    if (!stored && kind !== "weekly") await weatherSettled();
    const prep = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
    const fallback = (notice?: string): BriefingResult =>
      stored ? result(kind, prep, "gemini", stored.ai, { ...flags(stored, prep), notice }) : result(kind, prep, "template", null, { ...flags(null, prep), notice });
    if (!opts.force && recentlyFailed(skey)) return fallback();
    const d = today();
    if (isPaused(Date.now()) || remaining(d) <= 0) return fallback(aiNotice() ?? "AI writing isn't available right now.");
    try {
      const morning = kind === "morning";
      const system = morning ? prep.system_prompt + MORNING_JSON_NOTE : prep.system_prompt;
      const ask = async (note: string): Promise<AiParts | null> => {
        const raw = await generate({ system, user: prep.user_prompt + note, json: morning });
        if (!morning) return { briefing: raw };
        return parseMorningJson(raw);
      };
      // the stats guard looks at everything that will be spoken
      const spoken = (r: AiParts) => [r.greeting, r.weather_comment, r.affirmation, r.briefing].filter(Boolean).join("\n");
      let r = await ask("");
      // one stats-guard retry, only while the budget is comfortable
      if (r && hasStats(spoken(r)) && remaining(d) > LOW_BUDGET) r = await ask(RETRY_NOTE);
      if (r && r.briefing && !hasStats(spoken(r))) {
        // non-morning texts have no parts: only morning entries carry the new keys (their absence marks an old entry)
        const entry: StoredEntry = morning
          ? { briefing: r.briefing, greeting: r.greeting ?? null, weather_comment: r.weather_comment ?? null, affirmation: r.affirmation ?? null, fingerprint: prep.plan_fingerprint }
          : { briefing: r.briefing, weather_comment: null, fingerprint: prep.plan_fingerprint };
        await kvSet(DB, skey, entry);
        return result(kind, prep, "gemini", r);
      }
    } catch (e) {
      if (e instanceof GeminiError && e.blocked) return fallback(aiNotice() ?? "AI writing isn't available right now.");
    }
    failedAt.set(skey, Date.now());
    return fallback("Couldn't write that just now. Try again later.");
  });
}

/** The instant template version of a briefing (no network), shown while the AI one is written. */
export function templateBriefing(kind: Kind, date: string): BriefingResult {
  return result(kind, api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`), "template", null);
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
