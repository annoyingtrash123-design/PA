import { api, ApiError, getState, today } from "../core";
import { icon } from "../icons";
import { button, card, chip, clear, confirmDialog, field, h, muted, openModal, segmented, select, textInput, toast, toggle } from "../ui";
import { axisTicks } from "../util/chart";
import { fmtRepRange, fmtSetsReps, fmtWeight, parseNum } from "../util/text";
import type { Exercise, HistoryPoint, Rotation, Session, Template, TemplateExercise, TemplateKind, WorkoutPlan } from "../types";

type Sub = "today" | "rotation" | "templates" | "progress";
const SUBS: [Sub, string][] = [["today", "Today"], ["rotation", "Rotation"], ["templates", "Templates"], ["progress", "Progress"]];

let sub: Sub = "today";
let session: Session | null = null;
let plan: WorkoutPlan | null = null;
let progressEx = "";

export function renderGym(root: HTMLElement): void {
  const draw = () => {
    const y = window.scrollY;
    const seg = h("div", { class: "row", style: { justifyContent: "center" } }, segmented(SUBS, sub, (v) => { sub = v; draw(); window.scrollTo(0, 0); }));
    let body: (HTMLElement | null)[];
    try {
      body = { today: rToday, rotation: rRotation, templates: rTemplates, progress: rProgress }[sub](draw);
    } catch (e) {
      body = [card("stack", h("p", null, "Hmm, something went wrong."), muted((e as Error).message), button("Retry", draw, { icon: "restore" }))];
    }
    clear(root, seg, ...body);
    window.scrollTo(0, y);
  };
  draw();
}

const bigBtn = (label: string, ic: string, fn: () => void) => button(label, fn, { big: true, icon: ic, cls: "btn-wide" });

// ---------- today ----------
function rToday(draw: () => void): HTMLElement[] {
  if (session && !session.completed) return rWorkout(draw);
  plan = api<WorkoutPlan>("GET", "/api/gym/next");
  const skipBtn = () => button("Skip this one", async () => {
    if (!(await confirmDialog("Skip this one?", "The rotation moves on to the next item and nothing is logged.", "Skip", false))) return;
    try { api("POST", "/api/gym/skip"); draw(); } catch (e) { toast((e as Error).message); }
  }, { kind: "text", cls: "btn-wide" });
  if (plan.rest) {
    return [card("stack", h("div", { class: "row", style: { gap: "10px", color: "var(--sage)" } }, icon("leaf", 28), h("h3", { class: "title-md", style: { color: "var(--text)" } }, "Next up: Rest day")), muted("No lifting today. Stretch, walk, nap, whatever feels good."), button("Mark rest day", () => {
      try { api("POST", "/api/gym/rest"); toast("Rest day logged. Enjoy it!"); draw(); } catch (e) { toast((e as Error).message); }
    }, { icon: "check" }), button("Train today instead", () => {
      try { api("POST", "/api/gym/skip-rest"); draw(); } catch (e) { toast((e as Error).message); }
    }, { kind: "outline" }), h("p", { class: "muted small" }, "Missed a day? Days you don't train count as your rest day automatically.")), skipBtn()];
  }
  const p = plan;
  const cardio = p.kind === "cardio";
  const out: HTMLElement[] = [h("h2", { class: "title-md" }, `Next workout: ${p.name}`)];
  if (p.note) out.push(h("p", { class: "muted tpl-note" }, p.note));
  if (cardio) {
    out.push(card("compact stack",
      h("div", { class: "ex-head" }, h("h3", { class: "title-sm" }, "Pick one"), chip(`${p.minutes_min ?? 20}–${p.minutes_max ?? 40} min`)),
      h("div", { class: "chip-row" }, ...(p.options ?? []).map((o) => chip(o, { tone: "raised" })))));
  }
  for (const ex of p.exercises ?? []) {
    const lines: (HTMLElement | null)[] = [h("div", { class: "ex-head" }, h("h3", { class: "title-sm" }, ex.name), chip(fmtSetsReps(ex.sets, ex.rep_min, ex.rep_max)))];
    if (ex.note) lines.push(h("p", { class: "muted small ex-note" }, ex.note));
    if (ex.assisted) lines.push(h("p", { class: "muted small" }, "Counterweight machine: lower is stronger"));
    if (ex.last) lines.push(h("p", { class: "muted" }, `Last time (${ex.last.date}): ${ex.last.sets.map((s) => `${fmtWeight(s.weight)}kg x ${s.reps}`).join(", ")}`));
    else if (!ex.hint) lines.push(h("p", { class: "muted" }, "No history yet. Pick a weight that feels right."));
    if (ex.hint) lines.push(h("div", { class: "hint" }, icon("trend", 16), h("span", null, ex.hint)));
    out.push(card("compact stack", ...lines));
  }
  const resuming = getState().gym.sessions.some((s) => !s.completed && s.date === today());
  out.push(bigBtn(resuming ? "Continue workout" : "Start workout", "play", () => {
    try { session = api<Session>("POST", "/api/sessions", { template_id: p.template_id }); draw(); } catch (e) { toast((e as Error).message); }
  }));
  out.push(skipBtn());
  return out;
}

