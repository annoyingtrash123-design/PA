import { api, assistantName, getState, now, today } from "../core";
import { getBriefing, getDigest, type Digest } from "../services/ai-text";
import { onContextUpdate } from "../services/events";
import { icon, weatherIconName } from "../icons";
import { card, chip, clear, h, readAloud, segmented } from "../ui";
import { greeting, longDay } from "../util/dates";
import { pickNextUp, PART_LABELS } from "../util/nextup";
import { paragraphs } from "../util/text";
import type { Day } from "../types";

type Kind = "morning" | "evening";
let kind: Kind | null = null;

export function renderHome(root: HTMLElement): () => void {
  const clock = now();
  if (!kind) kind = clock.now < "17:00" ? "morning" : "evening";
  const name = assistantName();
  const user = getState().settings.user_name;
  const hello = user ? `${greeting(clock.now)}, ${user}` : greeting(clock.now);

  const day = api<Day[]>("GET", `/api/days?start=${today()}&days=1`)[0];
  const items = day.items;
  const done = items.filter((o) => o.done).length;
  const next = pickNextUp(items, clock.now);

  const weatherHolder = h("div", { hidden: true });
  const dayHolder = h("div");
  let token = 0;
  let digest: Digest | null = null;

  const progress = () => {
    const total = items.length;
    const R = 26, C = 2 * Math.PI * R;
    const ring = h("div", { class: "ring-wrap" });
    ring.innerHTML = `<svg width="64" height="64" viewBox="0 0 64 64" aria-hidden="true"><circle cx="32" cy="32" r="${R}" fill="none" stroke="var(--lav-soft)" stroke-width="7"/><circle cx="32" cy="32" r="${R}" fill="none" stroke="var(--lav)" stroke-width="7" stroke-linecap="round" stroke-dasharray="${total ? (done / total) * C : 0} ${C}" transform="rotate(-90 32 32)"/></svg>`;
    ring.appendChild(h("div", { class: "ring-text" }, `${done}/${total}`));
    let nextCol: HTMLElement;
    if (total === 0) nextCol = h("div", null, h("h3", { class: "title-sm" }, "Nothing planned"), h("p", { class: "muted" }, "A blank canvas. Add something in Tasks."));
    else if (!next) nextCol = h("div", null, h("h3", { class: "title-sm" }, "All done!"), h("p", { class: "muted" }, "Beautiful work today. Time to relax."));
    else nextCol = h("div", { class: "stack", style: { gap: "3px" } }, h("span", { class: "label-caps" }, "Next up"), h("div", { class: "next-title" }, next.title), h("span", { class: "muted" }, next.time ?? PART_LABELS[next.part_of_day || "anytime"]));
    return card("compact", h("div", { class: "row", style: { gap: "16px" } }, ring, h("div", { class: "grow" }, nextCol)));
  };

  const drawWeather = () => {
    const w = digest?.weather;
    weatherHolder.hidden = !w;
    if (!w) return clear(weatherHolder);
    const loc = getState().settings.location_name;
    const chips = [chip(`${Math.round(w.temp_min)}-${Math.round(w.temp_max)}°C`)];
    if ((w.uv_max || 0) >= 3) chips.push(chip(`UV ${Math.round(w.uv_max)}`, { icon: "sun", tone: "sage" }));
    if ((w.precip_prob || 0) >= 40) chips.push(chip(`rain ${Math.round(w.precip_prob)}%`, { icon: "rain", tone: "sage" }));
    clear(weatherHolder, card("raised stack", h("div", { class: "weather-top" }, icon(weatherIconName(w.summary), 26), h("div", null, h("div", { class: "title-sm", style: { color: "var(--text)" } }, w.summary), loc ? h("div", { class: "muted small" }, loc) : null)), h("div", { class: "row wrap", style: { gap: "8px" } }, ...chips), digest?.weather_comment ? h("p", { class: "comment" }, digest.weather_comment) : null));
  };

  const drawDay = (text: string | null | undefined, busy: boolean) => {
    if (busy) return clear(dayHolder, card("raised", h("div", { class: "row", style: { gap: "12px" } }, h("span", { class: "spinner" }), h("span", { class: "muted" }, `${name} is looking at your day…`))));
    const body = text || "I couldn't put your day into words just now. Check the Tasks tab for what's on.";
    const title = kind === "morning" ? "Your day" : "Your wind-down";
    clear(
      dayHolder,
      card(
        "raised summary",
        h("div", { class: "summary-head" }, icon(kind === "morning" ? "sun" : "moon", 20), h("span", { class: "label-caps" }, `${title} with ${name}`)),
        ...paragraphs(body).map((p) => h("p", null, p)),
        readAloud(() => body, () => getState().settings.voice, "Read it to me", !text),
      ),
    );
  };

  const load = async () => {
    const my = ++token;
    drawDay(null, true);
    const date = today();
    const dg = getDigest(date).catch(() => null);
    const br = getBriefing(kind!, date).catch(() => null);
    const d = await dg;
    if (my !== token) return;
    digest = d;
    drawWeather();
    const b = await br;
    if (my !== token) return;
    drawDay(b?.text || digest?.overview, false);
  };

  const segHolder = h("div", { class: "row", style: { justifyContent: "center" } });
  const drawSeg = () =>
    clear(segHolder, segmented<Kind>([["morning", "Morning"], ["evening", "Wind-down"]], kind!, (k) => {
      kind = k;
      drawSeg();
      void load();
    }));
  drawSeg();

  clear(
    root,
    h("div", null, h("h2", { class: "title-lg" }, hello), h("p", { class: "muted", style: { marginTop: "2px", fontSize: "14px" } }, longDay(clock.today))),
    weatherHolder,
    progress(),
    segHolder,
    dayHolder,
  );
  void load();
  return onContextUpdate(() => {
    // weather arrived after first paint: refresh the weather card + day text (cached AI text is reused)
    void load();
  });
}
