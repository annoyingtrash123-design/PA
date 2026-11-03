// Home: "Overview of the week" (formerly the Review tab). Collapsed by default so no AI text is fetched until it is opened.
import { api, getState, now, today } from "../core";
import { getBriefing, type BriefingResult } from "../services/ai-text";
import { stop as stopSpeech } from "../services/speech";
import { icon } from "../icons";
import { button, card, clear, h, iconBtn, readAloud, toast } from "../ui";
import { addDays, dayMonth } from "../util/dates";
import { paragraphs } from "../util/text";
import { defaultWeek, resolveOpen, weekOpenKey } from "../util/week";
import type { Day } from "../types";

const store = {
  get: (k: string): string | null => { try { return localStorage.getItem(k); } catch { return null; } },
  set: (k: string, v: string) => { try { localStorage.setItem(k, v); } catch { /* private mode */ } },
};

export function weekCard(): HTMLElement {
  const root = h("div");
  const dflt = defaultWeek(today(), now().now);
  const thisMonday = defaultWeek(today(), "23:59").week;
  let week = dflt.week;
  let glance = false;
  let token = 0;

  const isOpen = () => resolveOpen(store.get(weekOpenKey(week)), week === dflt.week ? dflt.open : false);
  const setOpen = (v: boolean) => { store.set(weekOpenKey(week), v ? "1" : "0"); stopSpeech(); draw(); };
  const go = (n: number) => { week = addDays(week, 7 * n); store.set(weekOpenKey(week), "1"); stopSpeech(); draw(); };

  const draw = () => {
    const my = ++token;
    const open = isOpen();
    const label = week === thisMonday ? "This week" : week === addDays(thisMonday, -7) ? "Last week" : week.slice(0, 4);
    if (!open) {
      clear(root, card("week-card", h("button", { type: "button", class: "week-head", "aria-expanded": "false", onclick: () => setOpen(true) },
        icon("insights", 20),
        h("span", { class: "grow stack", style: { gap: "2px" } }, h("span", { class: "title-sm" }, "Overview of the week"), h("span", { class: "muted small" }, "Tap to see how your week is going")),
        h("span", { style: { color: "var(--muted)", display: "flex" } }, icon("expand-more", 22)))));
      return;
    }
    const days = api<Day[]>("GET", `/api/days?start=${week}&days=7`);
    const anchor = week === thisMonday ? today() : week;
    const total = days.reduce((n, d) => n + d.items.length, 0);
    const done = days.reduce((n, d) => n + d.items.filter((o) => o.done).length, 0);
    const textHolder = h("div", { class: "summary" }, h("div", { class: "row", style: { gap: "12px" } }, h("span", { class: "spinner" }), h("span", { class: "muted" }, "Looking back over the week…")));
    const tiles = h("div", { class: "tiles" }, tile("Tasks done", `${done}/${total}`, "tasks"));
    const bars = card("", h("div", { class: "bars" }, ...days.map((d) => {
      const n = d.items.length, k = d.items.filter((o) => o.done).length;
      return h("div", { class: "bar-col" }, h("div", { class: "bar", role: "img", "aria-label": `${d.weekday}: ${k} of ${n} done` }, h("i", { style: { height: `${n ? (k / n) * 100 : 0}%` } })), h("span", null, d.weekday.slice(0, 3)), h("span", { style: { fontSize: "10px" } }, `${k}/${n}`));
    })));

    clear(root, card("stack week-card",
      h("button", { type: "button", class: "week-head", "aria-expanded": "true", onclick: () => setOpen(false) }, icon("insights", 20), h("span", { class: "grow title-sm" }, "Overview of the week"), h("span", { style: { color: "var(--muted)", display: "flex" } }, icon("expand-less", 22))),
      h("div", { class: "weekbar" }, iconBtn("left", "Previous week", () => go(-1)), h("div", { class: "mid" }, h("h3", { class: "title-sm", style: { fontSize: "16px" } }, `${dayMonth(week)} - ${dayMonth(addDays(week, 6))}`), h("p", { class: "muted small" }, label)), iconBtn("right", "Next week", () => go(1))),
      week === dflt.week ? null : h("div", { class: "row", style: { justifyContent: "center" } }, button(dflt.week === thisMonday ? "This week" : "Back to now", () => { week = dflt.week; store.set(weekOpenKey(week), "1"); stopSpeech(); draw(); }, { kind: "outline", icon: "event" })),
      textHolder,
      button(glance ? "Hide at a glance" : "At a glance", () => { glance = !glance; draw(); }, { kind: "text", cls: "small glance-toggle", icon: glance ? "expand-less" : "expand-more" }),
      glance ? tiles : null,
      glance ? bars : null,
    ));

    const show = (b: BriefingResult) => {
      const gym = b.prep.stats?.gym_sessions;
      if (typeof gym === "number" && !tiles.querySelector(".tile-gym")) tiles.appendChild(tile("Gym sessions", String(gym), "gym"));
      const text = b.text || "I couldn't put the week into words just now.";
      const ask = async () => {
        clear(textHolder, h("div", { class: "row", style: { gap: "12px" } }, h("span", { class: "spinner" }), h("span", { class: "muted" }, "Looking back over the week…")));
        const r = await getBriefing("weekly", anchor, { force: true }).catch(() => null);
        if (my !== token) return;
        if (r?.notice) toast(r.notice);
        show(r ?? b);
      };
      clear(
        textHolder,
        ...paragraphs(text).map((p) => h("p", null, p)),
        b.canWrite ? button("Write overview", () => void ask(), { kind: "tonal", icon: "insights" }) : null,
        b.outdated ? button("Plans changed — update summary", () => void ask(), { kind: "text", cls: "small" }) : null,
        readAloud(() => text, () => getState().settings.voice, "Read the week to me"),
      );
    };
    // only reached when the card is open: this is the one place the weekly text may be fetched or generated
    void getBriefing("weekly", anchor).then((b) => {
      if (my === token) show(b);
    }).catch(() => {
      if (my === token) clear(textHolder, h("p", { class: "muted" }, "I couldn't put the week into words just now."));
    });
  };
  draw();
  return root;
}

function tile(label: string, value: string, ic: string): HTMLElement {
  return h("div", { class: `tile tile-${ic}` }, icon(ic, 22), h("span", { class: "val" }, value), h("span", { class: "lbl" }, label));
}
