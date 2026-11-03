import { assistantName, getState, now, today } from "../core";
import type { BriefingResult } from "../services/ai-text";
import { affirmationFor, aiNotice, getBriefing, templateBriefing, templateDigest, type Digest } from "../services/ai-text";
import { hasKey } from "../services/cache";
import { onContextUpdate } from "../services/events";
import { icon, weatherIconName } from "../icons";
import { button, card, chip, clear, h, readAloud, segmented, toast } from "../ui";
import { greeting, longDay } from "../util/dates";
import { paragraphs } from "../util/text";

type Kind = "morning" | "evening";
let kind: Kind | null = null;

export function renderHome(root: HTMLElement): () => void {
  const clock = now();
  if (!kind) kind = clock.now < "21:00" ? "morning" : "evening";
  const name = assistantName();
  const user = getState().settings.user_name;
  const hello = user ? `${greeting(clock.now)}, ${user}` : greeting(clock.now);

  const weatherHolder = h("div", { hidden: true });
  const dayHolder = h("div");
  const playHolder = h("div", { class: "play-top" });
  let token = 0;
  let digest: Digest | null = null;
  const affHolder = h("div", { hidden: true });
  /** what "Read it to me" says right now: morning = greeting, weather, affirmation, briefing; evening = the wind-down text */
  let script = "";

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

  const drawAffirmation = (text: string | null | undefined) => {
    affHolder.hidden = !text;
    if (!text) return clear(affHolder);
    clear(affHolder, card("affirmation", h("div", { class: "aff-head" }, icon("leaf", 18), h("span", { class: "label-caps" }, "For today")), h("p", { class: "aff-text" }, text)));
  };

  const drawDay = (text: string | null | undefined, greetingLine: string | null | undefined, busy: boolean, note?: string, resting?: string | null, onUpdate?: (() => void) | null) => {
    if (busy) {
      clear(playHolder, readAloud(() => "", () => getState().settings.voice, "Read it to me", true));
      return clear(dayHolder, card("raised", h("div", { class: "row", style: { gap: "12px" } }, h("span", { class: "spinner" }), h("span", { class: "muted" }, `${name} is looking at your day…`))));
    }
    const body = text || "I couldn't put your day into words just now. Check the Tasks tab for what's on.";
    const title = kind === "morning" ? "Your day" : "Your wind-down";
    clear(
      dayHolder,
      card(
        "raised summary",
        h("div", { class: "summary-head" }, icon(kind === "morning" ? "sun" : "moon", 20), h("span", { class: "label-caps" }, `${title} with ${name}`)),
        note ? h("div", { class: "row muted small", style: { gap: "8px" } }, h("span", { class: "spinner" }), note) : null,
        greetingLine ? h("p", { class: "day-greeting" }, greetingLine) : null,
        ...paragraphs(body).map((p) => h("p", null, p)),
        resting ? h("p", { class: "muted small" }, resting) : null,
        onUpdate ? button("Plans changed — update summary", onUpdate, { kind: "text", cls: "small" }) : null,
      ),
    );
    clear(playHolder, readAloud(() => script || body, () => getState().settings.voice, kind === "morning" ? "Read it to me" : "Read my wind-down", !text));
  };

  /** Paint everything a briefing result holds: weather comment, affirmation card, "Your day" card, and the read-aloud script. */
  const show = async (b: BriefingResult, date: string, note?: string) => {
    const my = token;
    const p = b.parts;
    if (digest && p?.weather_comment) digest = { ...digest, weather_comment: p.weather_comment };
    drawWeather();
    // morning: the affirmation comes with the briefing; wind-down: the morning's (cached) or the template one
    const aff = p ? p.affirmation : await affirmationFor(date).catch(() => null);
    if (my !== token) return;
    drawAffirmation(aff);
    script = b.script;
    const update = b.outdated ? () => void regenerate(date) : null;
    drawDay(b.text, p?.greeting, false, note, b.source === "template" ? aiNotice() : null, update);
  };
  const regenerate = async (date: string) => {
    const my = ++token;
    drawDay(null, null, true);
    const b = await getBriefing(kind!, date, { force: true }).catch(() => null);
    if (my !== token) return;
    if (b?.notice) toast(b.notice);
    await show(b ?? templateBriefing(kind!, date), date);
  };

  const load = async () => {
    const my = ++token;
    const date = today();
    // show the instant template version straight away; the AI version replaces it when it arrives
    let tpl: BriefingResult | null = null;
    try {
      digest = templateDigest(date);
      tpl = templateBriefing(kind!, date);
      await show(tpl, date, hasKey() ? `${name} is putting this into words…` : undefined);
    } catch {
      drawDay(null, null, true);
    }
    // Only the first text of the day is automatic (the morning one writes all four parts); later changes need a tap
    const b = await getBriefing(kind!, date).catch(() => null);
    if (my !== token) return;
    await show(b ?? tpl ?? templateBriefing(kind!, date), date);
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
    affHolder,
    playHolder,
    segHolder,
    dayHolder,
  );
  void load();
  return onContextUpdate(() => {
    // weather arrived after first paint: refresh the weather card; the stored AI text is reused, never rewritten
    void load();
  });
}
