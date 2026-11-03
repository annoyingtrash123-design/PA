// The editor pass and the memory of recent summaries. Pure (the Gemini call and the stats check are passed in), so it
// can be unit tested without the wasm core or the network.
import type { BriefingPrep } from "../types";
import { addDays } from "../util/dates";
import type { GenerateOptions } from "./gemini";
import { judgeReply } from "./ai-reply";
import { newsIsFaithful } from "./news";
import type { AiParts } from "./morning-script";

type Kind = "morning" | "evening" | "weekly";
/** Room for the five-part JSON reply (same as the writer's). */
const MAX_TOKENS = 8192;

/** The summaries spoken just before this one, newest first: for a morning, last night's, yesterday morning's and the
 *  night before; for an evening, this morning's, last night's and yesterday morning's. */
export function previousSlots(kind: Kind, date: string): { kind: Kind; date: string }[] {
  if (kind === "weekly") return [{ kind: "weekly", date: addDays(date, -7) }];
  const y = addDays(date, -1);
  return kind === "morning"
    ? [{ kind: "evening", date: y }, { kind: "morning", date: y }, { kind: "evening", date: addDays(date, -2) }]
    : [{ kind: "morning", date }, { kind: "evening", date: y }, { kind: "morning", date: y }];
}

/** The block added to the writer's prompt: what was said recently, so it isn't said again the same way. */
export function recentBlock(recent: { label: string; text: string }[], cap = 2500): string {
  if (!recent.length) return "";
  const body = recent.map((r) => `--- ${r.label} ---\n${r.text.slice(0, cap)}`).join("\n");
  return `\n\nRECENTLY SPOKEN SUMMARIES (already heard, newest first; do not reuse their greeting, openings, sign-off, well-wishes, care touches, memories, facts, jokes or turns of phrase; the same tasks may come up again, said freshly):\n${body}`;
}

const words = (s: string | null | undefined) => (s ?? "").split(/\s+/).filter(Boolean).length;

/**
 * The editor pass: Gemini reads the whole draft against the brief and the recent summaries and returns a polished version.
 * Any problem (no budget, an error, a reply that fails the checks) keeps the draft as it is.
 */
export async function reviewDraft(o: {
  prep: BriefingPrep;
  draft: AiParts;
  recent: { label: string; text: string }[];
  jsonMode: boolean;
  generateFn: (o: GenerateOptions) => Promise<string>;
  hasStats: (t: string) => boolean;
}): Promise<AiParts | null> {
  const { prep, draft, recent, jsonMode, hasStats } = o;
  if (!prep.review_prompt) return null;
  const gen = o.generateFn;
  const d = draft;
  const fields: Record<string, string | null> = jsonMode
    ? prep.kind === "morning"
      ? { greeting: d.greeting ?? null, weather_comment: d.weather_comment ?? null, affirmation: d.affirmation ?? null, briefing: d.briefing ?? "", ...(prep.news ? { news: d.news ?? "" } : {}) }
      : { briefing: d.briefing ?? "", ...(prep.news ? { news: d.news ?? "" } : {}) }
    : { briefing: d.briefing ?? "" };
  const user =
    `THE BRIEF THE WRITER FOLLOWED:\n${prep.system_prompt}\n\nTHE CONTEXT IT WAS WRITTEN FROM:\n${prep.user_prompt}` +
    (recent.length ? `\n\nRECENTLY SPOKEN SUMMARIES (newest first):\n${recent.map((r) => `--- ${r.label} ---\n${r.text}`).join("\n")}` : "\n\nRECENTLY SPOKEN SUMMARIES: none on file.") +
    `\n\nDRAFT (JSON):\n${JSON.stringify(fields)}`;
  const raw = await gen({ system: prep.review_prompt, user, json: true, maxTokens: MAX_TOKENS });
  const j = judgeReply(raw, true, hasStats);
  const r = j.r;
  if (!r || !r.briefing || j.salvaged) return null;
  // the editor must not cut the text down or lose the news
  if (words(r.briefing) < words(d.briefing) * 0.8) return null;
  if (prep.news && d.news) {
    if (!r.news || !newsIsFaithful(r.news, prep.news.items.map((i) => i.title)) || words(r.news) < words(d.news) * 0.75) r.news = d.news;
  } else r.news = d.news ?? null;
  if (prep.kind === "morning") {
    r.greeting = r.greeting || d.greeting;
    r.affirmation = r.affirmation || d.affirmation;
    r.weather_comment = d.weather_comment ? r.weather_comment || d.weather_comment : null;
  }
  return { ...r, reviewed: true };
}