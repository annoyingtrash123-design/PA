import { describe, expect, it } from "vitest";
import { decide } from "./back";

describe("the back button", () => {
  it("first closes a sheet, then goes to the top, then home; on Home the second press leaves", () => {
    expect(decide({ sheetOpen: true, armed: true, onHome: false })).toBe("close-sheet");
    expect(decide({ sheetOpen: false, armed: false, onHome: false })).toBe("to-top");
    expect(decide({ sheetOpen: false, armed: true, onHome: false })).toBe("home");
    expect(decide({ sheetOpen: false, armed: false, onHome: true })).toBe("to-top");
    expect(decide({ sheetOpen: false, armed: true, onHome: true })).toBe("leave");
  });
});
