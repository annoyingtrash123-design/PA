// Why the built-in summary was shown instead of the AI one, and the small muted line under the "Your day" card.
// Pure helpers (plus a tiny localStorage note so the reason survives a reload); no network.
import { clockText } from "./ai-budget";

export type WhyCode = "no-key" | "budget" | "paused" | "network" | "timeout" | "http" | "parse" | "cut-off" | "stats" | "empty" | "headlines" | "other";
export interface Why {
  code: WhyCode;
  /** short phrase for the label: "AI reply couldn't be read" */
  text: string;
}

interface ErrLike {
  message?: string;
  status?: number;
  quota?: boolean;
  blocked?: boolean;
  pausedUntil?: number;
}

export const whyNoKey = (): Why => ({ code: "no-key", text: "add a Gemini key in Settings to turn on AI writing" });
export const whyBudget = (): Why => ({ code: "budget", text: "today's AI allowance is used up" });
export const whyPaused = (until: number, tz?: string): Why => ({ code: "paused", text: `Google's limit is resting until ${clockText(until, tz)}` });
export const whyParse = (): Why => ({ code: "parse", text: "AI reply couldn't be read" });
export const whyCutOff = (): Why => ({ code: "cut-off", text: "AI reply was cut off" });
export const whyStats = (): Why => ({ code: "stats", text: "AI reply read out counts" });
export const whyHeadlines = (): Why => ({ code: "headlines", text: "the AI reworded the headlines" });

/** Classify a failed request (a GeminiError, or anything thrown by fetch). */
export function whyFromError(e: unknown, tz?: string): Why {
  const x = (e ?? {}) as ErrLike;
  const msg = String(x.message ?? "");
  if (/no gemini key/i.test(msg)) return whyNoKey();
  if (x.pausedUntil && (x.blocked || x.quota)) return whyPaused(x.pausedUntil, tz);
  if (x.blocked) return whyBudget();
  if (x.quota) return { code: "paused", text: "Google's limit is resting for now" };
  if (x.status === 503 || x.status === 500 || x.status === 502 || x.status === 504) return { code: "http", text: "Google's AI was too busy just now (it happens at peak times); tap 'Try again' in a minute" };
  if (typeof x.status === "number") return { code: "http", text: `Google replied with an error (${x.status})` };
  if (/took too long/i.test(msg)) return { code: "timeout", text: "Google took too long to answer" };
  if (/couldn't reach/i.test(msg)) return { code: "network", text: "couldn't reach Google (no connection at that moment)" };
  if (/empty reply/i.test(msg)) return { code: "empty", text: "Google sent an empty reply" };
  return { code: "other", text: "the AI request didn't work" };
}

/** "Written by Miranda's AI" or "Built-in summary — <reason>". `newsWhy` adds a note when only the news segment is built in. */
export function sourceLabel(source: "gemini" | "template", why: Why | null | undefined, name = "Miranda", newsWhy?: string | null): string {
  if (source === "gemini") return newsWhy ? `Written by ${name}'s AI · the news part is the built-in version (${newsWhy})` : `Written by ${name}'s AI`;
  return why?.text ? `Built-in summary — ${why.text}` : "Built-in summary";
}

// ---- remembered for a reload (best effort)
const KEY = "pa-ai-why";
type Notes = Record<string, Why | string>;
function read(): Notes {
  try {
    const j = JSON.parse(localStorage.getItem(KEY) || "null");
    return j && typeof j === "object" ? j : {};
  } catch {
    return {};
  }
}
function write(n: Notes) {
  try {
    const keys = Object.keys(n);
    for (const k of keys.slice(0, Math.max(0, keys.length - 12))) delete n[k];
    localStorage.setItem(KEY, JSON.stringify(n));
  } catch {
    // ignore
  }
}
export function rememberWhy(skey: string, why: Why | null): void {
  const n = read();
  if (why) n[skey] = why;
  else delete n[skey];
  write(n);
}
export function recallWhy(skey: string): Why | null {
  const v = read()[skey];
  return v && typeof v === "object" && typeof v.text === "string" ? v : null;
}
export function rememberNewsWhy(skey: string, text: string | null): void {
  const n = read();
  if (text) n[`${skey}:news`] = text;
  else delete n[`${skey}:news`];
  write(n);
}
export function recallNewsWhy(skey: string): string | null {
  const v = read()[`${skey}:news`];
  return typeof v === "string" ? v : null;
}
