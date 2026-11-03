// The news list: `news.json` is published next to the app by a scheduled GitHub Action (feeds can't be read from a
// browser). Network first, falling back to the last copy kept in IndexedDB. The list is handed to the core through ctx.
import type { NewsItem } from "../types";
import { ctxCache } from "./cache";
import { kvGet, kvSet } from "./idb";

const DB = "pa-news";
const KEY = "latest";

export interface NewsFile {
  generated_at: string;
  items: NewsItem[];
}

const str = (v: unknown): string => (typeof v === "string" ? v.trim() : "");

/** Tolerant reader for news.json: drops anything without a title; never throws. */
export function parseNewsFile(raw: unknown): NewsFile {
  const o = raw && typeof raw === "object" ? (raw as Record<string, unknown>) : {};
  const list = Array.isArray(o.items) ? o.items : [];
  const items: NewsItem[] = [];
  for (const it of list) {
    if (!it || typeof it !== "object") continue;
    const r = it as Record<string, unknown>;
    const title = str(r.title);
    if (!title) continue;
    items.push({
      id: str(r.id) || title,
      source: str(r.source),
      title,
      summary: str(r.summary ?? r.lead).slice(0, 800),
      link: str(r.link),
      published: str(r.published),
      topics: Array.isArray(r.topics) ? r.topics.filter((t): t is string => typeof t === "string") : [],
    });
  }
  return { generated_at: str(o.generated_at), items };
}

let resolveFirst: () => void = () => {};
const settled: Promise<void> = new Promise((r) => (resolveFirst = r));
/** Resolves once the first news fetch has finished (or after `ms`), so the first summary sees the news. */
export const newsSettled = (ms = 6000): Promise<void> => Promise.race([settled, new Promise<void>((r) => setTimeout(r, ms))]);

/** Fetch news.json (network first), else the last stored copy. Updates the context the core reads. */
export async function loadNews(): Promise<NewsFile> {
  let file: NewsFile | null = null;
  try {
    const ctl = new AbortController();
    const timer = setTimeout(() => ctl.abort(), 8000);
    try {
      const r = await fetch(`${import.meta.env.BASE_URL}news.json`, { cache: "no-cache", signal: ctl.signal });
      if (r.ok) file = parseNewsFile(await r.json());
    } finally {
      clearTimeout(timer);
    }
    if (file && file.generated_at) await kvSet(DB, KEY, file).catch(() => {});
  } catch {
    file = null;
  }
  if (!file || !file.generated_at) {
    const kept = await kvGet<unknown>(DB, KEY).catch(() => undefined);
    const stored = kept ? parseNewsFile(kept) : null;
    // a fetched file with no timestamp (an empty fallback) never beats a stored copy that has items
    if (stored && stored.items.length && (!file || !file.items.length)) file = stored;
  }
  ctxCache.news = file ?? { generated_at: "", items: [] };
  resolveFirst();
  return ctxCache.news;
}

/** Quotes, case, accents, punctuation and spacing don't matter when checking that a headline was quoted. */
export const normHeadline = (s: string): string =>
  s
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[‘’‛′`]/g, "'")
    .replace(/[“”„″]/g, '"')
    .replace(/&/g, " and ")
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();

/** How many of the headlines appear word for word (after normalising) in the spoken news text. */
export function headlinesQuoted(news: string, titles: string[]): number {
  const n = ` ${normHeadline(news)} `;
  return titles.filter((t) => t.trim() && n.includes(` ${normHeadline(t)} `)).length;
}

/** The AI news is accepted when it quotes at least half of the headlines exactly (else the template news is used). */
export const newsIsFaithful = (news: string, titles: string[]): boolean => titles.length > 0 && headlinesQuoted(news, titles) * 2 >= titles.length;

/** "5:45 am" in the user's time zone for an ISO time, or "" when it can't be read. */
export function updatedLabel(iso: string, timeZone?: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "";
  try {
    return new Intl.DateTimeFormat("en-AU", { hour: "numeric", minute: "2-digit", hour12: true, timeZone }).format(new Date(t)).replace(/\s?([ap])m/i, (_, x: string) => ` ${x.toLowerCase()}m`);
  } catch {
    return "";
  }
}

/** True when the news list is newer than the news a summary was written from. */
export function hasNewerNews(storedAt: string | undefined, currentAt: string | undefined): boolean {
  const c = Date.parse(currentAt ?? "");
  if (!Number.isFinite(c)) return false;
  const s = Date.parse(storedAt ?? "");
  return !Number.isFinite(s) || c > s;
}

/** "Sources: ABC, BBC and Guardian" */
export const sourcesLine = (sources: string[]): string => (sources.length ? `Sources: ${sources.join(", ")}` : "");
