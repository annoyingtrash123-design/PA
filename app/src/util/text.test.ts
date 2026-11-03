import { describe, expect, it } from "vitest";
import { fmtWeight, paragraphs, parseNum, speechChunks } from "./text";

describe("paragraphs", () => {
  it("keeps existing paragraph breaks", () => {
    expect(paragraphs("One.\n\nTwo.\r\n\r\nThree.")).toEqual(["One.", "Two.", "Three."]);
  });
  it("leaves short single paragraphs alone", () => {
    expect(paragraphs("Just a short note.")).toEqual(["Just a short note."]);
  });
  it("splits a long wall of text into 3-5 paragraphs on sentence boundaries", () => {
    const text = Array.from({ length: 24 }, (_, i) => `This is sentence number ${i + 1} of the long summary text.`).join(" ");
    const ps = paragraphs(text);
    expect(ps.length).toBeGreaterThanOrEqual(3);
    expect(ps.length).toBeLessThanOrEqual(5);
    expect(ps.join(" ")).toBe(text);
    ps.forEach((p) => expect(p.endsWith(".")).toBe(true));
  });
});

describe("speechChunks", () => {
  it("returns nothing for empty text", () => {
    expect(speechChunks("   ")).toEqual([]);
  });
  it("keeps short text in one chunk", () => {
    expect(speechChunks("Hello there. How are you?", 180)).toEqual(["Hello there. How are you?"]);
  });
  it("groups sentences up to the limit and never exceeds it", () => {
    const text = Array.from({ length: 30 }, (_, i) => `Sentence ${i} is here.`).join(" ");
    const chunks = speechChunks(text, 100);
    expect(chunks.length).toBeGreaterThan(3);
    chunks.forEach((c) => expect(c.length).toBeLessThanOrEqual(100));
    expect(chunks.join(" ")).toBe(text);
  });
  it("breaks a very long sentence on commas and spaces", () => {
    const text = "word, ".repeat(100).trim() + ".";
    const chunks = speechChunks(text, 60);
    chunks.forEach((c) => expect(c.length).toBeLessThanOrEqual(60));
    expect(chunks.join(" ").replace(/\s+/g, " ")).toBe(text);
    const nocommas = "abcdefghij ".repeat(40).trim();
    speechChunks(nocommas, 50).forEach((c) => expect(c.length).toBeLessThanOrEqual(50));
  });
});

describe("number helpers", () => {
  it("parses decimals with a comma", () => {
    expect(parseNum("62,5")).toBe(62.5);
    expect(parseNum(" 8 ")).toBe(8);
    expect(parseNum("abc", 3)).toBe(3);
    expect(parseNum("", 7)).toBe(7);
  });
  it("formats weights without trailing zeros", () => {
    expect(fmtWeight(60)).toBe("60");
    expect(fmtWeight(62.5)).toBe("62.5");
    expect(fmtWeight(null)).toBe("");
  });
});
