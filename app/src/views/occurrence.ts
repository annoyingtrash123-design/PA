import { api } from "../core";
import { icon } from "../icons";
import { chip, dot, h, holdButton, toast } from "../ui";
import { PART_LABELS } from "../util/nextup";
import type { Occurrence } from "../types";

const expanded = new Set<string>();
export const timeOrPart = (o: Pick<Occurrence, "time" | "part_of_day">) => (o.time ? o.time : PART_LABELS[o.part_of_day || "anytime"] ?? "Anytime");

/** One task occurrence: tick, colour dot, title, carried-over tag, expandable checklist. */
export function occurrenceTile(o: Occurrence, onChanged: () => void, onOpen: (o: Occurrence) => void): HTMLElement {
  const key = `${o.task_id}|${o.date}`;
  const guard = (fn: () => void) => {
    try {
      fn();
      onChanged();
    } catch (e) {
      toast((e as Error).message);
    }
  };
  const tick = h("button", { type: "button", class: `check ${o.done ? "done" : ""}`, role: "checkbox", "aria-checked": String(o.done), "aria-label": `${o.done ? "Mark not done" : "Mark done"}: ${o.title}`, onclick: () => guard(() => api("POST", "/api/occurrences/toggle", { task_id: o.task_id, date: o.date, done: !o.done })) }, o.done ? icon("check", 16) : null);
  const main = h(
    "button",
    { type: "button", class: "occ-main", onclick: () => onOpen(o) },
    h("div", { class: "occ-title" }, o.title),
    h("div", { class: "occ-sub" }, h("span", null, timeOrPart(o)), o.rolled_over_from ? chip("carried over", { icon: "redo", tone: "lav" }) : null, o.recurring ? icon("repeat", 13) : null),
  );
  const row = h("div", { class: "occ-row" }, tick, dot(o.category?.color, 10), main);
  // one-off, unfinished: press and hold to move it to the next day (repeating tasks come back on their own)
  if (!o.done && !o.recurring) {
    row.appendChild(
      holdButton("Move to tomorrow", () => {
        try {
          api("POST", "/api/occurrences/move", { task_id: o.task_id, date: o.date });
          toast(`Moved “${o.title}” to the next day`);
          onChanged();
        } catch (e) {
          toast((e as Error).message);
        }
      }, { icon: "redo", cls: "icon-only", hint: "Press and hold the arrow to move this to the next day" }),
    );
  }
  const wrap = h("div", { class: `occ ${o.done ? "done" : ""}` }, row);
  if (o.checklist.length) {
    const nDone = o.checklist.filter((c) => c.done).length;
    const list = h(
      "div",
      { class: "checklist", hidden: !expanded.has(key) },
      ...o.checklist.map((c) => {
        const cb = h("input", { type: "checkbox", checked: c.done, onchange: () => guard(() => api("POST", "/api/occurrences/checklist", { task_id: o.task_id, date: o.date, item_id: c.id, done: cb.checked })) });
        return h("label", { class: c.done ? "done" : "" }, cb, h("span", null, c.text));
      }),
    );
    const toggleBtn = h("button", { type: "button", class: "checklist-toggle", "aria-expanded": String(expanded.has(key)), "aria-label": "Show checklist" }, `${nDone}/${o.checklist.length}`, icon(expanded.has(key) ? "expand-less" : "expand-more", 20));
    toggleBtn.onclick = () => {
      const open = list.hidden;
      list.hidden = !open;
      open ? expanded.add(key) : expanded.delete(key);
      toggleBtn.setAttribute("aria-expanded", String(open));
      toggleBtn.replaceChildren(`${nDone}/${o.checklist.length}`, icon(open ? "expand-less" : "expand-more", 20));
    };
    row.appendChild(toggleBtn);
    wrap.appendChild(list);
  }
  return wrap;
}