// ---------- cardio workout ----------
function rCardio(draw: () => void): HTMLElement[] {
  const s = session!;
  const tpl = getState().gym.templates.find((t) => t.id === s.template_id);
  const options = tpl?.options?.length ? tpl.options : plan?.options ?? [];
  const lo = tpl?.minutes_min ?? 20, hi = tpl?.minutes_max ?? 40;
  const c = s.cardio ?? { activity: "", minutes: 30 };
  const chips = h("div", { class: "chip-row" });
  const minutes = h("input", { class: "input", type: "text", inputmode: "numeric", value: String(c.minutes), "aria-label": "Minutes" });
  const save = (): boolean => {
    const m = Math.round(parseNum(minutes.value, c.minutes));
    c.minutes = m >= 1 ? m : c.minutes;
    minutes.value = String(c.minutes);
    try { session = api<Session>("PUT", `/api/sessions/${s.id}/cardio`, { activity: c.activity, minutes: c.minutes }); return true; } catch (e) { toast((e as Error).message); return false; }
  };
  minutes.onchange = () => { save(); };
  const drawChips = () => clear(chips, ...options.map((o) => h("button", { type: "button", class: `chip chip-${c.activity === o ? "solid" : "raised"} chip-btn`, "aria-pressed": String(c.activity === o), onclick: () => { c.activity = o; drawChips(); save(); } }, o)));
  drawChips();
  const out: HTMLElement[] = [h("div", { class: "row between" }, h("h2", { class: "title-md" }, s.name), chip("In progress", { icon: "timer" }))];
  if (tpl?.note) out.push(h("p", { class: "muted tpl-note" }, tpl.note));
  out.push(card("compact stack", h("h3", { class: "title-sm" }, "What did you do?"), chips, field(`Minutes (usually ${lo}–${hi})`, h("div", { class: "input-suffix" }, minutes, h("span", null, "min")))));
  out.push(bigBtn("Finish workout", "flag", () => {
    if (!c.activity) { toast("Pick what you did first"); return; }
    if (!save()) return;
    try {
      const done = api<Session>("POST", `/api/sessions/${s.id}/complete`);
      session = null;
      const m = openModal("Workout done!", h("p", null, `Lovely: ${done.cardio?.minutes ?? c.minutes} minutes of ${(done.cardio?.activity ?? c.activity).toLowerCase()} logged. Go get a snack and be proud of yourself!`), [button("Yay", () => m.close())], { onclose: draw });
    } catch (e) { toast((e as Error).message); }
  }));
  out.push(button("Back (keeps the workout)", () => { save(); session = null; draw(); }, { kind: "text", cls: "btn-wide" }));
  return out;
}

