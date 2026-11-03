import { describe, expect, it } from "vitest";
import { audioBlobFrom, buildPrompt, clampText, parseSampleRate, MAX_CLIPS, pcmToWav, planVoice, pruneIndex, restingReason, ttsCacheKey, type IndexEntry } from "./gemini-tts";
import { resolveVoiceType } from "./voice-prefs";

describe("pcmToWav", () => {
  it("writes a correct 44-byte header", () => {
    const pcm = new Uint8Array([1, 2, 3, 4, 5, 6]);
    const w = pcmToWav(pcm, 24000);
    const v = new DataView(w.buffer);
    const tag = (o: number) => String.fromCharCode(...w.slice(o, o + 4));
    expect(w.length).toBe(50);
    expect([tag(0), tag(8), tag(12), tag(36)]).toEqual(["RIFF", "WAVE", "fmt ", "data"]);
    expect(v.getUint32(4, true)).toBe(42);
    expect(v.getUint16(20, true)).toBe(1);
    expect(v.getUint16(22, true)).toBe(1);
    expect(v.getUint32(24, true)).toBe(24000);
    expect(v.getUint32(28, true)).toBe(48000);
    expect(v.getUint16(32, true)).toBe(2);
    expect(v.getUint16(34, true)).toBe(16);
    expect(v.getUint32(40, true)).toBe(6);
    expect([...w.slice(44)]).toEqual([1, 2, 3, 4, 5, 6]);
  });
  it("honours another sample rate", () => {
    const v = new DataView(pcmToWav(new Uint8Array(2), 16000).buffer);
    expect(v.getUint32(24, true)).toBe(16000);
    expect(v.getUint32(28, true)).toBe(32000);
  });
  it("audioBlobFrom wraps PCM as wav and passes containers through", () => {
    expect(audioBlobFrom(btoa("abcd"), "audio/L16;codec=pcm;rate=24000").type).toBe("audio/wav");
    expect(audioBlobFrom(btoa("abcd"), "audio/L16;codec=pcm;rate=24000").size).toBe(48);
    expect(audioBlobFrom(btoa("abcd"), "audio/mpeg").type).toBe("audio/mpeg");
  });
});

describe("parseSampleRate", () => {
  it("reads rate= from the mimeType", () => {
    expect(parseSampleRate("audio/L16;codec=pcm;rate=24000")).toBe(24000);
    expect(parseSampleRate("audio/L16;rate=16000")).toBe(16000);
  });
  it("defaults to 24000", () => {
    expect(parseSampleRate(undefined)).toBe(24000);
    expect(parseSampleRate("audio/L16")).toBe(24000);
    expect(parseSampleRate("audio/L16;rate=12")).toBe(24000);
  });
});

describe("ttsCacheKey", () => {
  it("is stable and sensitive to every part", () => {
    const k = ttsCacheKey("m", "Kore", "warm", "Hello there");
    expect(ttsCacheKey("m", "Kore", "warm", "Hello there")).toBe(k);
    expect(k).toMatch(/^tts-[0-9a-f]{16}-\d+$/);
    for (const other of [ttsCacheKey("m2", "Kore", "warm", "Hello there"), ttsCacheKey("m", "Puck", "warm", "Hello there"), ttsCacheKey("m", "Kore", "calm", "Hello there"), ttsCacheKey("m", "Kore", "warm", "Hello there!")]) expect(other).not.toBe(k);
  });
});

describe("pruneIndex", () => {
  const day = 24 * 3600 * 1000;
  it("keeps the newest clips (MAX_CLIPS), drops clips older than 3 days, never previews", () => {
    const now = 100 * day;
    const idx: IndexEntry[] = [{ key: "p", at: 0, kind: "preview" }, { key: "old", at: now - 4 * day, kind: "clip" }];
    for (let i = 0; i < MAX_CLIPS + 2; i++) idx.push({ key: `c${i}`, at: now - i * 1000, kind: "clip" });
    const { keep, drop } = pruneIndex(idx, now);
    expect(keep.map((e) => e.key)).toEqual(["p", ...Array.from({ length: MAX_CLIPS }, (_, i) => `c${i}`)]);
    expect(drop.map((e) => e.key).sort()).toEqual([`c${MAX_CLIPS}`, `c${MAX_CLIPS + 1}`, "old"]);
  });
});

describe("prompt and text", () => {
  it("prefixes the style instruction", () => {
    expect(buildPrompt(" Hi ", "warm")).toBe("Say warmly and chattily, like a caring friend talking to you over coffee, at a relaxed, unhurried pace: Hi");
    expect(buildPrompt("Hi", "nope")).toContain("warmly");
  });
  it("clamps at a paragraph boundary", () => {
    expect(clampText("a".repeat(30) + "\n\n" + "b".repeat(30), 50)).toBe("a".repeat(30));
    expect(clampText("short")).toBe("short");
  });
});

