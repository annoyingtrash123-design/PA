#!/usr/bin/env node
// Usage: node scripts/make-profile-code.mjs profile.json   -> prints a MIRANDA1: code (wrapped at 76 chars)
//        node scripts/make-profile-code.mjs --decode CODE  -> prints the JSON back
// The encode/decode logic duplicates src/util/profile-code.ts (a test checks they agree).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

export const PROFILE_PREFIX = "MIRANDA1:";
const isObj = (x) => typeof x === "object" && x !== null && !Array.isArray(x);

export function validateProfile(x) {
  const bad = (m) => { throw new Error(m); };
  if (!isObj(x)) bad("The profile must be a JSON object");
  if (x.v !== 1) bad("Unsupported profile version");
  if (typeof x.updated !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(x.updated)) bad("\"updated\" must be YYYY-MM-DD");
  if (x.interests !== undefined && typeof x.interests !== "string") bad("\"interests\" must be text");
  if (x.notes !== undefined && (!Array.isArray(x.notes) || x.notes.some((n) => typeof n !== "string"))) bad("\"notes\" must be a list of text");
  if (x.work !== undefined && !isObj(x.work)) bad("\"work\" must be an object");
  if (x.weather !== undefined && !isObj(x.weather)) bad("\"weather\" must be an object");
  if (x.add_goals !== undefined && (!Array.isArray(x.add_goals) || x.add_goals.some((g) => !isObj(g) || typeof g.title !== "string" || typeof g.horizon !== "string"))) bad("\"add_goals\" items need a title and horizon");
  if (x.add_leisure !== undefined && (!Array.isArray(x.add_leisure) || x.add_leisure.some((l) => !isObj(l) || typeof l.title !== "string" || typeof l.kind !== "string"))) bad("\"add_leisure\" items need a kind and title");
  return x;
}

export function encodeProfile(p) {
  const bytes = new TextEncoder().encode(JSON.stringify(validateProfile(p)));
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return PROFILE_PREFIX + btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function decodeProfile(code) {
  const c = code.replace(/\s+/g, "");
  if (!c.startsWith(PROFILE_PREFIX)) throw new Error("Not a profile code (should start with MIRANDA1:)");
  const std = c.slice(PROFILE_PREFIX.length).replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(std + "=".repeat((4 - (std.length % 4)) % 4));
  return validateProfile(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Uint8Array.from(bin, (ch) => ch.charCodeAt(0)))));
}

export const wrap = (s, n = 76) => s.match(new RegExp(`.{1,${n}}`, "g")).join("\n");

function main(argv) {
  try {
    if (argv[0] === "--decode") {
      if (!argv[1]) throw new Error("Usage: make-profile-code.mjs --decode CODE");
      console.log(JSON.stringify(decodeProfile(argv.slice(1).join("")), null, 2));
    } else if (argv[0]) {
      console.log(wrap(encodeProfile(JSON.parse(readFileSync(argv[0], "utf8")))));
    } else {
      throw new Error("Usage: make-profile-code.mjs profile.json | --decode CODE");
    }
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main(process.argv.slice(2));