// ---------- workout mode ----------
function rWorkout(draw: () => void): HTMLElement[] {
  const s = session!;
  if (s.cardio) return rCardio(draw);
  const tplNow = getState().gym.templates.find((t) => t.id === s.template_id);
  const assisted = new Set(getState().gym.exercises.filter((x) => x.assisted).map((x) => x.id));
  const tplEx = new Map((tplNow?.exercises ?? []).map((e) => [e.exercise_id, e]));
  const names = new Map(getState().gym.exercises.map((x) => [x.id, x.name]));
  const planEx = new Map((plan?.exercises ?? []).map((e) => [e.exercise_id, e]));
  const order: number[] = [];
  const byEx = new Map<number, Session["sets"]>();
  for (const st of s.sets) {
    if (!byEx.has(st.exercise_id)) { order.push(st.exercise_id); byEx.set(st.exercise_id, []); }
    byEx.get(st.exercise_id)!.push(st);
  }
  const fields = new Map<string, { w: HTMLInputElement; r: HTMLInputElement; d: HTMLInputElement }>();
  const keyOf = (e: number, n: number) => `${e}:${n}`;
  const collect = () => s.sets.forEach((st) => {
    const f = fields.get(keyOf(st.exercise_id, st.set_no));
    if (!f) return;
    st.weight = parseNum(f.w.value, 0);
    st.reps = Math.round(parseNum(f.r.value, 0));
    st.done = f.d.checked;
  });
  const save = (): boolean => {
    collect();
    try { session = api<Session>("PUT", `/api/sessions/${s.id}/sets`, { sets: s.sets }); return true; } catch { toast("Couldn't save that set"); return false; }
  };

  const out: HTMLElement[] = [h("div", { class: "row between" }, h("h2", { class: "title-md" }, s.name), chip("In progress", { icon: "timer" }))];
  if (tplNow?.note) out.push(h("p", { class: "muted tpl-note" }, tplNow.note));
  for (const exid of order) {
    const pe = planEx.get(exid);
    const col: (HTMLElement | null)[] = [h("div", { class: "ex-head" }, h("h3", { class: "title-sm" }, names.get(exid) ?? pe?.name ?? `Exercise ${exid}`), pe ?? tplEx.get(exid) ? h("span", { class: "muted small" }, `${fmtRepRange((pe ?? tplEx.get(exid))!.rep_min, (pe ?? tplEx.get(exid))!.rep_max)} reps`) : null)];
    const exNote = pe?.note ?? tplEx.get(exid)?.note;
    if (exNote) col.push(h("p", { class: "muted small ex-note" }, exNote));
    if (assisted.has(exid)) col.push(h("p", { class: "muted small" }, "Counterweight: lower is stronger"));
    if (pe?.last) col.push(h("p", { class: "muted small" }, "Last: " + pe.last.sets.map((x) => `${fmtWeight(x.weight)}x${x.reps}`).join(", ")));
    if (pe?.hint) col.push(h("p", { class: "small", style: { color: "var(--sage)" } }, pe.hint));
    for (const st of byEx.get(exid)!) {
      const w = h("input", { class: "input", type: "text", inputmode: "decimal", value: fmtWeight(st.weight), "aria-label": `Set ${st.set_no} weight in kg`, onchange: save });
      const r = h("input", { class: "input", type: "text", inputmode: "numeric", value: String(st.reps ?? ""), "aria-label": `Set ${st.set_no} reps`, onchange: save });
      const d = h("input", { type: "checkbox", checked: st.done, "aria-label": `Set ${st.set_no} done`, style: { width: "26px", height: "26px", accentColor: "var(--sage)", justifySelf: "center" }, onchange: save });
      fields.set(keyOf(exid, st.set_no), { w, r, d });
      col.push(h("div", { class: "setrow" }, h("span", { class: "muted" }, `Set ${st.set_no}`), h("div", { class: "input-suffix" }, w, h("span", null, "kg")), h("div", { class: "input-suffix" }, r, h("span", null, "reps")), d));
    }
    out.push(card("compact stack", ...col));
  }
  out.push(bigBtn("Finish workout", "flag", () => {
    if (!save()) return;
    try {
      const done = api<Session>("POST", `/api/sessions/${s.id}/complete`);
      const n = done.sets.filter((x) => x.done).length;
      session = null;
      const m = openModal("Workout done!", h("p", null, `You smashed ${done.name || "your workout"}: ${n} sets logged. Go get a snack and be proud of yourself!`), [button("Yay", () => m.close())], { onclose: draw });
    } catch (e) { toast((e as Error).message); }
  }));
  out.push(button("Back (keeps the workout)", () => { save(); session = null; draw(); }, { kind: "text", cls: "btn-wide" }));
  return out;
}

