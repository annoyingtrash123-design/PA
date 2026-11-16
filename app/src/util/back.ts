// The phone's back button / gesture inside the app. The app keeps one extra history entry (the "trap") so that
// back comes to us instead of leaving: a first press closes an open sheet or scrolls the page back to the top; a
// second press goes to Home; on Home, a second press leaves the app as usual. Pure decision logic is in `decide`
// so it can be tested without a browser.

export type BackAction = "close-sheet" | "to-top" | "home" | "leave";

/** What one back press does, given what is on screen and whether the previous press already scrolled to the top. */
export function decide(o: { sheetOpen: boolean; armed: boolean; onHome: boolean }): BackAction {
  if (o.sheetOpen) return "close-sheet";
  if (!o.armed) return "to-top";
  return o.onHome ? "leave" : "home";
}

const TRAP = { pa: "trap" };

/** Set up the back handling. `onHome()` says whether Home is showing; `goHome()` switches to it. */
export function setupBack(onHome: () => boolean, goHome: () => void): { ensureTrap: () => void; skipHashChange: () => boolean } {
  let armed = false;
  // the page that is showing; the entry under the trap may still carry an older address, which must not re-route
  let shown = location.href;
  let skipHash = false;
  // scrolling or a new screen starts over: the next back press goes to the top again
  let armedAt = 0;
  window.addEventListener("scroll", () => {
    if (armed && Date.now() - armedAt > 400 && window.scrollY > 40) armed = false;
  }, { passive: true });
  const ensureTrap = () => {
    armed = false;
    shown = location.href;
    if ((history.state as { pa?: string } | null)?.pa !== "trap") history.pushState(TRAP, "", location.href);
  };
  window.addEventListener("popstate", () => {
    if (location.href !== shown) {
      skipHash = true;
      history.replaceState(history.state, "", shown);
    }
    const sheet = document.querySelector<HTMLElement>(".sheet-overlay:last-of-type");
    const act = decide({ sheetOpen: !!sheet, armed, onHome: onHome() });
    if (act === "leave") {
      // we are on the entry below the trap: one more step back leaves the app
      skipHash = false;
      history.back();
      return;
    }
    history.pushState(TRAP, "", location.href);
    if (act === "close-sheet") sheet!.click();
    else if (act === "to-top") {
      window.scrollTo({ top: 0, behavior: "smooth" });
      armed = true;
      armedAt = Date.now();
    } else goHome();
  });
  // a hash change caused by our own back handling is not a navigation
  const skipHashChange = () => {
    const s = skipHash;
    skipHash = false;
    return s;
  };
  return { ensureTrap, skipHashChange };
}
