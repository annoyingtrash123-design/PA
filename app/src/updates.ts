// Keeps the installed web app up to date without reinstalling: checks for a new service worker at start,
// whenever you come back to the app and every 30 minutes, then reloads onto the new version.
import { registerSW } from "virtual:pwa-register";
import { toast } from "./ui";

export const APP_VERSION = "0.1.0";
const SEEN_KEY = "pa-version-seen";
const CHECK_EVERY_MS = 30 * 60 * 1000;
let pendingReload = false;

/** Pages where an automatic reload would lose what you're doing. */
const busy = () => document.querySelector(".sheet-overlay, .setrow") !== null;

function reloadWhenSafe() {
  if (!busy()) return location.reload();
  pendingReload = true;
  toast("An update is ready. It will apply when you close this screen.", 6000);
}

function announceIfUpdated() {
  try {
    const seen = localStorage.getItem(SEEN_KEY);
    localStorage.setItem(SEEN_KEY, APP_VERSION);
    if (seen && seen !== APP_VERSION) toast(`Updated to v${APP_VERSION}`, 5000);
  } catch {
    // storage unavailable
  }
}

export function setupUpdates() {
  if (!("serviceWorker" in navigator)) return;
  announceIfUpdated();
  registerSW({
    immediate: true,
    onNeedReload: reloadWhenSafe,
    onRegisteredSW(_url, r) {
      if (!r) return;
      const check = () => navigator.onLine && r.update().catch(() => {});
      check();
      setInterval(check, CHECK_EVERY_MS);
      document.addEventListener("visibilitychange", () => document.visibilityState === "visible" && check());
      window.addEventListener("online", check);
    },
  });
  window.addEventListener("hashchange", () => pendingReload && !busy() && location.reload());
}
