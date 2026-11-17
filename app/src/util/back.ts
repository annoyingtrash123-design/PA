// The phone's back button / gesture inside the app. The app keeps one extra history entry (the "trap") so that
// back comes to us instead of leaving the app straight away. Each press does the first of these that applies:
//   1. close an open sheet;
//   2. reset the tab: back to its first view (Gym > Today, Tasks > this week…) and to the top of the page;
//   3. go to Home;
//   4. on Home with nothing to reset, leave the app.
// So from Gym > Templates: back -> Gym Today, back -> Home, back -> out of the app.

export type BackAction = "close-sheet" | "reset" | "home" | "leave";

/** What one back press does (pure, for tests). `canReset`: the tab isn't on its first view, or the page is scrolled. */
export function decide(o: { sheetOpen: boolean; canReset: boolean; onHome: boolean }): BackAction {
  if (o.sheetOpen) return "close-sheet";
  if (o.canReset) return "reset";
  return o.onHome ? "leave" : "home";
}

// The showing tab can register how to go back to its first view: returns true if there was anything to undo.
let tabReset: (() => boolean) | null = null;
export const setTabReset = (fn: (() => boolean) | null) => void (tabReset = fn);
/** Whether the showing tab is away from its first view (asked without changing anything). */
let tabAway: (() => boolean) | null = null;
export const setTabAway = (fn: (() => boolean) | null) => void (tabAway = fn);

const TRAP = { pa: "trap" };

/** Set up the back handling. `onHome()` says whether Home is showing; `goHome()` switches to it. */
export function setupBack(onHome: () => boolean, goHome: () => void, exit?: () => void): { ensureTrap: () => void; skipHashChange: () => boolean; clearTab: () => void } {
  // the page that is showing; the entry under the trap may still carry an older address, which must not re-route
  let shown = location.href;
  let skipHash = false;
  const ensureTrap = () => {
    shown = location.href;
    if ((history.state as { pa?: string } | null)?.pa !== "trap") history.pushState(TRAP, "", location.href);
  };
  window.addEventListener("popstate", () => {
    if (location.href !== shown) {
      skipHash = true;
      history.replaceState(history.state, "", shown);
    }
    const sheet = document.querySelector<HTMLElement>(".sheet-overlay:last-of-type");
    const scrolled = window.scrollY > 5;
    const act = decide({ sheetOpen: !!sheet, canReset: scrolled || !!tabAway?.(), onHome: onHome() });
    if (act === "leave") {
      // we are on the entry below the trap: one more step back leaves the app (the Android app closes itself)
      skipHash = false;
      if (exit) exit();
      else history.back();
      return;
    }
    history.pushState(TRAP, "", location.href);
    if (act === "close-sheet") sheet!.click();
    else if (act === "reset") {
      tabReset?.();
      window.scrollTo({ top: 0, behavior: "smooth" });
    } else goHome();
  });
  // a hash change caused by our own back handling is not a navigation
  const skipHashChange = () => {
    const s = skipHash;
    skipHash = false;
    return s;
  };
  const clearTab = () => {
    tabReset = null;
    tabAway = null;
  };
  return { ensureTrap, skipHashChange, clearTab };
}
