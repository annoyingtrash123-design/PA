import { beforeEach, describe, expect, it } from "vitest";
import { clockText, DAILY_BUDGET, formatDuration, getPausedUntil, isPaused, nextPacificMidnight, parseRetryInMs, pauseUntilFor, quotaMessage, recordUse, remaining, restingNotice, setPausedUntil, usedToday, type Store } from "./ai-budget";

const mem = (): Store => {
  const m = new Map<string, string>();
  return { get: (k) => m.get(k) ?? "", set: (k, v) => void (v ? m.set(k, v) : m.delete(k)) };
};
let store: Store;
beforeEach(() => (store = mem()));

describe("daily budget", () => {
  it("counts per date and resets on a new date", () => {
    expect(remaining("2026-11-03", store)).toBe(DAILY_BUDGET);
    recordUse("2026-11-03", store);
    recordUse("2026-11-03", store);
    expect(usedToday("2026-11-03", store)).toBe(2);
    expect(remaining("2026-11-03", store)).toBe(DAILY_BUDGET - 2);
    expect(usedToday("2026-11-04", store)).toBe(0);
    recordUse("2026-11-04", store);
    expect(usedToday("2026-11-04", store)).toBe(1);
    expect(usedToday("2026-11-03", store)).toBe(0);
  });
  it("never goes below zero remaining", () => {
    for (let i = 0; i < DAILY_BUDGET + 3; i++) recordUse("d", store);
    expect(remaining("d", store)).toBe(0);
  });
});

describe("retry-in parsing", () => {
  it("parses hours, minutes and seconds", () => {
    expect(parseRetryInMs("quota. Please retry in 5h48m34.5s.")).toBe(((5 * 60 + 48) * 60 + 34.5) * 1000);
    expect(parseRetryInMs("Please retry in 28s")).toBe(28000);
    expect(parseRetryInMs("Please retry in 3m10s")).toBe(190000);
  });
  it("returns null without a hint", () => {
    expect(parseRetryInMs("You exceeded your current quota")).toBeNull();
  });
  it("falls back to the next Pacific midnight", () => {
    const now = Date.UTC(2026, 10, 3, 12, 0, 0); // 04:00 PST
    const until = nextPacificMidnight(now);
    expect(until - now).toBe(20 * 3600 * 1000);
    expect(pauseUntilFor("no hint", now)).toBe(until);
    expect(pauseUntilFor("retry in 10s", now)).toBe(now + 11000);
  });
});

describe("pause logic", () => {
  it("is paused only until the stored time", () => {
    expect(isPaused(1000, store)).toBe(false);
    setPausedUntil(5000, store);
    expect(getPausedUntil(store)).toBe(5000);
    expect(isPaused(4999, store)).toBe(true);
    expect(isPaused(5000, store)).toBe(false);
  });
  it("words the friendly messages", () => {
    expect(formatDuration(((5 * 60) + 48) * 60000 - 20000)).toBe("5h 48m");
    expect(quotaMessage(((5 * 60) + 48) * 60000, 0)).toBe("Google's free daily limit is used up. It resets in about 5h 48m.");
    const until = Date.UTC(2026, 10, 3, 18, 59);
    expect(clockText(until, "UTC")).toBe("6:59 pm");
    setPausedUntil(until, store);
    expect(restingNotice("Miranda", "d", until - 1000, "UTC", store)).toBe("Miranda's AI writing is resting until about 6:59 pm — this is the built-in summary.");
    expect(restingNotice("Miranda", "d", until + 1000, "UTC", store)).toBeNull();
  });
});

import { recordVoiceUse, voiceRemaining, VOICE_BUDGET, recordUse as recUse, remaining as textRemaining } from "./ai-budget";
describe("separate voice budget", () => {
  it("voice clips and text requests are counted independently", () => {
    const m = new Map<string, string>();
    const store = { get: (k: string) => m.get(k) ?? null, set: (k: string, v: string) => void m.set(k, v), del: (k: string) => void m.delete(k) };
    for (let i = 0; i < 12; i++) recUse("2026-11-03", store as never);
    expect(textRemaining("2026-11-03", store as never)).toBe(0);
    expect(voiceRemaining("2026-11-03", store as never)).toBe(VOICE_BUDGET);
    recordVoiceUse("2026-11-03", store as never);
    expect(voiceRemaining("2026-11-03", store as never)).toBe(VOICE_BUDGET - 1);
    expect(voiceRemaining("2026-11-04", store as never)).toBe(VOICE_BUDGET);
  });
});

import { cleanKey } from "./cache";
describe("cleanKey", () => {
  it("strips spaces, newlines and invisible characters from a pasted key", () => {
    expect(cleanKey(" AIzaSyAbc​123\n ")).toBe("AIzaSyAbc123");
  });
});

import { addExtra, autoTries, MAX_AUTO_TRIES, noteAutoTry, remaining as rem2, limitToday } from "./ai-budget";
describe("extra allowance and automatic tries", () => {
  const mem = () => {
    const m = new Map<string, string>();
    return { get: (k: string) => m.get(k) ?? "", set: (k: string, v: string) => void (v ? m.set(k, v) : m.delete(k)) };
  };
  it("'Allow 3 more today' raises today's limit only", () => {
    const s = mem();
    expect(limitToday("2026-11-10", s)).toBe(12);
    addExtra("2026-11-10", 3, s);
    expect(limitToday("2026-11-10", s)).toBe(15);
    expect(rem2("2026-11-10", s)).toBe(15);
    expect(limitToday("2026-11-11", s)).toBe(12);
  });
  it("counts automatic tries per summary per day", () => {
    const s = mem();
    for (let i = 0; i < MAX_AUTO_TRIES; i++) noteAutoTry("2026-11-10", "morning:2026-11-10", s);
    expect(autoTries("2026-11-10", "morning:2026-11-10", s)).toBe(MAX_AUTO_TRIES);
    expect(autoTries("2026-11-10", "evening:2026-11-10", s)).toBe(0);
    expect(autoTries("2026-11-11", "morning:2026-11-10", s)).toBe(0);
  });
});
