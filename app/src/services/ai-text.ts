// Assistant texts: ask the core what to write (prompts + template), ask Gemini if a key is saved,
// guard the reply (no raw stats), retry once, and fall back to the template. Replies are cached in `pa-ai-cache`.
import { api, hasStats } from "../core";
import type { BriefingPrep, DigestPrep, Weather } from "../types";
import { hasKey } from "./cache";
import { generate, parseDigestJson } from "./gemini";
import { kvGet, kvSet } from "./idb";

const DB = "pa-ai-cache";
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
}

export function getBriefing(kind: "morning" | "evening" | "weekly", date: string): Promise<Briefing> {
  const prep = api<BriefingPrep>("GET", `/api/briefing?kind=${kind}&date=${date}`);
  const template: Briefing = { text: prep.template_text, source: "template", prep };
  if (!hasKey()) return Promise.resolve(template);
  return once(`b:${prep.cache_key}`, async () => {
    const cached = await kvGet<string>(DB, prep.cache_key);
    if (cached) return { text: cached, source: "gemini" as const, prep };
    if (recentlyFailed(prep.cache_key)) return template;
    try {
      let reply = await generate({ system: prep.system_prompt, user: prep.user_prompt });
      if (hasStats(reply)) reply = await generate({ system: prep.system_prompt, user: prep.user_prompt + RETRY_NOTE });
      if (reply && !hasStats(reply)) {
        await kvSet(DB, prep.cache_key, reply);
        return { text: reply, source: "gemini" as const, prep };
      }
    } catch {
      // fall through to the template
    }
    failedAt.set(prep.cache_key, Date.now());
    return template;
  });
}

export interface Digest {
  weather: Weather | null;
  weather_comment: string | null;
  overview: string;
  source: "gemini" | "template";
}

export function getDigest(date: string): Promise<Digest> {
  const prep = api<DigestPrep>("GET", `/api/digest?date=${date}`);
  const template: Digest = { weather: prep.weather, weather_comment: prep.template.weather_comment, overview: prep.template.overview, source: "template" };
  if (!hasKey()) return Promise.resolve(template);
  return once(`d:${prep.cache_key}`, async () => {
    const cached = await kvGet<{ weather_comment: string | null; overview: string }>(DB, prep.cache_key);
    if (cached) return { weather: prep.weather, ...cached, source: "gemini" as const };
    if (recentlyFailed(prep.cache_key)) return template;
    try {
      for (const note of ["", RETRY_NOTE]) {
        const raw = await generate({ system: prep.system_prompt, user: prep.user_prompt + note, json: true });
        const parsed = parseDigestJson(raw);
        if (parsed && !hasStats(parsed.overview)) {
          await kvSet(DB, prep.cache_key, parsed);
          return { weather: prep.weather, ...parsed, source: "gemini" as const };
        }
      }
    } catch {
      // fall through
    }
    failedAt.set(prep.cache_key, Date.now());
    return template;
  });
}

/** Ask Gemini to pick a category for an auto-filed task; silently keeps the keyword guess on any failure. */
export async function refineCategory(taskId: number): Promise<string | null> {
  if (!hasKey()) return null;
  try {
    const p = api<{ system_prompt: string; user_prompt: string; names: string[] }>("GET", `/api/tasks/${taskId}/categorise-prompt`);
    const reply = (await generate({ system: p.system_prompt, user: p.user_prompt, maxTokens: 64 })).trim().replace(/^["'.\s]+|["'.\s]+$/g, "");
    const name = p.names.find((n) => n.toLowerCase() === reply.toLowerCase());
    if (!name) return null;
    api("PUT", `/api/tasks/${taskId}/category`, { name, auto: true });
    return name;
  } catch {
    return null;
  }
}

export async function testKey(key: string, model: string): Promise<string> {
  return generate({ system: "You are a friendly assistant.", user: "Reply with exactly: Hello, your key works.", key, model, maxTokens: 32 });
}
