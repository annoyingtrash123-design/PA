// The morning briefing is four spoken parts (greeting, weather, affirmation, briefing). Pure helpers: merge AI parts
// with the template parts, read old cache entries, and build the one combined read-aloud script. No browser, no network.
import type { MorningParts } from "../types";
import { paragraphs } from "../util/text";

/** What the AI (or an old cache entry) supplied; every field may be missing. */
export interface AiParts {
  greeting?: string | null;
  weather_comment?: string | null;
  affirmation?: string | null;
  briefing?: string | null;
  /** the news segment (morning/evening reply field; stored separately) */
  news?: string | null;
  /** an old entry: `briefing` is the whole text, with its own hello and weather inside */
  legacy?: boolean;
}

const clean = (s: unknown): string => (typeof s === "string" ? s.trim() : "");

/** AI parts where present, template parts for anything missing. A legacy text has its own hello, so no separate greeting. */
export function mergeParts(ai: AiParts | null | undefined, tpl: MorningParts): MorningParts {
  if (!ai) return tpl;
  const wc = clean(ai.weather_comment) || tpl.weather_comment;
  return {
    greeting: ai.legacy ? "" : clean(ai.greeting) || tpl.greeting,
    weather_comment: wc || null,
    affirmation: clean(ai.affirmation) || tpl.affirmation,
    briefing: clean(ai.briefing) || tpl.briefing,
  };
}

/** Read a stored cache value: a plain string (oldest), an object without the new fields (old), or the 4-part object. */
export function readEntry(v: unknown): (AiParts & { fingerprint?: string }) | null {
  if (!v) return null;
  if (typeof v === "string") return { briefing: v, legacy: true };
  if (typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  const briefing = clean(o.briefing);
  if (!briefing) return null;
  const isNew = "affirmation" in o || "greeting" in o;
  return {
    briefing,
    greeting: clean(o.greeting) || null,
    weather_comment: clean(o.weather_comment) || null,
    affirmation: clean(o.affirmation) || null,
    legacy: !isNew,
    fingerprint: typeof o.fingerprint === "string" ? o.fingerprint : undefined,
  };
}

/** Everything the morning "Read it to me" says, in order, separated by blank lines (= short pauses). */
export function morningScript(p: MorningParts): string {
  return [p.greeting, p.weather_comment, p.affirmation, p.briefing]
    .map(clean)
    .filter(Boolean)
    .join("\n\n");
}

// ---------------------------------------------------------------- news inside the spoken flow
// Mirrors core/src/news.rs (split_body / compose): the news sits at a position that varies by day:
// 0 = right after the greeting, weather and affirmation; 1 = after the first paragraph of the briefing (the day by theme);
// 2 = before the last paragraph (the closing line).

/** Split a body at the news position into [before, after] (blank-line separated paragraphs). */
export function splitForNews(body: string, pos: number): [string, string] {
  const paras = paragraphs(body);
  const n = paras.length;
  const cut = pos === 0 ? 0 : pos === 1 && n >= 2 ? 1 : n >= 2 ? n - 1 : n;
  return [paras.slice(0, cut).join("\n\n"), paras.slice(cut).join("\n\n")];
}

/** The body with the news inserted at its position. */
export function composeWithNews(body: string, news: string | null | undefined, pos: number): string {
  const n = clean(news);
  if (!n) return body;
  const [a, b] = splitForNews(body, pos);
  return [a, n, b].filter(Boolean).join("\n\n");
}

/** The morning script: greeting, weather, affirmation, then the briefing with the news inserted at its position. */
export function morningScriptWithNews(p: MorningParts, news: string | null | undefined, pos: number): string {
  return morningScript({ ...p, briefing: composeWithNews(p.briefing, news, pos) });
}

// ---------------------------------------------------------------- the three spoken sections
// Each briefing is recorded as three clips ("sections") that are played back-to-back. On screen it stays one text.
// Morning: 1 = greeting, weather, affirmation (+ the news when it sits first); 2 = the day (first two paragraphs of the
// briefing); 3 = the rest (tomorrow, the news when it sits last, the closing). Evening: 1 = the look back, 2 = tomorrow,
// 3 = the news (when present) and the closing.

/** News position 1 (older cached summaries) is treated as "last": the news never splits the middle section. */
export const normalisePos = (pos: number): number => (pos === 0 ? 0 : 2);

const join2 = (xs: string[]) => xs.map(clean).filter(Boolean).join("\n\n");

/** Spread paragraphs over at most three sections: [first], [second], [rest]. */
function spread(paras: string[]): string[][] {
  const n = paras.length;
  if (n <= 1) return [paras];
  if (n === 2) return [[paras[0]], [paras[1]]];
  return [[paras[0]], [paras[1]], paras.slice(2)];
}

export function morningSections(p: MorningParts, news: string | null | undefined, pos: number): string[] {
  const nw = clean(news);
  const paras = paragraphs(p.briefing);
  const first = [p.greeting, p.weather_comment, p.affirmation].map(clean).filter(Boolean);
  const day = paras.slice(0, Math.min(2, Math.max(1, paras.length - 1)));
  const rest = paras.slice(day.length);
  if (nw && normalisePos(pos) === 0) first.push(nw);
  if (nw && normalisePos(pos) === 2) rest.splice(Math.max(0, rest.length - 1), 0, nw);
  return [join2(first), join2(day), join2(rest)].filter(Boolean);
}

export function eveningSections(body: string, news: string | null | undefined): string[] {
  const nw = clean(news);
  const [a, b, c] = spread(paragraphs(body));
  const last = [...(nw ? [nw] : []), ...(c ?? [])];
  return [join2(a ?? []), join2(b ?? []), join2(last)].filter(Boolean);
}
