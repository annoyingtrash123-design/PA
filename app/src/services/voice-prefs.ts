// Voice preferences (kept in localStorage `pa-voice-prefs`: the core's settings only know a fixed set of fields).
import { hasKey } from "./cache";
import { DEFAULT_STYLE, DEFAULT_VOICE, STYLES, VOICES } from "./gemini-tts";

export type VoiceType = "gemini" | "phone";
export interface VoicePrefs {
  type?: VoiceType;
  voice?: string;
  style?: string;
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
