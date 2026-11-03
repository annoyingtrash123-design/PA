import type { Weather } from "../types";

export interface DailyBlock {
  time: string[];
  weather_code: (number | null)[];
  temperature_2m_max: (number | null)[];
  temperature_2m_min: (number | null)[];
  precipitation_probability_max?: (number | null)[];
  uv_index_max?: (number | null)[];
}

/** Turn Open-Meteo's daily arrays into a date -> Weather map. */
export function buildWeatherMap(daily: DailyBlock, summaryFor: (code: number) => string): Record<string, Weather> {
  const out: Record<string, Weather> = {};
  daily.time.forEach((date, i) => {
    const hi = daily.temperature_2m_max[i];
    const lo = daily.temperature_2m_min[i];
    if (hi === null || hi === undefined || lo === null || lo === undefined) return;
    out[date] = {
      summary: summaryFor(daily.weather_code[i] ?? 0),
      temp_max: Math.round(hi),
      temp_min: Math.round(lo),
      precip_prob: Math.round(daily.precipitation_probability_max?.[i] ?? 0),
      uv_max: Math.round((daily.uv_index_max?.[i] ?? 0) * 10) / 10,
    };
  });
  return out;
}
