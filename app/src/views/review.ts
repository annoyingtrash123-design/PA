import { api, getState, today } from "../core";
import { getBriefing, type BriefingResult } from "../services/ai-text";
import { stop as stopSpeech } from "../services/speech";
import { icon } from "../icons";
import { button, card, clear, h, iconBtn, readAloud, toast } from "../ui";
import { addDays, dayMonth, mondayOf } from "../util/dates";
import { paragraphs } from "../util/text";
import type { Day } from "../types";

let week = "";

export function renderReview(root: HTMLElement): void {
  if (!week) week = mondayOf(today());
  let token = 0;

  const draw = () => {
    const my = ++token;
    const days = api<Day[]>("GET", `/api/days?start=${week}&days=7`);
    const thisMonday = mondayOf(today());
    const isThis = week === thisMonday;
    const anchor = isThis ? today() : week;
    const go = (n: number) => { week = addDays(week, 7 * n); stopSpeech(); draw(); window.scrollTo(0, 0); };

    const total = days.reduce((n, d) => n + d.items.length, 0);
    const done = days.reduce((n, d) => n + d.items.filter((o) => o.done).length, 0);

    const textHolder = h("div", { class: "summary" }, h("div", { class: "row", style: { gap: "12px" } }, h("span", { class: "spinner" }), h("span", { class: "muted" }, "Looking back over the week…")));
    const tiles = h("div", { class: "tiles" }, tile("Tasks done", `${done}/${total}`, "tasks"));
    const bars = card("", h("div", { class: "bars" }, ...days.map((d) => {
      const n = d.items.length, k = d.items.filter((o) => o.done).length;
      return h("div", { class: "bar-col" }, h("div", { class: "bar", role: "img", "aria-label": `${d.weekday}: ${k} of ${n} done` }, h("i", { style: { height: `${n ? (k / n) * 100 : 0}%` } })), h("span", null, d.weekday.slice(0, 3)), h("span", { style: { fontSize: "10px" } }, `${k}/${n}`));
    })));

    clear(
      root,
      h("div", { class: "weekbar" }, iconBtn("left", "Previous week", () => go(-1)), h("div", { class: "mid" }, h("h2", { class: "title-sm", style: { fontSize: "17px" } }, `${dayMonth(week)} - ${dayMonth(addDays(week, 6))}`), h("p", { class: "muted small" }, isThis ? "This week" : week.slice(0, 4))), iconBtn("right", "Next week", () => go(1))),
      isThis ? null : h("div", { class: "row", style: { justifyContent: "center" } }, button("This week", () => { week = thisMonday; draw(); }, { kind: "outline", icon: "event" })),
      card("stack", h("div", { class: "row", style: { color: "var(--lav)", gap: "8px" } }, icon("insights", 20), h("h2", { class: "title-sm", style: { color: "var(--text)" } }, "Overview of the week")), textHolder),
      h("p", { class: "muted small" }, "At a glance"),
      tiles,
      bars,
    );

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
        readAloud(() => text, () => getState().settings.voice),
      );
    };
    void getBriefing("weekly", anchor).then((b) => {
      if (my === token) show(b);
    }).catch(() => {
      if (my === token) clear(textHolder, h("p", { class: "muted" }, "I couldn't put the week into words just now."));
    });
  };
  draw();
}

function tile(label: string, value: string, ic: string): HTMLElement {
  return h("div", { class: `tile tile-${ic}` }, icon(ic, 22), h("span", { class: "val" }, value), h("span", { class: "lbl" }, label));
}
