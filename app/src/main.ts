import "./styles.css";
import { assistantName, startCore, today } from "./core";
import { icon } from "./icons";
import { setupInstall } from "./install";
import { refreshHolidays } from "./services/holidays";
import { stop as stopSpeech } from "./services/speech";
import { refreshWeather } from "./services/weather";
import { clear, h, loading } from "./ui";
import { setupUpdates } from "./updates";
import { renderGoals } from "./views/goals";
import { renderGym } from "./views/gym";
import { renderHome } from "./views/home";
import { renderSettings } from "./views/settings";
import { renderTasks } from "./views/tasks";

setupInstall();
setupUpdates();

type View = (root: HTMLElement) => void | (() => void);
const routes: [string, View, string][] = [
  ["/", renderHome, "home"],
  ["/tasks", renderTasks, "tasks"],
  ["/goals", renderGoals, "goals"],
  ["/gym", renderGym, "gym"],
  ["/settings", renderSettings, "settings"],
];
const TABS = [
  { id: "home", hash: "#/", icon: "home", label: "Home" },
  { id: "tasks", hash: "#/tasks", icon: "tasks", label: "Tasks" },
  { id: "goals", hash: "#/goals", icon: "compass", label: "Plans" },
  { id: "gym", hash: "#/gym", icon: "gym", label: "Gym" },
  { id: "settings", hash: "#/settings", icon: "settings", label: "Settings" },
];

// Hide the tab bar while the on-screen keyboard is up.
{
  let tallest = 0;
  const check = () => {
    const hgt = window.visualViewport?.height ?? window.innerHeight;
    tallest = Math.max(tallest, hgt);
    document.body.classList.toggle("kb", tallest - hgt > 150);
  };
  window.visualViewport?.addEventListener("resize", check);
  window.addEventListener("orientationchange", () => { tallest = 0; setTimeout(check, 300); });
}

const topbar = document.getElementById("topbar")!;
const view = document.getElementById("view")!;
const tabs = document.getElementById("tabs")!;

function drawChrome(active: string) {
  clear(topbar, h("h1", null, assistantName()));
  clear(tabs, ...TABS.map((t) => h("a", { href: t.hash, class: t.id === active ? "active" : "", "aria-current": t.id === active ? "page" : null }, h("span", { class: "pill" }, icon(t.icon, 24)), t.label)));
}

let cleanup: (() => void) | void;
let renderedDay = "";
function route() {
  const path = location.hash.replace(/^#/, "").split("?")[0] || "/";
  const entry = routes.find(([p]) => p === path);
  if (!entry) {
    // old bookmarks: the Review tab now lives on Home
    location.hash = "#/";
    return;
  }
  cleanup?.();
  cleanup = undefined;
  stopSpeech();
  document.querySelectorAll(".sheet-overlay").forEach((el) => el.remove());
  document.body.classList.remove("modal-open");
  drawChrome(entry[2]);
  const root = h("div", { class: "page" }, loading());
  clear(view, root);
  window.scrollTo(0, 0);
  try {
    clear(root);
    cleanup = entry[1](root);
    renderedDay = today();
  } catch (e) {
    clear(root, h("div", { class: "card stack" }, h("h2", { class: "title-sm" }, "Hmm, something went wrong"), h("p", { class: "muted" }, String((e as Error).message ?? e)), h("button", { type: "button", class: "btn btn-primary", onclick: route }, "Retry")));
  }
}

async function start() {
  clear(view, h("div", { class: "page" }, loading("Waking up…")));
  try {
    await startCore();
  } catch (e) {
    clear(view, h("div", { class: "card" }, `Could not start the app core: ${String((e as Error).message ?? e)}`));
    return;
  }
  window.addEventListener("hashchange", route);
  window.addEventListener("pa-settings", () => { const t = topbar.querySelector("h1"); if (t) t.textContent = assistantName(); });
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && renderedDay && renderedDay !== today() && !document.querySelector(".sheet-overlay")) route();
  });
  route();
  void refreshWeather();
  void refreshHolidays();
}

void start();
