// In-memory caches the core reads synchronously when building its per-call context.
import type { Holiday, Weather } from "../types";

export const ctxCache: { weather: Record<string, Weather>; holidays: Holiday[] } = { weather: {}, holidays: [] };

export const KEY_STORAGE = "pa-gemini-key";
export const MODEL_STORAGE = "pa-gemini-model";
export const DEFAULT_MODEL = "gemini-3.8-flash";
/** Models Google has retired for new users; a saved one falls back to the default. */
const RETIRED_MODELS = ["gemini-2.5-flash", "gemini-2.0-flash", "gemini-1.5-flash"];

const ls = {
  get(k: string): string {
    try {
      return localStorage.getItem(k) ?? "";
    } catch {
      return "";
    }
  },
  set(k: string, v: string) {
    try {
      if (v) localStorage.setItem(k, v);
      else localStorage.removeItem(k);
    } catch {
      // ignore
    }
  },
};

export const getKey = () => ls.get(KEY_STORAGE).trim();
export const setKey = (v: string) => ls.set(KEY_STORAGE, v.trim());
export const hasKey = () => getKey() !== "";
export const getModel = () => {
  const m = ls.get(MODEL_STORAGE).trim();
  return m && !RETIRED_MODELS.includes(m) ? m : DEFAULT_MODEL;
};
export const setModel = (v: string) => ls.set(MODEL_STORAGE, v.trim());
