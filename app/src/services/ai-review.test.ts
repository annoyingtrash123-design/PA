import { describe, expect, it } from "vitest";
import { previousSlots, recentBlock } from "./ai-review";

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
