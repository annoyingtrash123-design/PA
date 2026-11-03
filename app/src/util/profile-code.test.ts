import { describe, expect, it } from "vitest";
// @ts-expect-error plain JS module without types
import * as script from "../../scripts/make-profile-code.mjs";
import { decodeProfile, encodeProfile, encodeProfileV2, ProfileCodeError, summarize, type Profile } from "./profile-code";

const sample: Profile = {
  v: 1,
  updated: "2026-11-03",
  interests: "Pottery, tide pools, café culture ☕ and 日本の歴史",
  notes: ["Example note — with “quotes” and emoji 🌱"],
  work: { enabled: true, days: [0, 1, 2], start: "09:00", end: "15:30", lunch_minutes: 45, lunch_window_start: "12:00", lunch_window_end: "13:30", holiday_region: "AU-NSW" },
  weather: { comfortable_max: 19, hot_above: 23, too_hot_above: 27, likes_rain: true, avoid_sunburn: false },
  add_goals: [{ title: "Learn pottery", horizon: "medium", why: "fun", deadline: "2027-01-31" }],
  add_leisure: [{ kind: "read", title: "A Made-up Book" }],
  add_tasks: [{ title: "Example task", date: "2026-11-10", time: "14:00", category: "auto", checklist: ["one", "two"] }],
};

/** Produced by `node scripts/make-profile-code.mjs` (zlib deflateRaw) from `sample`; the app must decode it with DecompressionStream. */
const FIXTURE_V2 = script.encodeProfile(sample) as string;

describe("profile code", () => {
  it("round-trips including unicode", async () => {
    const code = encodeProfile(sample);
    expect(code).toMatch(/^MIRANDA1:[A-Za-z0-9_-]+$/);
    expect(await decodeProfile(code)).toEqual(sample);
    const v2 = await encodeProfileV2(sample);
    expect(v2).toMatch(/^MIRANDA2:[A-Za-z0-9_-]+$/);
    expect(v2.length).toBeLessThan(code.length);
    expect(await decodeProfile(v2)).toEqual(sample);
  });
  it("tolerates wrapped whitespace and surrounding words", async () => {
    const code = encodeProfile(sample);
    const wrapped = code.replace(/(.{20})/g, "$1\n  ").replace("MIRANDA1:", "\n MIRANDA1: ");
    expect(await decodeProfile(wrapped)).toEqual(sample);
    const wrapped2 = script.wrap(FIXTURE_V2.slice(9));
    expect(await decodeProfile(`Here is your code:\nMIRANDA2:${wrapped2}\n\nThanks, enjoy!`)).toEqual(sample);
    expect(await decodeProfile(`Sure! ${encodeProfile(sample)} done`)).toEqual(sample);
  });
  it("rejects bad input", async () => {
    await expect(decodeProfile("hello")).rejects.toThrow(ProfileCodeError);
    await expect(decodeProfile("MIRANDA1:@@@")).rejects.toThrow(ProfileCodeError);
    await expect(decodeProfile("MIRANDA1:abc")).rejects.toThrow(ProfileCodeError);
    await expect(decodeProfile("MIRANDA2:abcd")).rejects.toThrow(ProfileCodeError);
    expect(() => encodeProfile({ v: 1, updated: "yesterday" } as Profile)).toThrow(ProfileCodeError);
    expect(() => encodeProfile({ ...sample, notes: [1] } as unknown as Profile)).toThrow(ProfileCodeError);
  });
  it("summarises", () => {
    expect(summarize({ interests: true, notes: 1, work: false, weather: false, goals_added: 11, leisure_added: 1 })).toBe("Profile updated · 11 goals added · 1 leisure item added");
    expect(summarize({ interests: false, notes: 0, work: false, weather: false, goals_added: 0, leisure_added: 0 })).toBe("Profile updated");
    expect(summarize({ interests: false, notes: 0, work: false, weather: false, goals_added: 2, leisure_added: 0, tasks_added: 2 })).toBe("Profile updated · 2 goals added · 2 tasks added");
  });
  it("script v1 matches the app encoder; script decodes both", async () => {
    expect(script.encodeProfile(sample, { v1: true })).toBe(encodeProfile(sample));
    expect(script.decodeProfile(encodeProfile(sample))).toEqual(sample);
    expect(script.decodeProfile(FIXTURE_V2)).toEqual(sample);
    const wrapped: string = script.wrap(FIXTURE_V2);
    expect(wrapped.split("\n").every((l: string) => l.length <= 76)).toBe(true);
    expect(wrapped.includes("\n")).toBe(true);
    expect(script.decodeProfile(wrapped)).toEqual(sample);
  });
  it("app decodes script (zlib deflateRaw) output via DecompressionStream", async () => {
    expect(FIXTURE_V2.startsWith("MIRANDA2:")).toBe(true);
    expect(await decodeProfile(FIXTURE_V2)).toEqual(sample);
    expect(await decodeProfile(script.wrap(FIXTURE_V2))).toEqual(sample);
  });
});
