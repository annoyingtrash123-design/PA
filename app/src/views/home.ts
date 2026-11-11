import { api, assistantName, getState, now, today } from "../core";
import type { Day, PartOfDay } from "../types";
import { checkinAnsweredOn, markCheckin, needsSlot, showCheckin } from "../util/checkin";
import type { BriefingResult } from "../services/ai-text";
import { affirmationFor, getBriefing, refreshNews, templateBriefing, templateDigest, type Digest, type NewsView } from "../services/ai-text";
import { sourceLabel } from "../services/ai-reason";
import { sourcesLine } from "../services/news";
import { hasKey } from "../services/cache";
import { onContextUpdate } from "../services/events";
import { icon, weatherIconName } from "../icons";
import { button, card, chip, clear, h, readAloud, segmented, toast } from "../ui";
import { addDays, greeting, longDay } from "../util/dates";
import { paragraphs } from "../util/text";
import { weekCard } from "./week-card";

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
  const checkinHolder = h("div");
  const playHolder = h("div", { class: "play-top" });
  let token = 0;
  /** the view (kind:date) whose final text is on screen */
  let shown = "";
  let digest: Digest | null = null;
  const affHolder = h("div", { hidden: true });
  /** what "Read it to me" says right now: morning = greeting, weather, affirmation, briefing; evening = the wind-down text */
  let script = "";
  /** the spoken sections of what is shown (each is read as one clip); undefined = split by length */
  let sections: string[] | undefined;

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

  /** The news segment inside the "Your day" card: a small muted label, the text, then sources + updated time. */
  const newsBlock = (n: NewsView, date: string): HTMLElement => {
    const meta = [sourcesLine(n.sources), n.updated ? `Updated ${n.updated}` : ""].filter(Boolean).join(" · ");
    let btn: HTMLElement | null = null;
    btn = n.canRefresh
      ? button("Get the latest news", () => {
          btn?.setAttribute("disabled", "true");
          void (async () => {
            const r = await refreshNews(kind!, date).catch(() => ({ done: false, notice: "Couldn't get the news just now." }));
            if (r.notice) toast(r.notice);
            if (r.done) void load();
            else btn?.removeAttribute("disabled");
          })();
        }, { kind: "text", cls: "small" })
      : null;
    return h("div", { class: "news-block stack", style: { gap: "8px" } }, h("p", { class: "label-caps muted" }, "In the news"), ...paragraphs(n.text).map((p) => h("p", null, p)), meta ? h("p", { class: "muted small" }, meta) : null, btn);
  };

  const drawDay = (text: string | null | undefined, greetingLine: string | null | undefined, busy: boolean, note?: string, resting?: string | null, onUpdate?: (() => void) | null, b?: BriefingResult, date?: string) => {
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
        resting ? h("p", { class: "muted small", "data-testid": "source-label" }, resting) : null,
        greetingLine ? h("p", { class: "day-greeting" }, greetingLine) : null,
        ...(b?.news && b.layout && text
          ? [...paragraphs(b.layout.before).map((p) => h("p", null, p)), newsBlock(b.news, date!), ...paragraphs(b.layout.after).map((p) => h("p", null, p))]
          : paragraphs(body).map((p) => h("p", null, p))),
        onUpdate ? button(b && b.source === "template" ? "Try again" : "Plans changed — update summary", onUpdate, { kind: "text", cls: "small" }) : null,
      ),
    );
    clear(playHolder, readAloud(() => script || body, () => getState().settings.voice, kind === "morning" ? "Read it to me" : "Read my wind-down", !text, true, { sections: () => sections }));
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
    sections = b.sections;
    // a built-in text because Google hiccuped (busy, slow, garbled): offer a manual retry
    const retryable = b.source === "template" && hasKey() && !!b.why && ["http", "timeout", "network", "empty", "parse", "cut-off", "other"].includes(b.why.code);
    const update = b.outdated || retryable ? () => void regenerate(date) : null;
    drawDay(b.text, p?.greeting, false, note, note ? null : sourceLabel(b.source, b.why, name, b.newsWhy), update, b, date);
  };
  const regenerate = async (date: string) => {
    const my = ++token;
    drawDay(null, null, true);
    const b = await getBriefing(kind!, date, { force: true }).catch(() => null);
    if (my !== token) return;
    if (b?.notice) toast(b.notice);
    await show(b ?? templateBriefing(kind!, date), date);
  };

  /** The evening check-in: one quick question per unplaced task of tomorrow. No AI is used. */
  const drawCheckin = (day: Day, onDone: () => void) => {
    const items = needsSlot(day);
    const parts: [PartOfDay, string][] = day.work ? [["morning", "Before work"], ["afternoon", "Afternoon"], ["evening", "After work"]] : [["morning", "Morning"], ["afternoon", "Afternoon"], ["evening", "Evening"]];
    const rows = items.map((o) => {
      const chips = h("div", { class: "chip-row", style: { marginTop: "6px" } });
      const paint = (sel: string | null) =>
        clear(
          chips,
          ...parts.map(([p, label]) =>
            button(label, () => {
              try {
                api("PUT", `/api/tasks/${o.task_id}/part`, { part_of_day: p });
                paint(p);
              } catch (e) {
                toast((e as Error).message);
              }
            }, { kind: sel === p ? "tonal" : "outline", cls: "small" }),
          ),
        );
      paint(null);
      return h("div", { class: "missed-item" }, h("div", { class: "occ-title" }, o.title), chips);
    });
    const finish = () => {
      markCheckin(day.date);
      clear(checkinHolder);
      onDone();
    };
    clear(
      checkinHolder,
      card(
        "compact stack",
        h("div", { class: "summary-head" }, icon("event", 20), h("span", { class: "label-caps" }, "Tomorrow check-in")),
        h("p", { class: "muted" }, `Looking at tomorrow: roughly when do these fit? Work things will just happen at work.`),
        ...rows,
        h("div", { class: "row", style: { gap: "8px", justifyContent: "flex-end" } }, button("Skip", finish, { kind: "text" }), button("All set", finish, { kind: "primary", icon: "check" })),
      ),
    );
  };

  const load = async () => {
    const my = ++token;
    const date = today();
    // wind-down: if some of tomorrow's tasks have no slot, ask (once), so the morning summary is ready with the answers.
    // It never holds anything back; answering just changes the plan (the "update summary" button appears if needed).
    clear(checkinHolder);
    if (kind === "evening") {
      try {
        const day = api<Day[]>("GET", `/api/days?start=${addDays(date, 1)}&days=1`)[0];
        if (day && showCheckin(day, checkinAnsweredOn())) drawCheckin(day, () => void load());
      } catch {
        // no check-in: carry on
      }
    }
    // First paint for this view: show the instant built-in version straight away; the AI version replaces it when it
    // arrives. Later refreshes (the weather or the news arriving) keep what is on screen until the new result is ready,
    // so the text never flicks back to the built-in one.
    let tpl: BriefingResult | null = null;
    const first = shown !== `${kind}:${date}`;
    try {
      digest = templateDigest(date);
      if (first) {
        tpl = templateBriefing(kind!, date);
        await show(tpl, date, hasKey() ? `${name} is putting this into words…` : undefined);
      } else drawWeather();
    } catch {
      if (first) drawDay(null, null, true);
    }
    // Only the first text of the day is automatic (the morning one writes all four parts); later changes need a tap
    const b = await getBriefing(kind!, date).catch(() => null);
    if (my !== token) return;
    await show(b ?? tpl ?? templateBriefing(kind!, date), date);
    shown = `${kind}:${date}`;
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
    checkinHolder,
    dayHolder,
    weekCard(),
  );
  void load();
  return onContextUpdate(() => {
    // weather arrived after first paint: refresh the weather card; the stored AI text is reused, never rewritten
    void load();
  });
}
