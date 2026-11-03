import { api, ApiError, today } from "../core";
import { icon } from "../icons";
import { button, card, chip, clear, confirmDialog, field, h, iconBtn, muted, openModal, segmented, textInput, toast, toggle } from "../ui";
import { prettyDate, relativeDate } from "../util/dates";
import type { Goal } from "../types";
import { leisureSection } from "./leisure";

const HORIZONS: ["medium" | "long", string][] = [["medium", "Medium-term"], ["long", "Long-term"]];
const open: Record<string, boolean> = { achieved: false, archived: false };

export const deadlineText = (g: Pick<Goal, "deadline">, todayIso: string) => (g.deadline ? `by ${prettyDate(g.deadline)} · ${relativeDate(g.deadline, todayIso)}` : "No deadline");

export function renderGoals(root: HTMLElement): void {
  const draw = () => {
    const y = window.scrollY;
    const goals = api<Goal[]>("GET", "/api/goals");
    const by = (st: Goal["status"], hz?: string) => goals.filter((g) => g.status === st && (!hz || g.horizon === hz));
    const out: (HTMLElement | null)[] = [
      h("div", { class: "row", style: { alignItems: "flex-start" } }, h("div", { class: "grow stack", style: { gap: "2px" } }, h("h2", { class: "title-md" }, "Plans"), muted("Goals your assistant uses to see how your days add up, and things for your free time.")), iconBtn("add-circle", "Add goal", () => editGoal(null, draw), "lav", 34)),
    ];
    if (!by("active").length) out.push(card("stack", muted("No active goals yet."), button("Add your first goal", () => editGoal(null, draw), { kind: "text", icon: "flag" })));
    for (const [hz, label] of HORIZONS) {
      const items = by("active", hz);
      if (!items.length) continue;
      out.push(h("p", { class: "sect-label" }, label), ...items.map((g) => goalCard(g, false, draw)));
    }
    for (const [key, label, ic] of [["achieved", "Achieved", "tasks"], ["archived", "Archived", "archive"]] as const) {
      const items = by(key);
      if (!items.length) continue;
      const isOpen = open[key];
      out.push(h("button", { type: "button", class: "collapse-head", "aria-expanded": String(isOpen), onclick: () => { open[key] = !isOpen; draw(); } }, h("span", { style: { color: key === "achieved" ? "var(--sage)" : "var(--muted)", display: "flex" } }, icon(ic, 18)), h("span", { class: "grow", style: { textAlign: "left" } }, `${label} (${items.length})`), h("span", { style: { color: "var(--muted)", display: "flex" } }, icon(isOpen ? "expand-less" : "expand-more", 22))));
      if (isOpen) out.push(...items.map((g) => goalCard(g, true, draw)));
    }
    out.push(leisureSection());
    clear(root, ...out);
    window.scrollTo(0, y);
  };
  draw();
}

function goalCard(g: Goal, dim: boolean, redraw: () => void): HTMLElement {
  const parts: (HTMLElement | null)[] = [h("h3", { class: "title-sm" }, g.title)];
  if (g.status === "achieved") parts.push(h("div", { class: "row", style: { color: "var(--sage)", fontSize: "13px" } }, icon("tasks", 16), g.achieved_on ? `Achieved ${prettyDate(g.achieved_on)}` : "Achieved"));
  else parts.push(h("div", { class: "row wrap", style: { gap: "6px" } }, chip(deadlineText(g, today()), { icon: "event" }), dim ? chip(g.horizon === "long" ? "Long-term" : "Medium-term", { tone: "raised" }) : null));
  if (g.why) parts.push(h("p", { class: "muted", style: { fontSize: "13.5px" } }, g.why));
  const c = h("button", { type: "button", class: "card compact goal", onclick: () => editGoal(g, redraw) }, ...parts);
  return c;
}

function editGoal(goal: Goal | null, onSaved: () => void) {
  const title = textInput(goal?.title ?? "", { placeholder: "e.g. Pass my driving test", autocapitalize: "sentences" });
  let horizon: "medium" | "long" = goal?.horizon ?? "medium";
  const segHolder = h("div", { class: "row", style: { justifyContent: "flex-start" } });
  const drawSeg = () => clear(segHolder, segmented(HORIZONS, horizon, (v) => { horizon = v; drawSeg(); }));
  drawSeg();
  const date = h("input", { class: "input", type: "date", value: goal?.deadline ?? "" });
  const dateField = field("Deadline", date);
  dateField.hidden = !goal?.deadline;
  const use = toggle("Set a deadline", !!goal?.deadline, (on) => { dateField.hidden = !on; });
  const why = h("textarea", { class: "input", rows: 3, placeholder: "Why does this matter to you?" }, goal?.why ?? "") as HTMLTextAreaElement;
  const err = h("p", { class: "error-text", hidden: true });
  const fail = (m: string) => { err.textContent = m; err.hidden = false; };

  const send = (status?: Goal["status"], msg = "Goal saved") => {
    const t = title.value.trim();
    if (!t) return fail("Give your goal a title first.");
    if (use.input.checked && !date.value) return fail("Pick a deadline date, or switch it off.");
    const st = status ?? goal?.status ?? "active";
    const body = {
      title: t, horizon, deadline: use.input.checked ? date.value : null, why: why.value.trim(), status: st,
      achieved_on: st === "achieved" ? (goal?.status === "achieved" ? goal.achieved_on : today()) : null,
    };
    try {
      goal ? api("PUT", `/api/goals/${goal.id}`, body) : api("POST", "/api/goals", body);
    } catch (e) {
      return fail(e instanceof ApiError ? e.message : String(e));
    }
    m.close();
    toast(msg);
    onSaved();
  };

  const extra: HTMLElement[] = [];
  if (goal) {
    if (goal.status === "active") {
      extra.push(button("Mark achieved", () => send("achieved", "Goal achieved, well done!"), { kind: "sage", icon: "check" }), button("Archive", () => send("archived", "Goal archived"), { kind: "outline", icon: "archive" }));
    } else extra.push(button("Back to active", () => send("active", "Goal is active again"), { kind: "outline", icon: "undo" }));
    extra.push(button("Delete", async () => {
      if (!(await confirmDialog("Delete this goal?", "This can't be undone. Archive it instead if you just want it out of the way.", "Delete"))) return;
      try { api("DELETE", `/api/goals/${goal.id}`); } catch (e) { return fail((e as Error).message); }
      m.close();
      toast("Goal deleted");
      onSaved();
    }, { kind: "text", icon: "delete", cls: "danger" }));
  }
  const body = [field("Goal", title), h("div", { class: "stack", style: { gap: "6px" } }, h("span", { class: "field-label" }, "Time frame"), segHolder), use.el, dateField, field("Why", why), err, extra.length ? h("div", { class: "row wrap", style: { gap: "8px" } }, ...extra) : null];
  const m = openModal(goal ? "Edit goal" : "New goal", body, [button("Cancel", () => m.close(), { kind: "text" }), button("Save", () => send())]);
  if (!goal) title.focus();
}
