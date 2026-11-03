// Judging one AI reply for a briefing: parse it tolerantly, then apply the "no raw counts" guard to the plan text only.
// Pure (the stats check is passed in), so it can be unit tested without the wasm core.
import { looksTruncated, parseMorningJson } from "./gemini";
import { whyCutOff, whyParse, whyStats, type Why } from "./ai-reason";
import type { AiParts } from "./morning-script";

/** What the stats guard looks at: everything spoken EXCEPT the news segment (it quotes headlines word for word, numbers and all). */
export const guardText = (r: AiParts): string => [r.greeting, r.weather_comment, r.affirmation, r.briefing].filter(Boolean).join("\n");

export interface Judged {
  /** the usable parts (when `why` is null) */
  r: AiParts | null;
  why: Why | null;
  /** the reply was cut off but complete fields were kept */
  salvaged?: boolean;
}

/** Parse + guard. `jsonMode` replies carry parts; plain replies are the briefing itself. */
export function judgeReply(raw: string, jsonMode: boolean, hasStats: (t: string) => boolean): Judged {
  let r: AiParts | null;
  let salvaged = false;
  if (!jsonMode) r = raw.trim() ? { briefing: raw } : null;
  else {
    const p = parseMorningJson(raw);
    r = p;
    salvaged = !!p?.salvaged;
  }
  if (!r || !r.briefing) return { r: null, why: looksTruncated(raw) ? whyCutOff() : whyParse() };
  if (hasStats(guardText(r))) return { r: null, why: whyStats() };
  return { r, why: null, salvaged };
}
