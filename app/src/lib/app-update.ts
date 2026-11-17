import { IS_NATIVE } from "./native";

const RELEASE_API = "https://api.github.com/repos/annoyingtrash123-design/PA/releases/tags/android-latest";

/** The `version: <versionName>` line of the release body the Android workflow writes. */
export function parseReleaseVersion(body: string | null | undefined): string | null {
  const m = /^\s*version:\s*(\S+)\s*$/im.exec(body ?? "");
  return m ? m[1] : null;
}

/** True when `latest` differs from the running build (dev builds never prompt). */
export function isNewerVersion(latest: string | null, current: string): boolean {
  return !!latest && current !== "dev" && latest !== current;
}

/** A newer APK on the `android-latest` release, or null (web, up to date, offline, dev build). */
export async function checkAppUpdate(): Promise<{ latest: string; url: string } | null> {
  if (!IS_NATIVE) return null;
  try {
    const r = await fetch(RELEASE_API, { headers: { Accept: "application/vnd.github+json" } });
    if (!r.ok) return null;
    const j = (await r.json()) as { body?: string; html_url?: string; assets?: { name: string; browser_download_url: string }[] };
    const latest = parseReleaseVersion(j.body);
    if (!isNewerVersion(latest, __APP_VERSION__)) return null;
    const apk = j.assets?.find((a) => a.name === "miranda.apk");
    return { latest: latest!, url: apk?.browser_download_url ?? j.html_url ?? "" };
  } catch {
    return null;
  }
}
