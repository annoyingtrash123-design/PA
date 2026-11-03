import { describe, expect, it } from "vitest";
import { pickNextUp } from "./nextup";

const o = (title: string, time: string | null, part: string | null, done = false) => ({ title, time, part_of_day: part as never, done });

describe("pickNextUp", () => {
  it("returns the first upcoming timed item", () => {
    const items = [o("a", "07:30", "morning"), o("b", "10:00", "morning"), o("c", "17:30", "afternoon")];
    expect(pickNextUp(items, "09:00")?.title).toBe("b");
    expect(pickNextUp(items, "10:00")?.title).toBe("b");
  });
  it("skips done items", () => {
    expect(pickNextUp([o("a", "10:00", "morning", true), o("b", "11:00", "morning")], "09:00")?.title).toBe("b");
  });
  it("falls back to an untimed item for the current or later part of day", () => {
    const items = [o("late", "08:00", "morning"), o("eve", null, "evening"), o("morn", null, "morning")];
    expect(pickNextUp(items, "13:00")?.title).toBe("eve");
    expect(pickNextUp([o("morn", null, "morning"), o("any", null, "anytime")], "13:00")?.title).toBe("any");
  });
  it("falls back to the first untimed, then to any remaining", () => {
    expect(pickNextUp([o("x", "07:00", "morning"), o("m", null, "morning")], "20:00")?.title).toBe("m");
    expect(pickNextUp([o("x", "07:00", "morning")], "20:00")?.title).toBe("x");
  });
  it("returns null when everything is done or empty", () => {
    expect(pickNextUp([], "09:00")).toBeNull();
    expect(pickNextUp([o("a", "10:00", "morning", true)], "09:00")).toBeNull();
  });
});
