import { describe, expect, it } from "vitest";
// @ts-expect-error plain JS module without types
import * as script from "../../scripts/make-profile-code.mjs";
import { decodeProfile, encodeProfile, ProfileCodeError, summarize, type Profile } from "./profile-code";

const sample: Profile = {
  v: 1,
  updated: "2026-11-03",
  interests: "Pottery, tide pools, café culture ☕ and 日本の歴史",
  notes: ["Example note — with “quotes” and emoji 🌱"],
  work: { enabled: true, days: [0, 1, 2], start: "09:00", end: "15:30", lunch_minutes: 45, lunch_window_start: "12:00", lunch_window_end: "13:30", holiday_region: "AU-NSW" },
  weather: { comfortable_max: 19, hot_above: 23, too_hot_above: 27, likes_rain: true, avoid_sunburn: false },
  add_goals: [{ title: "Learn pottery", horizon: "medium", why: "fun", deadline: "2027-01-31" }],
  add_leisure: [{ kind: "read", title: "A Made-up Book" }],
};

describe("profile code", () => {
  it("round-trips including unicode", () => {
    const code = encodeProfile(sample);
    expect(code.startsWith("MIRANDA1:")).toBe(true);
    expect(code).toMatch(/^MIRANDA1:[A-Za-z0-9_-]+$/);
    expect(decodeProfile(code)).toEqual(sample);
  });
  it("tolerates wrapped whitespace", () => {
    const code = encodeProfile(sample);
    const wrapped = code.replace(/(.{20})/g, "$1\n  ").replace("MIRANDA1:", "\n MIRANDA1: ");
    expect(decodeProfile(wrapped)).toEqual(sample);
  });
  it("rejects bad input", () => {
    expect(() => decodeProfile("hello")).toThrow(ProfileCodeError);
    expect(() => decodeProfile("MIRANDA1:@@@")).toThrow(ProfileCodeError);
    expect(() => decodeProfile("MIRANDA1:abc")).toThrow(ProfileCodeError);
    expect(() => encodeProfile({ v: 1, updated: "yesterday" } as Profile)).toThrow(ProfileCodeError);
    expect(() => encodeProfile({ ...sample, notes: [1] } as unknown as Profile)).toThrow(ProfileCodeError);
  });
  it("summarises", () => {
    expect(summarize({ interests: true, notes: 1, work: false, weather: false, goals_added: 11, leisure_added: 1 })).toBe("Profile updated · 11 goals added · 1 leisure item added");
    expect(summarize({ interests: false, notes: 0, work: false, weather: false, goals_added: 0, leisure_added: 0 })).toBe("Profile updated");
  });
  it("script produces identical output and decodes", () => {
    expect(script.encodeProfile(sample)).toBe(encodeProfile(sample));
    expect(script.decodeProfile(encodeProfile(sample))).toEqual(sample);
    const wrapped: string = script.wrap(script.encodeProfile(sample));
    expect(wrapped.split("\n").every((l: string) => l.length <= 76)).toBe(true);
    expect(wrapped.includes("\n")).toBe(true);
    expect(decodeProfile(wrapped)).toEqual(sample);
    expect(script.decodeProfile(wrapped)).toEqual(sample);
  });
});
