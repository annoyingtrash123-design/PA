// Assistant texts: ask the core what to write (prompts + template), ask Gemini if a key is saved,
// guard the reply (no raw stats), retry once, and fall back to the template. Replies are cached in `pa-ai-cache`.
import { api, assistantName, getState, hasStats, today } from "../core";
import type { BriefingPrep, DigestPrep, MorningParts, NewsPrep, Weather } from "../types";
import { ctxCache, hasKey } from "./cache";
import { DAILY_BUDGET, isPaused, LOW_BUDGET, remaining, restingNotice, usedToday, VOICE_BUDGET, voiceUsedToday } from "./ai-budget";
import { shouldAutoGenerate, showUpdateButton, showWriteButton, storeKey, type StoredText } from "./ai-policy";
import { mondayOf } from "../util/dates";
import { generate, GeminiError, parseMorningJson, parseNewsJson, setDateProvider } from "./gemini";
import { hasNewerNews, loadNews, newsIsFaithful, newsSettled, updatedLabel } from "./news";
import { weatherSettled } from "./weather";
import { composeWithNews, mergeParts, morningScriptWithNews, readEntry, splitForNews, type AiParts } from "./morning-script";
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
  /** the news segment inside this summary (null/absent when there is none) */
  news?: NewsView | null;
  /** `text` split around the news segment: what comes before it and after it */
  layout?: { before: string; after: string };
}

/** The news segment as the Home card shows it. */
export interface NewsView {
  text: string;
  source: "gemini" | "template";
  /** short names for the "Sources:" line */
  sources: string[];
  /** "5:45 am": when the news list was fetched */
  updated: string;
  position: number;
  /** a newer news list exists and an AI news-only rewrite is possible (button "Get the latest news") */
  canRefresh: boolean;
}

interface StoredNews {
  text: string;
  /** generated_at of the news list the text was written from */
  at: string;
  sources: string[];
  position: number;
}
const newsKey = (skey: string) => `${skey}:news`;

type Kind = "morning" | "evening" | "weekly";

const MORNING_JSON_NOTE =
  "\n\nOUTPUT FORMAT OVERRIDE: reply with ONLY a JSON object " +
  "{\"greeting\": string, \"weather_comment\": string or null, \"affirmation\": string, \"briefing\": string}. " +
  "No markdown, no code fences. Follow the part descriptions above exactly: \"briefing\" has no greeting and no weather, " +
  "\"weather_comment\" is null if no weather is given, and no part may contain a rain percentage or any count.";
