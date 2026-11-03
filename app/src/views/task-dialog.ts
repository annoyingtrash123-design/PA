// Add / edit task dialog.
import { api, ApiError, getState } from "../core";
import { icon } from "../icons";
import { refineCategory } from "../services/ai-text";
import { hasKey } from "../services/cache";
import { button, confirmDialog, field, h, openModal, select, textInput, toast, toggle } from "../ui";
import { derivePart, TIME_RE, WEEKDAY_SHORT } from "../util/dates";
import { PART_LABELS } from "../util/nextup";
import type { Occurrence, Task } from "../types";

const REPEAT: [string, string][] = [["none", "Doesn't repeat"], ["daily", "Every day"], ["weekdays", "Weekdays"], ["weekly", "Weekly on chosen days"]];

export function openTaskDialog(opts: { date: string; occ?: Occurrence; onSaved: () => void }) {
  const { occ } = opts;
  const state = getState();
  const task: Task | undefined = occ ? state.tasks.find((t) => t.id === occ.task_id) : undefined;
  const t: Partial<Task> = task ?? { title: "", time: null, part_of_day: "anytime", category_id: null, notes: "", recurrence: null, checklist: [] };
  const rec = t.recurrence;

  const title = textInput(t.title ?? "", { placeholder: "e.g. Call the dentist", autocapitalize: "sentences" });
  const time = h("input", { class: "input", type: "time", value: t.time ?? "09:00" });
  const useTime = toggle("Set a time", !!t.time, (on) => {
    timeField.hidden = !on;
    partField.hidden = on;
  });
  const timeField = field("Time", time);
  timeField.hidden = !t.time;
  const part = select(Object.entries(PART_LABELS) as [string, string][], t.part_of_day || "anytime");
  const partField = field("Part of day", part);
  partField.hidden = !!t.time;
  const catValue = !occ || t.category_auto ? "auto" : t.category_id ? String(t.category_id) : "none";
  const category = select([["auto", "Auto (let the assistant pick)"], ["none", "No category"], ...state.categories.map((c): [string, string] => [String(c.id), c.name])], catValue);
  const notes = h("textarea", { class: "input", rows: 2, placeholder: "Anything to remember" }, t.notes ?? "") as HTMLTextAreaElement;
  const repeat = select(REPEAT, rec ? rec.kind : "none");
  const days = new Set<number>(rec && rec.kind === "weekly" ? rec.days : []);
  const dayRow = h("div", { class: "daychips", hidden: !(rec && rec.kind === "weekly") });
  WEEKDAY_SHORT.forEach((n, i) => {
    const b = h("button", { type: "button", class: `daychip ${days.has(i) ? "on" : ""}`, "aria-pressed": String(days.has(i)) }, n);
    b.onclick = () => {
      days.has(i) ? days.delete(i) : days.add(i);
      b.classList.toggle("on", days.has(i));
      b.setAttribute("aria-pressed", String(days.has(i)));
    };
    dayRow.appendChild(b);
  });
  repeat.onchange = () => (dayRow.hidden = repeat.value !== "weekly");

  const items: { id?: number; input: HTMLInputElement }[] = [];
  const itemsCol = h("div", { class: "stack", style: { gap: "6px" } });
  const addItem = (text = "", id?: number) => {
    const input = textInput(text, { placeholder: "Step" });
    const it = { id, input };
    items.push(it);
    const row = h("div", { class: "row" }, h("div", { class: "grow" }, input), h("button", { type: "button", class: "icon-btn muted sm", "aria-label": "Remove step", onclick: () => { items.splice(items.indexOf(it), 1); row.remove(); } }, icon("close", 18)));
    itemsCol.appendChild(row);
  };
  (t.checklist ?? []).forEach((c) => addItem(c.text, c.id));

  const err = h("p", { class: "error-text", hidden: true });
  const fail = (m: string) => {
    err.textContent = m;
    err.hidden = false;
  };

  const save = () => {
    const ttl = title.value.trim();
    if (!ttl) return fail("Give it a title first.");
    let tm: string | null = null;
    if (useTime.input.checked) {
      const m = TIME_RE.exec(time.value.trim());
      if (!m) return fail("Time should look like 07:30.");
      tm = `${m[1].padStart(2, "0")}:${m[2]}`;
    }
    const kind = repeat.value;
    let recurrence: Task["recurrence"] = null;
    if (kind === "daily" || kind === "weekdays") recurrence = { kind };
    else if (kind === "weekly") {
      if (!days.size) return fail("Pick at least one day.");
      recurrence = { kind: "weekly", days: [...days].sort() };
    }
    const date = recurrence ? null : task?.date || opts.date;
    const body = {
      title: ttl, date, time: tm,
      part_of_day: tm ? derivePart(tm) : part.value || "anytime",
      category_id: category.value === "none" ? null : category.value === "auto" ? "auto" : Number(category.value),
      notes: notes.value, recurrence,
      checklist: items.filter((i) => i.input.value.trim()).map((i) => (i.id ? { id: i.id, text: i.input.value.trim() } : { text: i.input.value.trim() })),
    };
    let saved: Task;
    try {
      saved = occ ? api<Task>("PUT", `/api/tasks/${occ.task_id}`, body) : api<Task>("POST", "/api/tasks", body);
    } catch (e) {
      return fail(e instanceof ApiError ? e.message : String(e));
    }
    m.close();
    let msg = "Saved";
    if (body.category_id === "auto") {
      const name = getState().categories.find((c) => c.id === saved.category_id)?.name;
      if (name) msg = `Filed under ${name}`;
      if (hasKey())
        void refineCategory(saved.id).then((better) => {
          if (better && better !== name) {
            toast(`Filed under ${better}`);
            opts.onSaved();
          }
        });
    }
    toast(msg);
    opts.onSaved();
  };

  const del = async () => {
    if (!occ) return;
    const series = occ.recurring ? " This removes the whole repeating series." : "";
    if (!(await confirmDialog("Delete this task?", `"${occ.title}" will be removed.${series}`, "Delete"))) return;
    try {
      api("DELETE", `/api/tasks/${occ.task_id}`);
    } catch (e) {
      return toast((e as Error).message);
    }
    m.close();
    toast("Task deleted");
    opts.onSaved();
  };

  const body = [
    occ?.recurring ? h("div", { class: "note-box" }, "This is a repeating task: changes apply to the whole series.") : null,
    field("What needs doing?", title),
    useTime.el, timeField, partField,
    field("Category", category),
    field("Notes", notes),
    field("Repeat", repeat), dayRow,
    h("div", { class: "stack", style: { gap: "8px" } }, h("span", { class: "field-label" }, "Checklist (optional)"), itemsCol, button("Add checklist item", () => addItem(), { kind: "text", icon: "add"})),
    err,
  ];
  const actions = [
    occ ? button("Delete", () => void del(), { kind: "text", icon: "delete", cls: "danger spacer" }) : null,
    button("Cancel", () => m.close(), { kind: "text" }),
    button("Save", save),
  ];
  const m = openModal(occ ? "Edit task" : "New task", body, actions);
  if (!occ) title.focus();
}
