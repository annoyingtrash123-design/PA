import { describe, expect, it } from "vitest";
import { composeWithNews, morningScriptWithNews, splitForNews } from "./morning-script";
import { parseMorningJson, parseNewsJson } from "./gemini";
import { hasNewerNews, headlinesQuoted, newsIsFaithful, parseNewsFile, sourcesLine, updatedLabel } from "./news";

describe("parseNewsFile", () => {
  it("keeps valid items, drops untitled ones, tolerates junk", () => {
    const f = parseNewsFile({ generated_at: "2026-11-02T18:45:00Z", items: [{ id: "a", source: "ABC News", title: " Hello ", summary: "Lead.", link: "u", published: "2026-11-02T10:00:00Z", topics: ["australia", 3] }, { title: "" }, null, 5] });
    expect(f.items).toHaveLength(1);
    expect(f.items[0]).toMatchObject({ title: "Hello", topics: ["australia"], summary: "Lead." });
    expect(parseNewsFile(null)).toEqual({ generated_at: "", items: [] });
    expect(parseNewsFile({ items: "x" }).items).toEqual([]);
  });
});

describe("headline quoting", () => {
  const titles = ["Sydney ferry services disrupted by storm damage", "Ceasefire talks resume in Geneva", "Japan’s exports rise"];
  it("counts exact headlines, ignoring curly quotes and case", () => {
    const text = 'Okay, so the first one\'s from the ABC, and the headline is: "Sydney ferry services disrupted by storm damage". So basically... Next up: "Ceasefire talks resume in Geneva".';
    expect(headlinesQuoted(text, titles)).toBe(2);
    expect(headlinesQuoted("Japan's exports rise, apparently.", titles)).toBe(1);
  });
  it("is faithful when at least half are quoted", () => {
    expect(newsIsFaithful('"Sydney ferry services disrupted by storm damage" and "Ceasefire talks resume in Geneva"', titles)).toBe(true);
    expect(newsIsFaithful("Some ferries had trouble and talks resumed.", titles)).toBe(false);
    expect(newsIsFaithful("anything", [])).toBe(false);
  });
});

describe("news helpers", () => {
  it("detects newer news", () => {
    expect(hasNewerNews(undefined, "2026-11-02T05:45:00Z")).toBe(true);
    expect(hasNewerNews("2026-11-01T18:45:00Z", "2026-11-02T05:45:00Z")).toBe(true);
    expect(hasNewerNews("2026-11-02T05:45:00Z", "2026-11-02T05:45:00Z")).toBe(false);
    expect(hasNewerNews("2026-11-01T18:45:00Z", "")).toBe(false);
  });
  it("formats the updated time and sources", () => {
    expect(updatedLabel("2026-11-01T18:45:00Z", "Australia/Sydney")).toBe("5:45 am");
    expect(updatedLabel("nope")).toBe("");
    expect(sourcesLine(["ABC", "BBC"])).toBe("Sources: ABC, BBC");
    expect(sourcesLine([])).toBe("");
  });
});

describe("news inside the spoken flow", () => {
  const body = "One.\n\nTwo.\n\nThree.\n\nClosing.";
  it("splits and composes at each position (same as the core)", () => {
    expect(splitForNews(body, 0)).toEqual(["", body]);
    expect(splitForNews(body, 1)).toEqual(["One.", "Two.\n\nThree.\n\nClosing."]);
    expect(splitForNews(body, 2)).toEqual(["One.\n\nTwo.\n\nThree.", "Closing."]);
    expect(composeWithNews(body, "NEWS", 1)).toBe("One.\n\nNEWS\n\nTwo.\n\nThree.\n\nClosing.");
    expect(composeWithNews("Only.", "NEWS", 2)).toBe("Only.\n\nNEWS");
    expect(composeWithNews(body, null, 1)).toBe(body);
  });
  it("puts the news after the affirmation when first", () => {
    const parts = { greeting: "Hi.", weather_comment: null, affirmation: "Aff.", briefing: body };
    expect(morningScriptWithNews(parts, "NEWS", 0)).toBe("Hi.\n\nAff.\n\nNEWS\n\nOne.\n\nTwo.\n\nThree.\n\nClosing.");
  });
});

describe("5-part reply parsing", () => {
  it("reads the fifth field", () => {
    const r = parseMorningJson('{"greeting":"G","weather_comment":null,"affirmation":"A","briefing":"B","news":"Okay, so **one**."}');
    expect(r).toEqual({ greeting: "G", weather_comment: null, affirmation: "A", briefing: "B", news: "Okay, so one." });
  });
  it("falls back when news is missing, empty or not a string", () => {
    expect(parseMorningJson('{"briefing":"B"}')?.news).toBeNull();
    expect(parseMorningJson('{"briefing":"B","news":"  "}')?.news).toBeNull();
    expect(parseMorningJson('{"briefing":"B","news":5}')?.news).toBeNull();
  });
  it("parses the news-only reply", () => {
    expect(parseNewsJson('```json\n{"news":"Hello."}\n```')).toBe("Hello.");
    expect(parseNewsJson("nope")).toBeNull();
    expect(parseNewsJson('{"other":1}')).toBeNull();
  });
});
