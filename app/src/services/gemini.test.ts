import { describe, expect, it } from "vitest";
import { cleanReply, parseDigestJson } from "./gemini";
import { pickVoice, voiceScore } from "./speech";

describe("cleanReply", () => {
  it("strips markdown decoration", () => {
    expect(cleanReply("# Hello\n**Bold** and *italic* and `code`.\n- one\n- two\n1. three")).toBe("Hello\nBold and italic and code.\none\ntwo\nthree");
  });
  it("removes links but keeps their text", () => {
    expect(cleanReply("See [the plan](https://x.y/z) now")).toBe("See the plan now");
  });
  it("removes emojis and collapses spacing", () => {
    expect(cleanReply("Good morning ☀️  friend 😊!\n\n\n\nBye")).toBe("Good morning friend !\n\nBye");
  });
  it("keeps apostrophes and plain punctuation", () => {
    expect(cleanReply("Here's the plan: it's simple, really.")).toBe("Here's the plan: it's simple, really.");
  });
});

describe("parseDigestJson", () => {
  it("parses plain JSON", () => {
    expect(parseDigestJson('{"weather_comment":"Nice and mild.","overview":"A calm day."}')).toEqual({ weather_comment: "Nice and mild.", overview: "A calm day." });
  });
  it("parses fenced JSON and chatter around it", () => {
    expect(parseDigestJson('```json\n{"weather_comment":null,"overview":"**Busy** morning."}\n```')).toEqual({ weather_comment: null, overview: "Busy morning." });
    expect(parseDigestJson('Sure! {"overview":"Hi there."} Hope that helps')).toEqual({ weather_comment: null, overview: "Hi there." });
  });
  it("returns null for unusable replies", () => {
    expect(parseDigestJson("not json")).toBeNull();
    expect(parseDigestJson('{"weather_comment":"x"}')).toBeNull();
    expect(parseDigestJson('{"overview":""}')).toBeNull();
    expect(parseDigestJson("[1,2]")).toBeNull();
  });
});

describe("voice choice", () => {
  const v = (name: string, lang: string) => ({ name, lang }) as SpeechSynthesisVoice;
  const voices = [v("Daniel", "en-GB"), v("Samantha", "en-US"), v("Google US English", "en-US"), v("Fred", "en-US"), v("Karen", "en-AU")];
  it("honours the saved name", () => {
    expect(pickVoice(voices, "Daniel")?.name).toBe("Daniel");
  });
  it("defaults to a natural en-US voice and avoids novelty voices", () => {
    const best = pickVoice(voices)!;
    expect(["Samantha", "Google US English"]).toContain(best.name);
    expect(voiceScore(v("Fred", "en-US"))).toBeLessThan(voiceScore(v("Daniel", "en-GB")));
  });
  it("falls back when the saved voice is gone, and copes with none", () => {
    expect(pickVoice(voices, "Gone")?.name).toBeTruthy();
    expect(pickVoice([])).toBeNull();
  });
});

import { looksTruncated, parseMorningJson as pmj, salvageFields } from "./gemini";

describe("tolerant briefing JSON (truncated replies)", () => {
  const full = '{"greeting":"Morning, Sam!","weather_comment":"Cool.","affirmation":"Dentist first.","briefing":"Para one.\\n\\nPara two.","news":"From the ABC, the headline is: \\"Prices up 5%\\". So there';
  it("keeps the complete fields of a reply cut off in the news and uses the briefing", () => {
    expect(looksTruncated(full)).toBe(true);
    const r = pmj(full);
    expect(r).toMatchObject({ greeting: "Morning, Sam!", affirmation: "Dentist first.", briefing: "Para one.\n\nPara two.", news: null, salvaged: true });
  });
  it("handles fences, an unclosed fence, and missing parts", () => {
    expect(pmj('```json\n{"briefing":"Hi there.","greeting":"Hello"}\n```')).toMatchObject({ briefing: "Hi there.", greeting: "Hello" });
    expect(pmj('```json\n{"greeting":"Hello","briefing":"Done.","affirmation":"Go"')).toMatchObject({ briefing: "Done.", affirmation: "Go", salvaged: true });
  });
  it("needs a complete briefing field", () => {
    expect(pmj('{"greeting":"Hello","briefing":"Cut off mid sen')).toBeNull();
    expect(salvageFields('{"a":"x","b":null,"c":3,"d":"unfinished')).toEqual({ a: "x", b: null, c: 3 });
  });
});
