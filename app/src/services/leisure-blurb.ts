// One text request per leisure item to write a short spoiler-free blurb (only on add or when tapped; never in bulk).
import { api } from "../core";
import type { Leisure } from "../types";
import { hasKey } from "./cache";
import { cleanReply, generate, GeminiError } from "./gemini";
import { blurbFromReply } from "../util/leisure";

export type BlurbResult = { ok: true; item: Leisure } | { ok: false; message: string };

/** Ask Gemini (one request, counted in the daily budget) and save the blurb on the item. */
export async function fetchBlurb(id: number): Promise<BlurbResult> {
  if (!hasKey()) return { ok: false, message: "Add a Gemini key in Settings to get a short description." };
  try {
    const p = api<{ system_prompt: string; user_prompt: string }>("GET", `/api/leisure/${id}/blurb-prompt`);
    const raw = await generate({ system: p.system_prompt, user: p.user_prompt, maxTokens: 400 });
    const blurb = blurbFromReply(cleanReply(raw));
    if (!blurb) return { ok: false, message: "I couldn't write a description just now." };
    const cur = api<Leisure[]>("GET", "/api/leisure").find((l) => l.id === id);
    if (!cur) return { ok: false, message: "That item is gone." };
    const item = api<Leisure>("PUT", `/api/leisure/${id}`, { kind: cur.kind, title: cur.title, note: cur.note, status: cur.status, finished_on: cur.finished_on, blurb });
    return { ok: true, item };
  } catch (e) {
    return { ok: false, message: e instanceof GeminiError ? e.message : "I couldn't write a description just now." };
  }
}