const NEWS_FIELD_NOTE = " Also include the fifth field \"news\": string (the news segment described under NEWS), written in the same request.";
const EVENING_JSON_NOTE =
  "\n\nOUTPUT FORMAT OVERRIDE: reply with ONLY a JSON object {\"briefing\": string, \"news\": string}, no markdown, no code fences. " +
  "\"briefing\" is the wind-down in three paragraphs separated by a blank line, and \"news\" is the news segment described under NEWS. No rain percentage or count in \"briefing\".";

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
function result(kind: Kind, prep: BriefingPrep, source: "gemini" | "template", ai: AiParts | null, extra: Partial<BriefingResult> & { storedNews?: StoredNews | null } = {}): BriefingResult {
  const { storedNews, ...rest } = extra;
  const tpl = kind === "morning" ? templateParts(prep) : undefined;
  const base = { source, prep, outdated: false, canWrite: false, ...rest };
  // the news: the AI text when one was written for this summary, else the template segment from the core
  const np = prep.news ?? null;
  const ownText = storedNews?.text?.trim();
  const view: NewsView | null =
    ownText || np
      ? {
          text: ownText || np!.template_text,
          source: ownText ? "gemini" : "template",
          sources: ownText ? storedNews!.sources : np!.sources,
          updated: updatedLabel(ownText ? storedNews!.at : np!.generated_at, getState().settings.timezone || undefined),
          position: ownText ? storedNews!.position : np!.position,
          canRefresh: hasKey() && (kind === "morning" || kind === "evening") && hasNewerNews(ownText ? storedNews!.at : undefined, ctxCache.news.generated_at),
        }
      : null;
  const pos = view?.position ?? 2;
  const withNews = (text: string) => {
    const [before, after] = view ? splitForNews(text, pos) : [text, ""];
    return { news: view, layout: { before, after } };
  };
  if (!tpl) {
    const text = ai?.briefing || (np ? prep.template_body || prep.template_text : prep.template_text);
    // an old template_text may already contain the news (no template_body): then show it as plain text
    const embedded = !!np && !prep.template_body && !ai?.briefing;
    if (embedded) return { ...base, text, script: text };
    return { ...base, text, script: view ? composeWithNews(text, view.text, pos) : text, ...withNews(text) };
  }
  const parts = mergeParts(ai, tpl);
  return { ...base, text: parts.briefing, parts, script: morningScriptWithNews(parts, view?.text, pos), weather_comment: parts.weather_comment, ...withNews(parts.briefing) };
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

const readNews = async (skey: string): Promise<StoredNews | null> => {
  try {
    const v = await kvGet<StoredNews>(DB, newsKey(skey));
    return v && typeof v.text === "string" && v.text.trim() ? v : null;
  } catch {
    return null;
  }
};

/** Remember which stories this summary used (so they are never repeated for ~48 hours and re-viewing doesn't re-pick). */
function recordNews(kind: Kind, date: string, news: NewsPrep | null | undefined): void {
  if (!news || kind === "weekly" || date !== today() || news.recorded || !news.items.length) return;
  try {
    api("POST", "/api/news/record", { kind, date, items: news.items, generated_at: new Date().toISOString() });
  } catch {
    // not fatal: the next summary may repeat a story
  }
}

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
  if (!hasKey()) {
    return (async () => {
      if (kind !== "weekly") await newsSettled();
      const prep = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
      recordNews(kind, date, prep.news);
      return result(kind, prep, "template", null);
    })();
  }
  return once(GEN_KEY(`${skey}:${opts.force ? "f" : "a"}`), async () => {
    let stored = await readStored(skey, early.cache_key);
    const storedNews = await readNews(skey);
    if (stored && !opts.force) return result(kind, early, "gemini", stored.ai, { ...flags(stored, early), storedNews });
    if (!stored && !opts.force && !shouldAutoGenerate(kind, null, { currentWeek })) {
      return result(kind, early, "template", null, flags(null, early));
    }
    // Let the first forecast and news arrive so the first request is made once, with them in it. Later refreshes never trigger anything.
    if (!stored && kind !== "weekly") await Promise.all([weatherSettled(), newsSettled()]);
    const prep = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
    const fallback = (notice?: string): BriefingResult => {
      // the template news is shown too: remember it so tomorrow's summary doesn't repeat it
      recordNews(kind, date, prep.news);
      return stored ? result(kind, prep, "gemini", stored.ai, { ...flags(stored, prep), notice, storedNews }) : result(kind, prep, "template", null, { ...flags(null, prep), notice });
    };
    if (!opts.force && recentlyFailed(skey)) return fallback();
    const d = today();
    if (isPaused(Date.now()) || remaining(d) <= 0) return fallback(aiNotice() ?? "AI writing isn't available right now.");
    try {
      const morning = kind === "morning";
      const hasNews = !!prep.news;
      const jsonMode = morning || (kind === "evening" && hasNews);
      const note = morning ? MORNING_JSON_NOTE + (hasNews ? NEWS_FIELD_NOTE : "") : kind === "evening" && hasNews ? EVENING_JSON_NOTE : "";
      const system = prep.system_prompt + note;
      const ask = async (extra: string): Promise<AiParts | null> => {
        const raw = await generate({ system, user: prep.user_prompt + extra, json: jsonMode });
        if (!jsonMode) return { briefing: raw };
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
        // the news is kept apart from the plan text; it counts only if it quotes the headlines word for word
        let own: StoredNews | null = null;
        if (prep.news && r.news && newsIsFaithful(r.news, prep.news.items.map((i) => i.title))) {
          own = { text: r.news, at: prep.news.generated_at, sources: prep.news.sources, position: prep.news.position };
          await kvSet(DB, newsKey(skey), own);
        }
        recordNews(kind, date, prep.news);
        return result(kind, prep, "gemini", r, { storedNews: own ?? storedNews });
      }
    } catch (e) {
      if (e instanceof GeminiError && e.blocked) return fallback(aiNotice() ?? "AI writing isn't available right now.");
    }
    failedAt.set(skey, Date.now());
    return fallback("Couldn't write that just now. Try again later.");
  });
}

/**
 * "Get the latest news": one AI request that rewrites only the news segment, and only when the news list has newer items
 * and the budget allows. Returns a notice for a toast when nothing was done.
 */
export async function refreshNews(kind: "morning" | "evening", date: string): Promise<{ done: boolean; notice?: string }> {
  if (!hasKey()) return { done: false, notice: "Add your Gemini key in Settings to get the news written up." };
  const d = today();
  if (isPaused(Date.now()) || remaining(d) <= 0) return { done: false, notice: aiNotice() ?? "AI writing isn't available right now." };
  const skey = storeKey(kind, date, mondayOf(date));
  const old = await readNews(skey);
  const file = await loadNews();
  if (!hasNewerNews(old?.at, file.generated_at)) return { done: false, notice: "There's nothing newer yet." };
  const sel = api<NewsPrep | null>("POST", "/api/news/select", { kind, date, refresh: true });
  if (!sel || !sel.items.length) return { done: false, notice: "No new stories since the last ones." };
  try {
    const raw = await generate({ system: sel.system_prompt, user: sel.user_prompt, json: true });
    const text = parseNewsJson(raw);
    if (!text || !newsIsFaithful(text, sel.items.map((i) => i.title))) return { done: false, notice: "Couldn't write the news up just now. Try again later." };
    await kvSet(DB, newsKey(skey), { text, at: sel.generated_at, sources: sel.sources, position: sel.position } satisfies StoredNews);
    recordNews(kind, date, { ...sel, recorded: false });
    return { done: true };
  } catch (e) {
    if (e instanceof GeminiError && e.blocked) return { done: false, notice: aiNotice() ?? "AI writing isn't available right now." };
    return { done: false, notice: "Couldn't write the news up just now. Try again later." };
  }
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
