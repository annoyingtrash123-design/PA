// Voice preferences (kept in localStorage `pa-voice-prefs`: the core's settings only know a fixed set of fields).
import { hasKey } from "./cache";
import { DEFAULT_STYLE, DEFAULT_VOICE, STYLES, VOICES } from "./gemini-tts";

export type VoiceType = "gemini" | "phone";
export interface VoicePrefs {
  type?: VoiceType;
  voice?: string;
  style?: string;
  speed?: number;
  /** favourite Gemini voice names, in the order they were starred */
  favs?: string[];
}
export const VOICE_PREFS_KEY = "pa-voice-prefs";

export function readPrefs(): VoicePrefs {
  try {
    const j = JSON.parse(localStorage.getItem(VOICE_PREFS_KEY) || "null");
    return j && typeof j === "object" ? j : {};
  } catch {
    return {};
  }
}
export function savePrefs(patch: VoicePrefs): void {
  try {
    localStorage.setItem(VOICE_PREFS_KEY, JSON.stringify({ ...readPrefs(), ...patch }));
  } catch {
    // ignore
  }
}

/** Natural voice if the person chose it, or if they never chose and a key is saved. */
export const resolveVoiceType = (p: VoicePrefs, keySaved: boolean): VoiceType => p.type ?? (keySaved ? "gemini" : "phone");
export const getVoiceType = (): VoiceType => resolveVoiceType(readPrefs(), hasKey());
export const getGeminiVoice = (): string => {
  const v = readPrefs().voice;
  return v && VOICES.some((x) => x.name === v) ? v : DEFAULT_VOICE;
};
export const getStyle = (): string => {
  const s = readPrefs().style;
  return s && STYLES.some((x) => x.id === s) ? s : DEFAULT_STYLE;
};

export const SPEED_MIN = 0.7;
export const SPEED_MAX = 1.15;
export const SPEED_STEP = 0.05;
export const DEFAULT_SPEED = 0.88;
/** Clamp to the slider range; anything that isn't a number gives the default (0.88 is deliberately off the slider's grid). */
export const clampSpeed = (v: unknown): number => {
  const n = typeof v === "number" ? v : Number.NaN;
  if (!Number.isFinite(n)) return DEFAULT_SPEED;
  return Math.round(Math.min(SPEED_MAX, Math.max(SPEED_MIN, n)) * 100) / 100;
};
export const getSpeed = (): number => clampSpeed(readPrefs().speed);

// ---- favourite voices
/** Valid favourite names only (known voices, no repeats). */
export const cleanFavs = (v: unknown): string[] => (Array.isArray(v) ? [...new Set(v.filter((x): x is string => typeof x === "string" && VOICES.some((y) => y.name === x)))] : []);
export const getFavourites = (): string[] => cleanFavs(readPrefs().favs);
/** Add or remove a favourite; returns the new list. */
export function toggleFavourite(name: string): string[] {
  const cur = getFavourites();
  const next = cur.includes(name) ? cur.filter((n) => n !== name) : [...cur, name];
  savePrefs({ favs: next });
  return next;
}
/** The voice list for Settings: favourites first (in the order starred), then the rest in the usual order. */
export function orderVoices<T extends { name: string }>(voices: T[], favs: string[]): { favourites: T[]; others: T[] } {
  const favourites = favs.map((n) => voices.find((v) => v.name === n)).filter((v): v is T => !!v);
  return { favourites, others: voices.filter((v) => !favs.includes(v.name)) };
}
