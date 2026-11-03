// "Install this app" banner. Android/desktop Chrome fire `beforeinstallprompt`; iOS Safari gets instructions.
import { button, h } from "./ui";

interface InstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: "accepted" | "dismissed" }>;
}

const DISMISS_KEY = "pa-install-dismissed";
const DISMISS_DAYS = 7;
let deferred: InstallPromptEvent | null = null;

export const isStandalone = () => window.matchMedia?.("(display-mode: standalone)").matches || (navigator as unknown as { standalone?: boolean }).standalone === true;
export const isIos = () => /iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1);

function dismissedRecently(): boolean {
  try {
    return Date.now() - Number(localStorage.getItem(DISMISS_KEY) || 0) < DISMISS_DAYS * 86_400_000;
  } catch {
    return false;
  }
}

function showBanner() {
  if (isStandalone() || dismissedRecently() || document.querySelector(".install-banner")) return;
  const ios = isIos();
  if (!ios && !deferred) return;
  const close = () => {
    banner.remove();
    try {
      localStorage.setItem(DISMISS_KEY, String(Date.now()));
    } catch {
      // ignore
    }
  };
  const banner = h(
    "div",
    { class: "install-banner", role: "dialog", "aria-label": "Install app" },
    h("img", { src: `${import.meta.env.BASE_URL}icon-192.png`, alt: "" }),
    h("div", { class: "install-text" }, h("strong", null, "Install Miranda"), h("span", null, ios ? "Tap the Share button, then “Add to Home Screen”." : "Add it to your home screen. Works offline.")),
    ios ? null : button("Install", async () => { const e = deferred; deferred = null; banner.remove(); if (e) { await e.prompt(); await e.userChoice; } }),
    h("button", { type: "button", class: "icon-btn sm", "aria-label": "Close", onclick: close }, "×"),
  );
  document.body.appendChild(banner);
}

export function setupInstall() {
  window.addEventListener("beforeinstallprompt", (e) => {
    e.preventDefault();
    deferred = e as InstallPromptEvent;
    showBanner();
  });
  window.addEventListener("appinstalled", () => {
    deferred = null;
    document.querySelector(".install-banner")?.remove();
  });
  if (isIos()) setTimeout(showBanner, 2000);
}
