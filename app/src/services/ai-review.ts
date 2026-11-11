// The memory of recent summaries: what was said lately, so a new summary doesn't say it again the same way. Pure.
import { addDays } from "../util/dates";

type Kind = "morning" | "evening" | "weekly";

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
