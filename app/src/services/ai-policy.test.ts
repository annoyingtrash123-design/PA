import { describe, expect, it } from "vitest";
import { shouldAutoGenerate, showUpdateButton, showWriteButton, storeKey } from "./ai-policy";

const cached = { text: "hello", fingerprint: "a" };

describe("shouldAutoGenerate", () => {
  it("writes the first morning/evening text automatically", () => {
    expect(shouldAutoGenerate("morning", null)).toBe(true);
    expect(shouldAutoGenerate("evening", undefined)).toBe(true);
  });
  it("never regenerates when any text exists", () => {
    expect(shouldAutoGenerate("morning", cached)).toBe(false);
    expect(shouldAutoGenerate("evening", { text: "x" })).toBe(false);
    expect(shouldAutoGenerate("weekly", cached, { currentWeek: true })).toBe(false);
  });
  it("weekly is automatic for the current week only", () => {
    expect(shouldAutoGenerate("weekly", null, { currentWeek: true })).toBe(true);
    expect(shouldAutoGenerate("weekly", null, { currentWeek: false })).toBe(false);
    expect(shouldAutoGenerate("weekly", null)).toBe(false);
  });
});

describe("showUpdateButton", () => {
  it("appears only when the fingerprint changed", () => {
    expect(showUpdateButton(cached, "a")).toBe(false);
    expect(showUpdateButton(cached, "b")).toBe(true);
  });
  it("not without a text or a known fingerprint", () => {
    expect(showUpdateButton(null, "b")).toBe(false);
    expect(showUpdateButton({ text: "x" }, "b")).toBe(false);
    expect(showUpdateButton(cached, undefined)).toBe(false);
  });
});

describe("weekly buttons", () => {
  it("past/future weeks without text offer Write overview", () => {
    expect(showWriteButton("weekly", null, { currentWeek: false })).toBe(true);
    expect(showWriteButton("weekly", null, { currentWeek: true })).toBe(false);
    expect(showWriteButton("weekly", cached, { currentWeek: false })).toBe(false);
    expect(showWriteButton("morning", null)).toBe(false);
  });
  it("current week with a changed fingerprint offers update", () => {
    expect(showUpdateButton({ text: "w", fingerprint: "1" }, "2")).toBe(true);
  });
});

describe("storeKey", () => {
  it("is per day for daily kinds and per week for weekly", () => {
    expect(storeKey("morning", "2026-11-03", "2026-11-02")).toBe("morning:2026-11-03");
    expect(storeKey("weekly", "2026-11-04", "2026-11-02")).toBe("week:2026-11-02");
  });
});
