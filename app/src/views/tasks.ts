import { api, today } from "../core";
import { onContextUpdate } from "../services/events";
import { icon } from "../icons";
import { button, card, chip, clear, h, iconBtn, toast } from "../ui";
import { addDays, dayMonth, longDay, mondayOf, WEEKDAY_SHORT, weekdayIndex } from "../util/dates";
import { occurrenceTile } from "./occurrence";
import { openTaskDialog } from "./task-dialog";
import type { Day, Occurrence } from "../types";

let week = "";

export function renderTasks(root: HTMLElement): () => void {
  if (!week) week = mondayOf(today());
  const thisWeek = () => mondayOf(today());

  const draw = () => {
    const y = window.scrollY;
    const days = api<Day[]>("GET", `/api/days?start=${week}&days=7`);
    const end = addDays(week, 6);
    const isThis = week === thisWeek();
    const missed = isThis ? api<Occurrence[]>("GET", "/api/missed") : [];
    const go = (n: number) => {
      week = addDays(week, 7 * n);
      draw();
      window.scrollTo(0, 0);
    };
    const out: (HTMLElement | null)[] = [
      h("div", { class: "row", style: { justifyContent: "flex-end" } }, button("New task", () => openTaskDialog({ date: today(), onSaved: draw }), { kind: "tonal", icon: "add" })),
      h("div", { class: "weekbar" }, iconBtn("left", "Previous week", () => go(-1)), h("div", { class: "mid" }, h("h2", { class: "title-sm", style: { fontSize: "17px" } }, `${dayMonth(week)} - ${dayMonth(end)}`), h("p", { class: "muted small" }, isThis ? "This week" : week.slice(0, 4))), iconBtn("right", "Next week", () => go(1))),
      isThis ? null : h("div", { class: "row", style: { justifyContent: "center" } }, button("This week", () => { week = thisWeek(); draw(); }, { kind: "outline", icon: "event" })),
      missed.length ? missedCard(missed, draw) : null,
      ...days.map((d) => dayCard(d, draw)),
    ];
    clear(root, ...out);
    window.scrollTo(0, y);
  };

  draw();
  return onContextUpdate(draw);
}

function dayCard(d: Day, redraw: () => void): HTMLElement {
  const isToday = d.date === today();
  const head = h("div", { class: "chips" }, h("span", { class: "day-title" }, longDay(d.date)));
  if (isToday) head.appendChild(chip("Today", { tone: "solid" }));
  if (d.workout) head.appendChild(chip(d.workout.rest ? "Rest" : d.workout.name ?? "Workout", { icon: d.workout.rest ? "leaf" : "gym", tone: d.workout.rest ? "sage" : "lav" }));
  if (d.holiday) head.appendChild(chip(d.holiday, { icon: "star", tone: "sage" }));
  if (d.day_off) head.appendChild(chip("Day off", { icon: "beach", tone: "sage" }));

  const menu = dayMenu(d, redraw);
  const top = h("div", { class: "day-head" }, head, menu, iconBtn("add-circle", "Add task", () => openTaskDialog({ date: d.date, onSaved: redraw }), "lav", 32));
  const body: HTMLElement[] = [top];
  if (!d.items.length && !d.work) body.push(h("p", { class: "muted" }, "Nothing planned"));
  // merge the work band into the timed items by start time
  const entries: { key: string; o: Day["items"][number] | null }[] = d.items.map((o) => ({ key: o.time ?? "99:99", o }));
  if (d.work) entries.push({ key: d.work.start, o: null });
  entries.sort((a, b) => (a.key === b.key ? (a.o ? 1 : 0) - (b.o ? 1 : 0) : a.key < b.key ? -1 : 1));
  for (const e of entries) {
    if (!e.o) body.push(h("div", { class: "work-band" }, icon("work", 18), `Work ${d.work!.start}-${d.work!.end}`));
    else body.push(occurrenceTile(e.o, redraw, (occ) => openTaskDialog({ date: occ.date, occ, onSaved: redraw })));
  }
  return card(`compact ${isToday ? "raised hl today" : ""}`, ...body);
}

function dayMenu(d: Day, redraw: () => void): HTMLElement {
  const wrap = h("div", { class: "menu" });
  const btn = iconBtn("more", "Day options", () => {
    if (wrap.querySelector(".menu-pop")) return close();
    const pop = h("div", { class: "menu-pop", role: "menu" }, h("button", { type: "button", role: "menuitem", onclick: () => {
      close();
      try {
        d.day_off ? api("DELETE", `/api/days-off/${d.date}`) : api("POST", "/api/days-off", { date: d.date });
        redraw();
      } catch (e) {
        toast((e as Error).message);
      }
    } }, icon(d.day_off ? "event" : "beach", 20), d.day_off ? "Undo day off" : "Mark as day off"));
    wrap.appendChild(pop);
    setTimeout(() => document.addEventListener("click", close, { once: true }), 0);
  }, "muted");
  const close = () => wrap.querySelector(".menu-pop")?.remove();
  wrap.appendChild(btn);
  return wrap;
}

/** Timed tasks / appointments whose day passed: never moved silently, the user decides. */
function missedCard(items: Occurrence[], redraw: () => void): HTMLElement {
  const act = (fn: () => void) => {
    try {
      fn();
      redraw();
    } catch (e) {
      toast((e as Error).message);
    }
  };
  const rows = items.map((o) =>
    h(
      "div",
      { class: "missed-item" },
      h("div", { class: "occ-title" }, o.title),
      h("div", { class: "occ-sub" }, `was ${WEEKDAY_SHORT[weekdayIndex(o.date)]}${o.time ? ` ${o.time}` : ""}`),
      h(
        "div",
        { class: "chip-row", style: { marginTop: "6px" } },
        button("Move to today", () => act(() => api("POST", `/api/missed/${o.task_id}/today`)), { kind: "tonal", icon: "redo" }),
        button("Reschedule", () => openTaskDialog({ date: o.date, occ: o, onSaved: redraw }), { kind: "outline", icon: "event" }),
        button("Dismiss", () => act(() => api("POST", `/api/missed/${o.task_id}/dismiss`)), { kind: "text" }),
      ),
    ),
  );
  return card("compact missed", h("div", { class: "day-title" }, "Missed"), ...rows);
}
