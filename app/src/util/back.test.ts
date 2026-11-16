import { describe, expect, it } from "vitest";
import { decide } from "./back";

describe("the back button", () => {
  it("closes a sheet, then resets the tab, then goes Home, then leaves", () => {
    expect(decide({ sheetOpen: true, canReset: true, onHome: false })).toBe("close-sheet");
    expect(decide({ sheetOpen: false, canReset: true, onHome: false })).toBe("reset");
    expect(decide({ sheetOpen: false, canReset: false, onHome: false })).toBe("home");
    expect(decide({ sheetOpen: false, canReset: true, onHome: true })).toBe("reset");
    expect(decide({ sheetOpen: false, canReset: false, onHome: true })).toBe("leave");
  });
});
