import { describe, expect, it } from "vitest";
import { isNewerVersion, parseReleaseVersion } from "./app-update";

describe("app update check", () => {
  it("reads the version line of the release notes", () => {
    expect(parseReleaseVersion("Miranda Android app (latest build)\n\nversion: 2026.11.17-12\n\nCommit: abc")).toBe("2026.11.17-12");
    expect(parseReleaseVersion("nothing here")).toBeNull();
  });
  it("offers an update only when the version differs and this isn't a dev build", () => {
    expect(isNewerVersion("2026.11.17-12", "2026.11.16-11")).toBe(true);
    expect(isNewerVersion("2026.11.17-12", "2026.11.17-12")).toBe(false);
    expect(isNewerVersion("2026.11.17-12", "dev")).toBe(false);
    expect(isNewerVersion(null, "2026.11.16-11")).toBe(false);
  });
});
