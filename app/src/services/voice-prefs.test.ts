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

import { cleanFavs, orderVoices } from "./voice-prefs";

describe("favourite voices", () => {
  const voices = [{ name: "Sulafat" }, { name: "Achernar" }, { name: "Aoede" }, { name: "Puck" }];
  it("lists favourites first (in the order starred), then the rest in the usual order", () => {
    const o = orderVoices(voices, ["Puck", "Achernar"]);
    expect(o.favourites.map((v) => v.name)).toEqual(["Puck", "Achernar"]);
    expect(o.others.map((v) => v.name)).toEqual(["Sulafat", "Aoede"]);
    expect(orderVoices(voices, []).favourites).toEqual([]);
  });
  it("ignores unknown names and repeats", () => {
    expect(cleanFavs(["Sulafat", "Nobody", "Sulafat", 5])).toEqual(["Sulafat"]);
    expect(cleanFavs("x")).toEqual([]);
  });
});

import { VOICES } from "./gemini-tts";
import { cleanFavs as clean2 } from "./voice-prefs";
it("only female voices are offered, and a saved male favourite is dropped", () => {
  const names = VOICES.map((v) => v.name);
  for (const m of ["Puck", "Charon", "Fenrir", "Orus", "Enceladus", "Iapetus", "Umbriel", "Algieba", "Algenib", "Rasalgethi", "Alnilam", "Schedar", "Achird", "Zubenelgenubi", "Sadachbia", "Sadaltager"]) expect(names).not.toContain(m);
  expect(names).toContain("Sulafat");
  expect(clean2(["Puck", "Kore"])).toEqual(["Kore"]);
});