// ---------- rotation ----------
function rRotation(draw: () => void): HTMLElement[] {
  const rot = api<Rotation>("GET", "/api/rotation");
  const tmpls = api<Template[]>("GET", "/api/templates");
  const names = new Map(tmpls.map((t) => [t.id, t.name]));
  const items = rot.items;
  const put = (next: Rotation["items"]) => { try { api("PUT", "/api/rotation", { items: next }); draw(); } catch (e) { toast((e as Error).message); } };
  const move = (i: number, d: number) => { const n = [...items]; [n[i], n[i + d]] = [n[i + d], n[i]]; put(n); };
  const rows = items.map((it, i) => {
    const isNext = i === (rot.next_index ?? 0);
    return h("div", { class: `cycle-row ${isNext ? "next" : ""}` },
      h("span", { class: "num-badge" }, String(i + 1)),
      h("div", { class: "grow" }, h("div", { style: { fontWeight: "500" } }, it.rest ? "Rest" : names.get(it.template_id!) ?? "?"), isNext ? h("div", { class: "muted small" }, "You are here: next up") : null),
      h("button", { type: "button", class: "icon-btn sm", "aria-label": "Move up", disabled: i === 0, onclick: () => move(i, -1) }, icon("up", 20)),
      h("button", { type: "button", class: "icon-btn sm", "aria-label": "Move down", disabled: i === items.length - 1, onclick: () => move(i, 1) }, icon("down", 20)),
      h("button", { type: "button", class: "icon-btn sm", "aria-label": "Remove from cycle", onclick: () => put(items.filter((_, k) => k !== i)) }, icon("delete", 20)));
  });
  const add = select([["", "Add to cycle..."], ...tmpls.map((t): [string, string] => [`t${t.id}`, t.name]), ["rest", "Rest day"]], "");
  return [
    h("h2", { class: "title-md" }, "Your cycle"), muted("Workouts follow this order and loop around."),
    card("stack", ...(rows.length ? rows : [muted("Empty. Add a workout below.")])),
    h("div", { class: "row" }, h("div", { class: "grow" }, add), button("Add", () => { if (add.value) put([...items, add.value === "rest" ? { rest: true } : { template_id: Number(add.value.slice(1)) }]); })),
  ];
}

// ---------- templates ----------
function rTemplates(draw: () => void): HTMLElement[] {
  const tmpls = api<Template[]>("GET", "/api/templates");
  const out: HTMLElement[] = [h("div", { class: "row between" }, h("h2", { class: "title-md" }, "Templates"), button("New", () => editTemplate(null, draw), { kind: "tonal", icon: "add" }))];
  for (const t of tmpls) {
    out.push(h("button", { type: "button", class: "card compact goal", onclick: () => editTemplate(t, draw) }, h("h3", { class: "title-sm" }, t.name), t.note ? h("p", { class: "small" }, t.note) : null, h("p", { class: "muted small" }, t.kind === "cardio" ? `Cardio, ${t.minutes_min ?? 20}–${t.minutes_max ?? 40} min: ${(t.options ?? []).join(", ")}` : t.exercises.map((x) => `${x.name} ${fmtSetsReps(x.sets, x.rep_min, x.rep_max)}`).join(", ") || "No exercises yet")));
  }
  if (!tmpls.length) out.push(muted("No templates yet."));
  return out;
}

