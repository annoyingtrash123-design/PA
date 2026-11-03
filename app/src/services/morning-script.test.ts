import { describe, expect, it } from "vitest";
import type { MorningParts } from "../types";
import { parseMorningJson } from "./gemini";
import { mergeParts, morningScript, readEntry } from "./morning-script";

const tpl: MorningParts = { greeting: "Good morning! Happy Tuesday.", weather_comment: "Cool, 14 degrees.", affirmation: "Start with the dentist at 10.", briefing: "Yesterday you sorted the bins." };

describe("morningScript", () => {
  it("speaks greeting, weather, affirmation, briefing in that order, as blank-line separated paragraphs", () => {
    expect(morningScript(tpl)).toBe([tpl.greeting, tpl.weather_comment, tpl.affirmation, tpl.briefing].join("\n\n"));
  });
  it("skips empty parts (no weather)", () => {
    expect(morningScript({ ...tpl, weather_comment: null })).toBe([tpl.greeting, tpl.affirmation, tpl.briefing].join("\n\n"));
    expect(morningScript({ ...tpl, greeting: "" }).startsWith("Cool")).toBe(true);
  });
});

describe("parseMorningJson (four parts)", () => {
  it("parses all four", () => {
    const r = parseMorningJson('{"greeting":"Morning! Happy Tuesday.","weather_comment":"Cool.","affirmation":"Dentist first.","briefing":"Yesterday was **good**."}');
    expect(r).toEqual({ greeting: "Morning! Happy Tuesday.", weather_comment: "Cool.", affirmation: "Dentist first.", briefing: "Yesterday was good.", news: null });
  });
  it("is tolerant of fences and chatter, and missing fields come back null", () => {
    expect(parseMorningJson('```json\n{"briefing":"Hi.","weather_comment":null}\n```')).toEqual({ greeting: null, weather_comment: null, affirmation: null, briefing: "Hi.", news: null });
    expect(parseMorningJson('Sure! {"affirmation":"A","briefing":"B"} done')).toMatchObject({ affirmation: "A", briefing: "B" });
  });
  it("needs a briefing", () => {
    expect(parseMorningJson('{"greeting":"x","affirmation":"y"}')).toBeNull();
    expect(parseMorningJson("not json")).toBeNull();
  });
});

describe("mergeParts", () => {
  it("falls back to the template part for each missing AI field", () => {
    const ai = parseMorningJson('{"affirmation":"Take the dentist first.","briefing":"The AI body."}');
    expect(mergeParts(ai, tpl)).toEqual({ ...tpl, affirmation: "Take the dentist first.", briefing: "The AI body." });
  });
  it("uses the template when there is no AI text", () => {
    expect(mergeParts(null, tpl)).toEqual(tpl);
  });
});

describe("cache back-compat", () => {
  it("an old plain-text entry is a briefing-only text: template weather and affirmation, no extra greeting", () => {
    const e = readEntry("Good morning! A whole old text.");
    expect(e).toEqual({ briefing: "Good morning! A whole old text.", legacy: true });
    expect(mergeParts(e, tpl)).toEqual({ greeting: "", weather_comment: tpl.weather_comment, affirmation: tpl.affirmation, briefing: "Good morning! A whole old text." });
  });
  it("an old {briefing, weather_comment} entry keeps its AI weather comment", () => {
    const e = readEntry({ briefing: "Old text.", weather_comment: "Old AI weather.", fingerprint: "f1" });
    expect(e).toMatchObject({ legacy: true, fingerprint: "f1" });
    const p = mergeParts(e, tpl);
    expect(p).toMatchObject({ greeting: "", weather_comment: "Old AI weather.", affirmation: tpl.affirmation, briefing: "Old text." });
    expect(morningScript(p).startsWith("Old AI weather.")).toBe(true);
  });
  it("a new entry is used as written, with template fills for nulls", () => {
    const e = readEntry({ briefing: "B", greeting: "Hi!", weather_comment: null, affirmation: "A", fingerprint: "f2" });
    expect(e?.legacy).toBe(false);
    expect(mergeParts(e, tpl)).toEqual({ greeting: "Hi!", weather_comment: tpl.weather_comment, affirmation: "A", briefing: "B" });
  });
  it("junk is ignored", () => {
    expect(readEntry(null)).toBeNull();
    expect(readEntry({ briefing: "  " })).toBeNull();
  });
});

import { eveningSections, morningSections, normalisePos } from "./morning-script";

describe("spoken sections", () => {
  const p: MorningParts = { greeting: "Morning!", weather_comment: "Cool.", affirmation: "Dentist first.", briefing: "P1 day.\n\nP2 order.\n\nP3 tomorrow.\n\nP4 close." };
  it("morning: hello/weather/affirmation, the day, then tomorrow + closing", () => {
    expect(morningSections(p, null, 2)).toEqual(["Morning!\n\nCool.\n\nDentist first.", "P1 day.\n\nP2 order.", "P3 tomorrow.\n\nP4 close."]);
  });
  it("news first joins section 1; news last sits before the closing in section 3; never in the middle", () => {
    const first = morningSections(p, "NEWS", 0);
    expect(first[0].endsWith("NEWS")).toBe(true);
    expect(first[1]).toBe("P1 day.\n\nP2 order.");
    const last = morningSections(p, "NEWS", 2);
    expect(last[1]).toBe("P1 day.\n\nP2 order.");
    expect(last[2]).toBe("P3 tomorrow.\n\nNEWS\n\nP4 close.");
    expect(normalisePos(1)).toBe(2);
  });
  it("evening: look back, tomorrow, then news + cosy close", () => {
    expect(eveningSections("A back.\n\nB tomorrow.\n\nC close.", "NEWS")).toEqual(["A back.", "B tomorrow.", "NEWS\n\nC close."]);
    expect(eveningSections("A back.\n\nB tomorrow.\n\nC close.", null)).toHaveLength(3);
  });
  it("joined sections read as the same text as the screen shows", () => {
    expect(morningSections(p, "NEWS", 2).join("\n\n")).toBe(["Morning!", "Cool.", "Dentist first.", "P1 day.", "P2 order.", "P3 tomorrow.", "NEWS", "P4 close."].join("\n\n"));
  });
});
