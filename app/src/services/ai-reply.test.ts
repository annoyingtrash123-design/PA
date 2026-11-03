import { describe, expect, it } from "vitest";
import { guardText, judgeReply } from "./ai-reply";
import { sourceLabel, whyBudget, whyFromError, whyPaused } from "./ai-reason";

// a stand-in for the core's guard: flags "N of M" and percentages
const stats = (t: string) => /\d+ of \d+|%|percent/i.test(t);
const reply = (o: Record<string, unknown>) => JSON.stringify(o);

describe("stats guard", () => {
  it("ignores the news segment (verbatim headlines hold numbers)", () => {
    const raw = reply({ greeting: "Morning!", weather_comment: "Cool.", affirmation: "Dentist first.", briefing: "Looking at today.", news: 'From the Guardian, the headline is: "1 in 5 say 40% of jobs are at risk".' });
    const j = judgeReply(raw, true, stats);
    expect(j.why).toBeNull();
    expect(j.r?.news).toContain("40%");
    expect(guardText(j.r!)).not.toContain("40%");
  });
  it("still stops raw counts in the plan text", () => {
    const j = judgeReply(reply({ briefing: "You did 3 of 5 things." }), true, stats);
    expect(j.r).toBeNull();
    expect(j.why?.code).toBe("stats");
  });
  it("tells a cut-off reply from an unreadable one", () => {
    expect(judgeReply('{"greeting":"Hi","briefing":"Half a sen', true, stats).why?.code).toBe("cut-off");
    expect(judgeReply("sorry, no", true, stats).why?.code).toBe("parse");
  });
  it("salvages the briefing when only the news was cut off", () => {
    const j = judgeReply('{"greeting":"Hi","briefing":"All good.","news":"From the ABC, the head', true, stats);
    expect(j.why).toBeNull();
    expect(j.salvaged).toBe(true);
    expect(j.r?.briefing).toBe("All good.");
  });
});

describe("fallback reasons and the source label", () => {
  it("names the reason", () => {
    expect(whyFromError({ message: "Gemini said 503", status: 503 }).text).toBe("Google replied with an error (503)");
    expect(whyFromError({ message: "Couldn't reach Gemini (x)" }).code).toBe("network");
    expect(whyFromError({ message: "Gemini took too long to answer" }).code).toBe("timeout");
    expect(whyFromError({ message: "No Gemini key saved" }).code).toBe("no-key");
    expect(whyFromError({ message: "used up", blocked: true }).code).toBe("budget");
    expect(whyFromError({ message: "q", blocked: true, quota: true, pausedUntil: Date.UTC(2026, 10, 3, 8, 0) }, "UTC").text).toMatch(/^Google's limit is resting until 8:00 am/);
  });
  it("labels both sources", () => {
    expect(sourceLabel("gemini", null)).toBe("Written by Miranda's AI");
    expect(sourceLabel("template", whyBudget())).toBe("Built-in summary — today's AI allowance is used up");
    expect(sourceLabel("template", whyPaused(Date.UTC(2026, 10, 3, 19, 0), "UTC"))).toBe("Built-in summary — Google's limit is resting until 7:00 pm");
    expect(sourceLabel("template", null)).toBe("Built-in summary");
    expect(sourceLabel("gemini", null, "Miranda", null, true)).toBe("Written by Miranda's AI and checked by her AI editor");
    expect(sourceLabel("gemini", null, "Miranda", "the AI reworded the headlines")).toContain("news part is the built-in version");
  });
});