describe("planVoice (fallback decision)", () => {
  const base = { wantNatural: true, hasKey: true, blockedUntil: 0, remaining: 5, now: 1_000_000 };
  it("uses Gemini when chosen and available", () => expect(planVoice(base)).toEqual({ mode: "gemini" }));
  it("is silent about the phone voice when the phone voice was chosen", () => expect(planVoice({ ...base, wantNatural: false })).toEqual({ mode: "phone", reason: null }));
  it("falls back without a key", () => expect(planVoice({ ...base, hasKey: false })).toMatchObject({ mode: "phone", reason: expect.stringContaining("key") }));
  it("falls back while paused, naming the time", () => {
    const p = planVoice({ ...base, blockedUntil: base.now + 3600_000, timeZone: "UTC" });
    expect(p).toEqual({ mode: "phone", reason: restingReason("Miranda", base.now + 3600_000, "UTC") });
    expect((p as { reason: string }).reason).toMatch(/resting until \d{1,2}:\d{2} [ap]m/);
  });
  it("ignores a pause that has passed", () => expect(planVoice({ ...base, blockedUntil: base.now - 1 })).toEqual({ mode: "gemini" }));
  it("falls back when the budget is used up", () => expect(planVoice({ ...base, remaining: 0 })).toMatchObject({ mode: "phone", reason: expect.stringContaining("allowance") }));
});

describe("resolveVoiceType", () => {
  it("defaults to Natural with a key, Phone without, and respects a saved choice", () => {
    expect(resolveVoiceType({}, true)).toBe("gemini");
    expect(resolveVoiceType({}, false)).toBe("phone");
    expect(resolveVoiceType({ type: "phone" }, true)).toBe("phone");
    expect(resolveVoiceType({ type: "gemini" }, false)).toBe("gemini");
  });
});

import { rankTtsModels } from "./gemini-tts";
describe("rankTtsModels", () => {
  it("keeps only TTS models, flash before pro, newest first", () => {
    expect(rankTtsModels(["gemini-3.8-flash", "gemini-2.5-pro-preview-tts", "gemini-2.5-flash-preview-tts", "gemini-3.8-flash-tts", "gemini-3.8-flash-tts"])).toEqual([
      "gemini-3.8-flash-tts",
      "gemini-2.5-flash-preview-tts",
      "gemini-2.5-pro-preview-tts",
    ]);
  });
});

import { chunkScript, chunkSections, splitSentences } from "./gemini-tts";

const sentence = (n: number) => `This is sentence number ${n} and it has a handful of ordinary spoken words in it.`; // 16 words
const para = (start: number, count: number) => Array.from({ length: count }, (_, k) => sentence(start + k)).join(" ");
const words = (s: string) => s.split(/\s+/).filter(Boolean).length;

describe("chunkScript", () => {
  it("splits a ~750 word script into ~1.5 minute chunks at paragraph boundaries", () => {
    const paras = Array.from({ length: 8 }, (_, i) => para(i * 6, 6)); // 8 paragraphs of 96 words
    const chunks = chunkScript(paras.join("\n\n"));
    expect(chunks.length).toBeGreaterThanOrEqual(3);
    expect(chunks.length).toBeLessThanOrEqual(4);
    for (const c of chunks) {
      expect(words(c)).toBeGreaterThan(80);
      expect(words(c)).toBeLessThanOrEqual(300);
    }
    // nothing lost, nothing cut: every chunk is whole paragraphs
    expect(chunks.join("\n\n")).toBe(paras.join("\n\n"));
  });
  it("splits one huge paragraph at sentence ends only", () => {
    const big = para(0, 40); // 640 words, one paragraph
    const chunks = chunkScript(big);
    expect(chunks.length).toBeGreaterThanOrEqual(3);
    for (const c of chunks) {
      expect(c.endsWith(".")).toBe(true);
      expect(c.startsWith("This is sentence number")).toBe(true);
      expect(words(c)).toBeLessThanOrEqual(260);
    }
    expect(chunks.join(" ")).toBe(big);
  });
  it("keeps a short script as one chunk and folds a tiny tail into the previous chunk", () => {
    expect(chunkScript("Hello there.\n\nHow are you today?")).toEqual(["Hello there.\n\nHow are you today?"]);
    const withTail = [para(0, 15), para(20, 2)].join("\n\n"); // 240 + 32 words
    expect(chunkScript(withTail)).toHaveLength(1);
  });
  it("does not treat an ellipsis or an abbreviation as a sentence end", () => {
    expect(splitSentences("Hmm… let's see, so... anyway. The U.S. said yes. Done!")).toEqual(["Hmm… let's see, so... anyway.", "The U.S. said yes.", "Done!"]);
  });
});

describe("chunkSections", () => {
  it("makes each section one clip and only splits one that runs past about two minutes", () => {
    const s1 = para(0, 8);
    const s2 = para(10, 40);
    const out = chunkSections([s1, "", s2]);
    expect(out[0]).toBe(s1);
    expect(out.length).toBeGreaterThanOrEqual(3);
    expect(out.slice(1).every((c) => words(c) <= 300 && c.endsWith("."))).toBe(true);
  });
});
