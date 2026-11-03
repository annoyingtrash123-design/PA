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
