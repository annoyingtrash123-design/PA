import { describe, expect, it } from "vitest";
import { blurbFromReply, finishedItems, openItems, UNKNOWN_BLURB } from "./leisure";
import type { Leisure } from "../types";

const mk = (id: number, kind: Leisure["kind"], status: Leisure["status"], finished_on: string | null = null): Leisure => ({ id, kind, title: `T${id}`, note: "", status, created: "2026-11-01", finished_on, last_suggested: null, blurb: "" });

describe("leisure helpers", () => {
  const items = [mk(1, "read", "someday"), mk(2, "watch", "current"), mk(3, "read", "finished", "2026-10-01"), mk(4, "read", "current"), mk(5, "play", "finished", "2026-11-01")];
  it("filters by kind and puts current first, hiding finished", () => {
    expect(openItems(items, "all").map((l) => l.id)).toEqual([2, 4, 1]);
    expect(openItems(items, "read").map((l) => l.id)).toEqual([4, 1]);
    expect(openItems(items, "try")).toEqual([]);
  });
  it("lists finished items newest first", () => {
    expect(finishedItems(items).map((l) => l.id)).toEqual([5, 3]);
  });
  it("maps the UNKNOWN reply to the fallback blurb", () => {
    expect(blurbFromReply("UNKNOWN")).toBe(UNKNOWN_BLURB);
    expect(blurbFromReply("  unknown. ")).toBe(UNKNOWN_BLURB);
    expect(blurbFromReply("\"UNKNOWN\"")).toBe(UNKNOWN_BLURB);
  });
  it("keeps a real blurb, tidied, and rejects empty replies", () => {
    expect(blurbFromReply("It's a quiet novel.\n\nYou might like it.")).toBe("It's a quiet novel. You might like it.");
    expect(blurbFromReply("   ")).toBeNull();
    expect(blurbFromReply("Unknown territory is what this game is about, and it's a lovely one for explorers of all kinds.")).not.toBe(UNKNOWN_BLURB);
  });
});
