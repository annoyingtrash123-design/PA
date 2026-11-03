import { describe, expect, it } from "vitest";
import { clampSpeed, DEFAULT_SPEED } from "./voice-prefs";

describe("clampSpeed", () => {
  it("defaults, clamps and snaps", () => {
    expect(clampSpeed(undefined)).toBe(DEFAULT_SPEED);
    expect(clampSpeed("x")).toBe(DEFAULT_SPEED);
    expect(clampSpeed(0.2)).toBe(0.7);
    expect(clampSpeed(5)).toBe(1.15);
    expect(clampSpeed(0.88)).toBe(0.88);
    expect(clampSpeed(1)).toBe(1);
  });
});
