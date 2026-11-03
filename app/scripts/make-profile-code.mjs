#!/usr/bin/env node
// Usage: node scripts/make-profile-code.mjs profile.json        -> prints a MIRANDA2: code (deflate-raw, wrapped at 76 chars)
//        node scripts/make-profile-code.mjs --v1 profile.json    -> prints the older MIRANDA1: code
//        node scripts/make-profile-code.mjs --decode CODE        -> prints the JSON back (either format)
// The encode/decode logic duplicates src/util/profile-code.ts (a test checks they agree).
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { deflateRawSync, inflateRawSync } from "node:zlib";

export const PROFILE_PREFIX_V1 = "MIRANDA1:";
export const PROFILE_PREFIX = "MIRANDA2:";
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
  if (x.add_tasks !== undefined && (!Array.isArray(x.add_tasks) || x.add_tasks.some((t) => !isObj(t) || typeof t.title !== "string" || (t.date !== undefined && typeof t.date !== "string") || (t.checklist !== undefined && (!Array.isArray(t.checklist) || t.checklist.some((c) => typeof c !== "string")))))) bad("\"add_tasks\" items need a title and a YYYY-MM-DD date");
  return x;
}

const b64url = (buf) => Buffer.from(buf).toString("base64url");

export function encodeProfile(p, { v1 = false } = {}) {
  const raw = Buffer.from(JSON.stringify(validateProfile(p)), "utf8");
  return v1 ? PROFILE_PREFIX_V1 + b64url(raw) : PROFILE_PREFIX + b64url(deflateRawSync(raw, { level: 9 }));
}

export function decodeProfile(code) {
  const c = code.replace(/\s+/g, "");
  const m = /MIRANDA([12]):([A-Za-z0-9_-]+)/.exec(c);
  if (!m) throw new Error("Not a profile code (should start with MIRANDA1: or MIRANDA2:)");
  const bytes = Buffer.from(m[2], "base64url");
  const text = new TextDecoder("utf-8", { fatal: true }).decode(m[1] === "1" ? bytes : inflateRawSync(bytes));
  return validateProfile(JSON.parse(text));
}

export const wrap = (s, n = 76) => s.match(new RegExp(`.{1,${n}}`, "g")).join("\n");

function main(argv) {
  try {
    const v1 = argv[0] === "--v1";
    if (v1) argv = argv.slice(1);
    if (argv[0] === "--decode") {
      if (!argv[1]) throw new Error("Usage: make-profile-code.mjs --decode CODE");
      console.log(JSON.stringify(decodeProfile(argv.slice(1).join("")), null, 2));
    } else if (argv[0]) {
      console.log(wrap(encodeProfile(JSON.parse(readFileSync(argv[0], "utf8")), { v1 })));
    } else {
      throw new Error("Usage: make-profile-code.mjs [--v1] profile.json | --decode CODE");
    }
  } catch (e) {
    console.error(e.message);
    process.exit(1);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) main(process.argv.slice(2));