function editTemplate(t: Template | null, onDone: () => void) {
  let exercises: Exercise[];
  try { exercises = api<Exercise[]>("GET", "/api/exercises"); } catch (e) { return toast((e as Error).message); }
  const rows: TemplateExercise[] = (t?.exercises ?? []).map((x) => ({ ...x, assisted: exercises.find((e) => e.id === x.exercise_id)?.assisted ?? false }));
  let kind: TemplateKind = t?.kind ?? "strength";
  const name = textInput(t?.name ?? "", { placeholder: "e.g. Push 1" });
  const note = textInput(t?.note ?? "", { placeholder: "e.g. Chest + Triceps" });
  const list = h("div", { class: "stack", style: { gap: "8px" } });
  const err = h("p", { class: "error-text", hidden: true });
  const num = (r: TemplateExercise, key: "sets" | "rep_min" | "rep_max", label: string) => {
    const input = h("input", { class: "input", type: "text", inputmode: "numeric", value: String(r[key]), "aria-label": label, onchange: () => { r[key] = Math.max(1, Math.round(parseNum(input.value, r[key]))); input.value = String(r[key]); } });
    return field(label, input);
  };
  const rebuild = () => {
    clear(list, ...rows.map((r, i) => {
      const sw = h("input", { class: "input", type: "text", inputmode: "decimal", value: fmtWeight(r.start_weight), placeholder: "none", "aria-label": "Starting weight in kg", onchange: () => { const v = sw.value.trim(); r.start_weight = v === "" ? null : Math.max(0, parseNum(v, 0)); sw.value = fmtWeight(r.start_weight); } });
      const exNote = h("input", { class: "input", type: "text", value: r.note ?? "", placeholder: "e.g. Or cable fly", "aria-label": "Exercise note", onchange: () => { r.note = exNote.value.trim(); } });
      const asst = toggle("Assisted (counterweight)", !!r.assisted, (v) => { r.assisted = v; });
      return h("div", { class: "tpl-row" },
        h("div", { class: "row", style: { gap: "0" } }, h("span", { class: "grow", style: { fontWeight: "500" } }, r.name ?? exercises.find((x) => x.id === r.exercise_id)?.name ?? "Exercise"),
          h("button", { type: "button", class: "icon-btn sm", "aria-label": "Move up", disabled: i === 0, onclick: () => { [rows[i - 1], rows[i]] = [rows[i], rows[i - 1]]; rebuild(); } }, icon("up", 18)),
          h("button", { type: "button", class: "icon-btn sm", "aria-label": "Move down", disabled: i === rows.length - 1, onclick: () => { [rows[i + 1], rows[i]] = [rows[i], rows[i + 1]]; rebuild(); } }, icon("down", 18)),
          h("button", { type: "button", class: "icon-btn sm", "aria-label": "Remove exercise", onclick: () => { rows.splice(i, 1); rebuild(); } }, icon("close", 18))),
        h("div", { class: "triple" }, num(r, "sets", "Sets"), num(r, "rep_min", "Min reps"), num(r, "rep_max", "Max reps")),
        h("div", { class: "pair" }, field("Start weight (kg)", sw), field("Note", exNote)),
        asst.el);
    }));
  };
  rebuild();
  const picker = select([["", "Add exercise..."], ...exercises.map((x): [string, string] => [String(x.id), x.name]), ["new", "+ New exercise..."]], "");
  const newName = textInput("", { placeholder: "New exercise name" });
  const newField = field("New exercise name", newName);
  newField.hidden = true;
  picker.onchange = () => { newField.hidden = picker.value !== "new"; };
  const addEx = () => {
    const v = picker.value;
    if (!v) return;
    let ex: Exercise | undefined;
    if (v === "new") {
      const nm = newName.value.trim();
      if (!nm) return;
      try { ex = api<Exercise>("POST", "/api/exercises", { name: nm }); } catch (e) { return toast((e as Error).message); }
      exercises.push(ex);
      picker.insertBefore(h("option", { value: String(ex.id) }, ex.name), picker.lastElementChild);
    } else ex = exercises.find((x) => String(x.id) === v);
    if (!ex) return;
    rows.push({ exercise_id: ex.id, name: ex.name, sets: 4, rep_min: 10, rep_max: 10, start_weight: null, note: "", assisted: !!ex.assisted });
    picker.value = ""; newName.value = ""; newField.hidden = true;
    rebuild();
  };
  const strengthBox = h("div", { class: "stack", style: { gap: "8px" } }, list, h("div", { class: "row" }, h("div", { class: "grow" }, picker), button("Add", addEx, { kind: "tonal" })), newField);
  const optionsIn = textInput((t?.options ?? []).join(", "), { placeholder: "Swimming, Rowing machine, ..." });
  const minLo = textInput(String(t?.minutes_min ?? 20), { inputmode: "numeric", "aria-label": "Minimum minutes" });
  const minHi = textInput(String(t?.minutes_max ?? 40), { inputmode: "numeric", "aria-label": "Maximum minutes" });
  const cardioBox = h("div", { class: "stack", style: { gap: "8px" } }, field("Activities to choose from", optionsIn, "Separate with commas"), h("div", { class: "pair" }, field("Min minutes", minLo), field("Max minutes", minHi)));
  const kindSeg = h("div", { class: "row", style: { justifyContent: "center" } });
  const showKind = () => {
    strengthBox.hidden = kind === "cardio";
    cardioBox.hidden = kind !== "cardio";
    clear(kindSeg, segmented<TemplateKind>([["strength", "Strength"], ["cardio", "Cardio"]], kind, (v) => { kind = v; showKind(); }));
  };
  showKind();
  const save = () => {
    const nm = name.value.trim();
    if (!nm) { err.textContent = "Give the template a name."; err.hidden = false; return; }
    const body = kind === "cardio"
      ? { name: nm, kind, note: note.value.trim(), options: optionsIn.value.split(",").map((x) => x.trim()).filter(Boolean), minutes_min: Math.round(parseNum(minLo.value, 20)), minutes_max: Math.round(parseNum(minHi.value, 40)) }
      : { name: nm, kind, note: note.value.trim(), exercises: rows.map((r) => ({ exercise_id: r.exercise_id, sets: r.sets, rep_min: r.rep_min, rep_max: r.rep_max, start_weight: r.start_weight ?? null, note: r.note ?? "", assisted: !!r.assisted })) };
    try { t ? api("PUT", `/api/templates/${t.id}`, body) : api("POST", "/api/templates", body); } catch (e) { err.textContent = e instanceof ApiError ? e.message : String(e); err.hidden = false; return; }
    m.close(); onDone();
  };
  const m = openModal(t ? "Edit template" : "New template", [field("Template name", name), field("Note (optional)", note), kindSeg, strengthBox, cardioBox, err], [
    t ? button("Delete", async () => {
      if (!(await confirmDialog("Delete this template?", `"${t.name}" will be removed from your templates and cycle.`, "Delete"))) return;
      try { api("DELETE", `/api/templates/${t.id}`); } catch (e) { return toast((e as Error).message); }
      m.close(); onDone();
    }, { kind: "text", cls: "danger spacer" }) : null,
    button("Cancel", () => m.close(), { kind: "text" }), button("Save", save)]);
}

