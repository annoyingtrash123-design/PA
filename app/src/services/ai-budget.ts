// Gemini usage guard: a daily request budget, quota pauses ("retry in 5h48m34s"), and reuse rules.
// Everything takes an explicit store/date/clock so it can be unit tested without a browser.

export interface Store {
  get(k: string): string;
  set(k: string, v: string): void;
}

export const lsStore: Store = {
  get(k) {
    try {
      return localStorage.getItem(k) ?? "";
    } catch {
      return "";
    }
  },
  set(k, v) {
    try {
      if (v) localStorage.setItem(k, v);
      else localStorage.removeItem(k);
    } catch {
      // ignore
    }
  },
};

export const DAILY_BUDGET = 12;
/** At or below this many requests left we skip optional retries. */
export const LOW_BUDGET = 3;
export const USAGE_KEY = "pa-ai-usage";
export const PAUSED_KEY = "pa-ai-paused-until";
export const MAIN_PAUSED_KEY = "pa-ai-main-paused-until";
export const TTS_PAUSED_KEY = "pa-tts-paused-until";

// ---- daily budget (counter per local date)
export function usedToday(date: string, store: Store = lsStore): number {
  try {
    const j = JSON.parse(store.get(USAGE_KEY) || "null");
    if (j && j.date === date && Number.isFinite(j.count)) return Math.max(0, j.count);
  } catch {
    // fall through
  }
  return 0;
}
export const remaining = (date: string, store: Store = lsStore): number => Math.max(0, DAILY_BUDGET - usedToday(date, store));
export function recordUse(date: string, store: Store = lsStore): number {
  const count = usedToday(date, store) + 1;
  store.set(USAGE_KEY, JSON.stringify({ date, count }));
  return count;
}

// ---- quota pauses
/** Parse "Please retry in 5h48m34.5s" (any of h/m/s) into milliseconds; null if absent. */
export function parseRetryInMs(msg: string): number | null {
  const m = /retry in\s+(?:(\d+)\s*h)?\s*(?:(\d+)\s*m(?!s))?\s*(?:(\d+(?:\.\d+)?)\s*s)?/i.exec(msg);
  if (!m || (m[1] === undefined && m[2] === undefined && m[3] === undefined)) return null;
  return Math.round(((Number(m[1] ?? 0) * 60 + Number(m[2] ?? 0)) * 60 + Number(m[3] ?? 0)) * 1000);
}

/** The next midnight in US Pacific time (when Google resets free-tier daily quotas), as epoch ms. */
export function nextPacificMidnight(nowMs: number): number {
  const parts = new Intl.DateTimeFormat("en-US", { timeZone: "America/Los_Angeles", hourCycle: "h23", hour: "numeric", minute: "numeric", second: "numeric" }).formatToParts(new Date(nowMs));
  const n = (t: string) => Number(parts.find((p) => p.type === t)?.value ?? 0);
  const elapsed = (n("hour") % 24) * 3600 + n("minute") * 60 + n("second");
  return nowMs + (86400 - elapsed) * 1000;
}

/** When to resume after a quota error message: its "retry in" hint, else the next Pacific midnight. */
export function pauseUntilFor(msg: string, nowMs: number): number {
  const ms = parseRetryInMs(msg);
  return ms !== null ? nowMs + ms + 1000 : nextPacificMidnight(nowMs);
}

const getMs = (k: string, store: Store) => Number(store.get(k)) || 0;
export const getPausedUntil = (store: Store = lsStore) => getMs(PAUSED_KEY, store);
export const setPausedUntil = (ms: number, store: Store = lsStore) => store.set(PAUSED_KEY, String(Math.round(ms)));
/** True while no Gemini model may be called. */
export const isPaused = (nowMs: number, store: Store = lsStore) => getPausedUntil(store) > nowMs;
/** The main model alone is out of quota (the lite model may still work). */
export const getMainPausedUntil = (store: Store = lsStore) => getMs(MAIN_PAUSED_KEY, store);
export const setMainPausedUntil = (ms: number, store: Store = lsStore) => store.set(MAIN_PAUSED_KEY, String(Math.round(ms)));
export const isMainPaused = (nowMs: number, store: Store = lsStore) => getMainPausedUntil(store) > nowMs;

/** The natural-voice (TTS) models alone are out of quota; text writing may still work. */
export const getTtsPausedUntil = (store: Store = lsStore) => getMs(TTS_PAUSED_KEY, store);
export const setTtsPausedUntil = (ms: number, store: Store = lsStore) => store.set(TTS_PAUSED_KEY, String(Math.round(ms)));
/** When the natural voice may be used again (0 = now): the later of the global and the TTS-only pause. */
export const ttsBlockedUntil = (nowMs: number, store: Store = lsStore) => {
  const u = Math.max(getPausedUntil(store), getTtsPausedUntil(store));
  return u > nowMs ? u : 0;
};

export function formatDuration(ms: number): string {
  const mins = Math.max(1, Math.ceil(ms / 60000));
  const h = Math.floor(mins / 60);
  const m = mins % 60;
  if (h === 0) return `${m}m`;
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}

export const quotaMessage = (untilMs: number, nowMs: number) => `Google's free daily limit is used up. It resets in about ${formatDuration(untilMs - nowMs)}.`;

/** "6:59 pm" in the given IANA timezone. */
export function clockText(ms: number, timeZone?: string): string {
  let s: string;
  try {
    s = new Date(ms).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone });
  } catch {
    s = new Date(ms).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
  }
  return s.replace(/\s?([AP])M$/i, (_, a: string) => ` ${a.toLowerCase()}m`);
}

/** The friendly line shown under the home summary when the AI is resting, or null when it is not. */
export function restingNotice(name: string, date: string, nowMs: number, timeZone?: string, store: Store = lsStore): string | null {
  const until = getPausedUntil(store);
  if (until > nowMs) return `${name}'s AI writing is resting until about ${clockText(until, timeZone)} — this is the built-in summary.`;
  if (remaining(date, store) <= 0) return `${name} has used today's AI writing allowance — this is the built-in summary.`;
  return null;
}
