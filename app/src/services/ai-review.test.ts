import { describe, expect, it } from "vitest";
import { previousSlots, recentBlock, reviewDraft } from "./ai-review";
import type { BriefingPrep } from "../types";

const noStats = () => false;
const prep = (over: Partial<BriefingPrep> = {}): BriefingPrep =>
  ({
    kind: "evening",
    system_prompt: "BRIEF",
    user_prompt: "CONTEXT",
    review_prompt: "EDITOR",
    news: { items: [{ title: "Ferries return to the harbour" }, { title: "Rain eases in the west" }] },
    ...over,
  }) as unknown as BriefingPrep;
const long = (w: string, n: number) => Array.from({ length: n }, () => w).join(" ");
const draftNews = `From the ABC, the headline is: "Ferries return to the harbour". ${long("so", 40)} And from the BBC: "Rain eases in the west". ${long("ok", 40)}`;

describe("recent summaries", () => {
  it("a morning looks back at last night and the day before; an evening at this morning first", () => {
    expect(previousSlots("morning", "2026-11-04")).toEqual([
      { kind: "evening", date: "2026-11-03" },
      { kind: "morning", date: "2026-11-03" },
      { kind: "evening", date: "2026-11-02" },
    ]);
    expect(previousSlots("evening", "2026-11-04")[0]).toEqual({ kind: "morning", date: "2026-11-04" });
    expect(previousSlots("weekly", "2026-11-08")).toEqual([{ kind: "weekly", date: "2026-11-01" }]);
  });
  it("the writer is told what was said recently", () => {
    expect(recentBlock([])).toBe("");
    const b = recentBlock([{ label: "evening of 2026-11-03", text: "Sleep well, and I'll see you in the morning." }]);
    expect(b).toContain("RECENTLY SPOKEN SUMMARIES");
    expect(b).toContain("see you in the morning");
  });
});

describe("the editor pass", () => {
  it("sends the brief, context, recent summaries and the draft, and takes the polished text", async () => {
    let sent = "";
    const out = await reviewDraft({
      prep: prep(),
      draft: { briefing: long("word", 100), news: draftNews },
      recent: [{ label: "morning of 2026-11-04", text: "Morning! Grab a cuppa." }],
      jsonMode: true,
      hasStats: noStats,
      generateFn: async (o) => {
        sent = o.system + "|" + o.user;
        return JSON.stringify({ briefing: long("better", 100), news: draftNews.replace("so so", "so, so") });
      },
    });
    expect(sent).toContain("EDITOR|THE BRIEF THE WRITER FOLLOWED:\nBRIEF");
    expect(sent).toContain("Grab a cuppa");
    expect(sent).toContain("DRAFT (JSON)");
    expect(out?.briefing).toContain("better");
    expect(out?.reviewed).toBe(true);
  });
  it("keeps the draft's news when the editor rewords a headline", async () => {
    const out = await reviewDraft({
      prep: prep(),
      draft: { briefing: long("word", 100), news: draftNews },
      recent: [],
      jsonMode: true,
      hasStats: noStats,
      generateFn: async () => JSON.stringify({ briefing: long("better", 100), news: "Ferries are back, and the rain eased." }),
    });
    expect(out?.news).toBe(draftNews);
  });
  it("rejects a reply that cuts the text down, reads out counts or isn't JSON", async () => {
    const base = { prep: prep(), draft: { briefing: long("word", 100), news: draftNews }, recent: [], jsonMode: true };
    expect(await reviewDraft({ ...base, hasStats: noStats, generateFn: async () => JSON.stringify({ briefing: long("short", 40) }) })).toBeNull();
    expect(await reviewDraft({ ...base, hasStats: () => true, generateFn: async () => JSON.stringify({ briefing: long("x", 100) }) })).toBeNull();
    expect(await reviewDraft({ ...base, hasStats: noStats, generateFn: async () => "sorry, I can't" })).toBeNull();
  });
  it("does nothing without an editor brief", async () => {
    const out = await reviewDraft({ prep: prep({ review_prompt: undefined }), draft: { briefing: "x" }, recent: [], jsonMode: true, hasStats: noStats, generateFn: async () => "{}" });
    expect(out).toBeNull();
  });
});