// ---------- progress ----------
function rProgress(draw: () => void): HTMLElement[] {
  const exercises = api<Exercise[]>("GET", "/api/exercises");
  if (!progressEx && exercises.length) progressEx = String(exercises[0].id);
  const dd = select(exercises.map((x): [string, string] => [String(x.id), x.name]), progressEx || null);
  dd.onchange = () => { progressEx = dd.value; draw(); };
  const out: HTMLElement[] = [h("h2", { class: "title-md" }, "Progress"), field("Exercise", dd)];
  if (!progressEx) return [...out, muted("Add some exercises first.")];
  const hist = api<HistoryPoint[]>("GET", `/api/exercises/${progressEx}/history`);
  if (!hist.length) return [...out, card("", muted("No sessions logged for this one yet. Go lift something!"))];
  const assisted = !!exercises.find((x) => String(x.id) === progressEx)?.assisted;
  out.push(card("compact stack", muted(assisted ? "Counterweight (lower is stronger)" : "Top weight (kg)"), weightChart(hist, assisted)));
  out.push(card("compact stack", h("h3", { class: "title-sm" }, "Entries"), ...[...hist].reverse().map((p) => h("div", { class: "entry" }, h("span", { class: "grow" }, p.date), h("strong", { style: { fontWeight: "500" } }, `${fmtWeight(p.top_weight)} kg x ${p.best_reps ?? "-"}`), assisted || p.est_1rm == null ? null : h("span", { class: "muted small" }, `1RM ~${fmtWeight(p.est_1rm)}`)))));
  return out;
}

function weightChart(hist: HistoryPoint[], assisted = false): SVGElement {
  const W = 340, H = 200, L = 38, R = 12, T = 10, B = 26;
  const ws = hist.map((p) => p.top_weight);
  const { yMin, yMax, ticks } = axisTicks(Math.min(...ws), Math.max(...ws));
  const x = (i: number) => (hist.length === 1 ? (L + W - R) / 2 : L + (i * (W - L - R)) / (hist.length - 1));
  const y = (v: number) => T + (1 - (v - yMin) / (yMax - yMin)) * (H - T - B);
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  svg.setAttribute("class", "chart");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", `${assisted ? "Counterweight (lower is stronger)" : "Top weight"} per session, from ${fmtWeight(ws[0])} to ${fmtWeight(ws[ws.length - 1])} kilograms`);
  const line = hist.map((p, i) => `${i ? "L" : "M"}${x(i).toFixed(1)} ${y(p.top_weight).toFixed(1)}`).join(" ");
  const area = `${line} L${x(hist.length - 1).toFixed(1)} ${H - B} L${x(0).toFixed(1)} ${H - B} Z`;
  const step = Math.max(1, Math.ceil(hist.length / 4));
  svg.innerHTML =
    ticks.map((v) => `<line x1="${L}" x2="${W - R}" y1="${y(v)}" y2="${y(v)}" stroke="var(--line)"/><text x="${L - 6}" y="${y(v) + 3}" text-anchor="end">${fmtWeight(v)}</text>`).join("") +
    (hist.length > 1 ? `<path d="${area}" fill="var(--lav)" opacity="0.2"/>` : "") +
    `<path d="${line}" fill="none" stroke="var(--lav)" stroke-width="3.5" stroke-linecap="round" stroke-linejoin="round"/>` +
    hist.map((p, i) => `<circle cx="${x(i)}" cy="${y(p.top_weight)}" r="4.5" fill="var(--lav)" stroke="var(--card)" stroke-width="1.5"/>`).join("") +
    hist.map((p, i) => (i % step === 0 ? `<text x="${x(i)}" y="${H - 8}" text-anchor="middle">${p.date.slice(5)}</text>` : "")).join("");
  return svg;
}
