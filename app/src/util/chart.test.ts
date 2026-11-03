import { describe, expect, it } from "vitest";
import { axisTicks } from "./chart";
import { buildWeatherMap } from "./weather-map";

describe("axisTicks", () => {
  it("covers the data with nice steps", () => {
    const a = axisTicks(55, 62.5);
    expect(a.step).toBe(2.5);
    expect(a.yMin).toBeLessThanOrEqual(55);
    expect(a.yMax).toBeGreaterThanOrEqual(62.5);
    expect(a.ticks[0]).toBe(a.yMin);
  });
  it("handles a single value", () => {
    const a = axisTicks(60, 60);
    expect(a.yMin).toBeLessThan(60);
    expect(a.yMax).toBeGreaterThan(60);
  });
  it("never goes below zero", () => {
    expect(axisTicks(1, 2).yMin).toBeGreaterThanOrEqual(0);
  });
});

describe("buildWeatherMap", () => {
  it("maps Open-Meteo arrays to per-day weather and skips days without temperatures", () => {
    const map = buildWeatherMap(
      { time: ["2026-11-03", "2026-11-04"], weather_code: [61, 0], temperature_2m_max: [14.4, null], temperature_2m_min: [8.2, 9], precipitation_probability_max: [80, 0], uv_index_max: [1.46, 8] },
      (c) => (c === 61 ? "Light rain" : "Clear"),
    );
    expect(map).toEqual({ "2026-11-03": { summary: "Light rain", temp_max: 14, temp_min: 8, precip_prob: 80, uv_max: 1.5 } });
  });
});
