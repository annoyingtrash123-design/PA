// Left / right swipes on an element (for flipping between sub-tabs). Ignores vertical scrolls, short drags,
// and swipes that start on things that use their own horizontal movement (inputs, sliders, charts, sheets).
export function onSwipe(el: HTMLElement, cb: (dir: "left" | "right") => void): void {
  let x0 = 0;
  let y0 = 0;
  let t0 = 0;
  let live = false;
  el.addEventListener("touchstart", (e) => {
    const t = e.touches[0];
    const target = e.target as HTMLElement | null;
    live = e.touches.length === 1 && !target?.closest("input, textarea, select, .chart, .sheet-overlay, [data-noswipe]");
    x0 = t.clientX;
    y0 = t.clientY;
    t0 = Date.now();
  }, { passive: true });
  el.addEventListener("touchend", (e) => {
    if (!live) return;
    live = false;
    const t = e.changedTouches[0];
    const dx = t.clientX - x0;
    const dy = t.clientY - y0;
    if (Math.abs(dx) >= 60 && Math.abs(dx) > 1.6 * Math.abs(dy) && Date.now() - t0 < 800) cb(dx < 0 ? "left" : "right");
  }, { passive: true });
}
