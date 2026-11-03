import { assistantName, getState, now, today } from "../core";
import { getBriefing, getDigest, type Digest } from "../services/ai-text";
import { onContextUpdate } from "../services/events";
import { icon, weatherIconName } from "../icons";
import { card, chip, clear, h, readAloud, segmented } from "../ui";
import { greeting, longDay } from "../util/dates";
import { paragraphs } from "../util/text";

type Kind = "morning" | "evening";
let kind: Kind | null = null;

export function renderHome(root: HTMLElement): () => void {
  const clock = now();
  if (!kind) kind = clock.now < "17:00" ? "morning" : "evening";
  const name = assistantName();
  const user = getState().settings.user_name;
  const hello = user ? `${greeting(clock.now)}, ${user}` : greeting(clock.now);

  const weatherHolder = h("div", { hidden: true });
  const dayHolder = h("div");
  let token = 0;
  let digest: Digest | null = null;

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
    segHolder,
    dayHolder,
  );
  void load();
  return onContextUpdate(() => {
    // weather arrived after first paint: refresh the weather card + day text (cached AI text is reused)
    void load();
  });
}
