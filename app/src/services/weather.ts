// Weather from Open-Meteo (no key). Cached in IndexedDB for ~3 hours; the core only ever sees the cached map.
import { api, getState } from "../core";
import type { Weather } from "../types";
import { buildWeatherMap } from "../util/weather-map";
import { ctxCache } from "./cache";
import { kvGet, kvSet } from "./idb";
import { emitContextUpdate } from "./events";

const CACHE_MS = 3 * 60 * 60 * 1000;
const DB = "pa-weather";

interface Cached {
  lat: number;
  lon: number;
  at: number;
  days: Record<string, Weather>;
}

let resolveFirst: () => void = () => {};
let settled: Promise<void> = new Promise((r) => (resolveFirst = r));

/** Resolves once the first forecast fetch has finished (or after `ms`), so AI texts see the weather. */
export function weatherSettled(ms = 4000): Promise<void> {
  return Promise.race([settled, new Promise<void>((r) => setTimeout(r, ms))]);
}

export async function refreshWeather(force = false): Promise<void> {
  try {
    await refreshInner(force);
  } finally {
    resolveFirst();
  }
}

async function refreshInner(force: boolean): Promise<void> {
  const { latitude: lat, longitude: lon } = getState().settings;
  if (typeof lat !== "number" || typeof lon !== "number") return;
  const cached = await kvGet<Cached>(DB, "forecast");
  const fresh = cached && cached.lat === lat && cached.lon === lon && Date.now() - cached.at < CACHE_MS;
  if (cached && cached.lat === lat && cached.lon === lon && !Object.keys(ctxCache.weather).length) {
    ctxCache.weather = cached.days; // even a stale forecast beats nothing while offline
    emitContextUpdate();
  }
  if (fresh && !force) return;
  try {
    const url =
      `https://api.open-meteo.com/v1/forecast?latitude=${lat}&longitude=${lon}` +
      `&daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,uv_index_max` +
      `&timezone=auto&forecast_days=16`;
    const res = await fetch(url);
    if (!res.ok) throw new Error(String(res.status));
    const j = await res.json();
    const memo = new Map<number, string>();
    const summaryFor = (code: number) => {
      if (!memo.has(code)) {
        let s = "";
        try {
          s = api<{ summary: string }>("GET", `/api/weather-summary?code=${code}`).summary;
        } catch {
          // leave empty
        }
        memo.set(code, s);
      }
      return memo.get(code) ?? "";
    };
    const days = buildWeatherMap(j.daily, summaryFor);
    ctxCache.weather = days;
    await kvSet(DB, "forecast", { lat, lon, at: Date.now(), days } satisfies Cached);
    emitContextUpdate();
  } catch {
    // offline or blocked: keep whatever we had; the UI simply hides the weather card
  }
}

/** Forget the forecast (after the location changed) and fetch again. */
export async function resetWeather(): Promise<void> {
  ctxCache.weather = {};
  await kvSet(DB, "forecast", null);
  emitContextUpdate();
  await refreshWeather(true);
}
