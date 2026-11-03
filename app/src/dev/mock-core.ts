// Development stand-in for the Rust/wasm core (VITE_MOCK_CORE=1). Same `call` contract as docs/WEB_ARCH.md,
// in plain TypeScript and deliberately simple. It is aliased in place of src/core-wasm/pa_wasm.js.
/* eslint-disable @typescript-eslint/no-explicit-any */
import { addDays, weekdayIndex, longDay } from "../util/dates";

type Any = any;
class HttpError extends Error {
  constructor(public status: number, msg: string) {
    super(msg);
  }
}
const bad = (m: string) => new HttpError(400, m);
const nf = (m = "not found") => new HttpError(404, m);

export default async function init(): Promise<void> {}

const CATS = [
  ["Chores", "#A1887F"], ["Exercise", "#6BAA75"], ["Study", "#5C6BC0"], ["Work", "#4FA3D1"], ["Errands", "#E8B931"],
  ["Appointments", "#EC6A5C"], ["Social", "#D5679B"], ["Self-care", "#26A69A"], ["Admin & Money", "#9575CD"], ["Other", "#78909C"],
];
const EX = ["Bench Press", "Overhead Press", "Incline Dumbbell Press", "Triceps Pushdown", "Barbell Row", "Lat Pulldown", "Biceps Curl", "Squat", "Romanian Deadlift", "Leg Press"];
const TPL: [string, number[]][] = [
  ["Push A", [1, 2, 3, 4]], ["Push B", [3, 2, 1, 4]], ["Pull A", [5, 6, 7]], ["Pull B", [6, 5, 7]], ["Legs A", [8, 9, 10]], ["Legs B", [10, 8, 9]],
];

export function init_state(today: string): string {
  const s: Any = {
    version: 8, next_id: 100,
    settings: { assistant_name: "Miranda", user_name: "", voice: "", location_name: "Sydney", latitude: -33.8688, longitude: 151.2093, timezone: "Australia/Sydney" },
    categories: CATS.map(([name, color], i) => ({ id: i + 1, name, color })),
    tasks: [], completions: {}, checklist_done: {}, rolled_over: {}, missed: {}, days_off: [], goals: [], leisure: [],
    memory: {
      weather: { comfortable_max: 18, hot_above: 22, too_hot_above: 25, likes_rain: true, avoid_sunburn: true },
      work: { enabled: true, days: [0, 1, 2, 3, 4], start: "10:00", end: "17:00", holiday_region: "AU-NSW", lunch_minutes: 60, lunch_window_start: "12:00", lunch_window_end: "14:00" },
      rhythm: { wake_from: "06:00", wake_to: "07:00", sleep_from: "23:00", sleep_to: "00:00" },
      notes: [], profile_updated: "", interests: "History and politics, especially Western European and East Asian history. Particularly the 18th and 19th centuries, and keen to learn more about the 13th to 17th centuries.",
    },
    gym: {
      exercises: EX.map((name, i) => ({ id: i + 1, name, assisted: false })),
      templates: [...TPL.map(([name, ids], i) => ({
        id: i + 1, name, kind: "strength", note: "", options: [], minutes_min: null, minutes_max: null,
        exercises: ids.map((id) => ({ exercise_id: id, name: EX[id - 1], sets: 4, rep_min: 10, rep_max: 10, start_weight: null, note: "" })),
      })), { id: 7, name: "Cardio", kind: "cardio", note: "Do one of the following for 20–40 mins", exercises: [], options: ["Swimming", "Rowing machine", "Stair machine", "Intense walking/hiking"], minutes_min: 20, minutes_max: 40 }],
      rotation: { items: [{ template_id: 1 }, { template_id: 3 }, { template_id: 5 }, { template_id: 7 }, { template_id: 2 }, { template_id: 4 }, { template_id: 6 }, { template_id: 7 }], next_index: 0 },
      sessions: [],
    },
  };
  void today;
  return JSON.stringify(s);
}

export function has_stats(text: string): boolean {
  return /\b\d+\s*(of|out of|\/)\s*\d+\b/i.test(text) || /\d\s*%|\bpercent\b/i.test(text);
}

const WMO: Record<number, [string, string]> = {
  0: ["Clear sky", "sun"], 1: ["Mainly clear", "sun"], 2: ["Partly cloudy", "cloud"], 3: ["Overcast", "cloud"], 45: ["Fog", "cloud"],
  51: ["Light drizzle", "rain"], 61: ["Light rain", "rain"], 63: ["Rain", "rain"], 65: ["Heavy rain", "rain"], 71: ["Light snow", "snow"],
  80: ["Rain showers", "rain"], 95: ["Thunderstorm", "storm"],
};

const PARTS = ["morning", "afternoon", "evening", "anytime"];

function sampleData(s: Any, today: string) {
  const id = () => s.next_id++;
  const mk = (t: Any) => { const task = { id: id(), date: null, time: null, part_of_day: "anytime", category_id: null, notes: "", recurrence: null, checklist: [], ...t }; s.tasks.push(task); return task; };
  const cat = (n: string) => s.categories.find((c: Any) => c.name === n)?.id ?? null;
  mk({ title: "Morning routine", time: "07:30", part_of_day: "morning", category_id: cat("Self-care"), recurrence: { kind: "daily" },
    checklist: ["Make bed", "Stretch", "Breakfast"].map((text) => ({ id: id(), text })) });
  mk({ title: "Team stand-up", time: "10:00", part_of_day: "morning", category_id: cat("Work"), recurrence: { kind: "weekdays" } });
  mk({ title: "Buy groceries", date: today, time: "17:30", part_of_day: "afternoon", category_id: cat("Errands") });
  mk({ title: "Call mum", date: today, part_of_day: "evening", category_id: cat("Social") });
  const plants = mk({ title: "Water the plants", date: addDays(today, -1), category_id: cat("Chores") });
  s.rolled_over[plants.id] = plants.date;
  mk({ title: "Book dentist", date: addDays(today, 2), part_of_day: "morning", category_id: cat("Appointments"), notes: "Check-up and clean" });
  mk({ title: "Pay electricity bill", date: addDays(today, 3), category_id: cat("Admin & Money") });
  mk({ title: "Practise driving theory", recurrence: { kind: "weekly", days: [1, 3, 5] }, part_of_day: "evening", category_id: cat("Study") });
  s.goals.push(
    { id: id(), title: "Pass my driving test", horizon: "medium", deadline: addDays(today, 120), why: "Freedom to visit family without relying on lifts", status: "active", achieved_on: null, created: today },
    { id: id(), title: "Run a half marathon", horizon: "long", deadline: null, why: "", status: "active", achieved_on: null, created: today },
    { id: id(), title: "Learn to cook five dinners", horizon: "medium", deadline: addDays(today, -20), why: "Eat better and save money", status: "achieved", achieved_on: addDays(today, -20), created: addDays(today, -90) },
    { id: id(), title: "Tidy the garage", horizon: "medium", deadline: null, why: "", status: "archived", achieved_on: null, created: addDays(today, -60) },
  );
  s.leisure.push(
    { id: id(), kind: "read", title: "Piranesi", note: "On chapter twelve", status: "current", created: today, finished_on: null, last_suggested: null, blurb: "It's a quiet, dreamlike novel about a man who lives alone in an endless house of statues and tides." },
    { id: id(), kind: "watch", title: "Slow Horses", note: "", status: "someday", created: today, finished_on: null, last_suggested: null, blurb: "It's a sharp, funny spy series about a team of disgraced agents shunted off to a shabby office." },
    { id: id(), kind: "try", title: "The new cafe on Crown Street", note: "Someone said the flat white is great", status: "someday", created: today, finished_on: null, last_suggested: null, blurb: "It's a small neighbourhood cafe that's meant to be good for a slow coffee and a pastry." },
    { id: id(), kind: "play", title: "Stardew Valley", note: "", status: "someday", created: today, finished_on: null, last_suggested: null, blurb: "It's a cosy farming game where you take over a run-down farm and slowly build a life in a village." },
    { id: id(), kind: "watch", title: "Wolf Hall", note: "", status: "finished", created: addDays(today, -30), finished_on: addDays(today, -4), last_suggested: null, blurb: "" },
  );
  const sets = (w: number, reps: number) => [1, 2, 3].map((n) => ({ exercise_id: 1, set_no: n, weight: w, reps, done: true }));
  s.gym.sessions.push({ id: id(), template_id: 1, name: "Push A", date: addDays(today, -14), completed: true, sets: sets(55, 8) });
  s.gym.sessions.push({ id: id(), template_id: 1, name: "Push A", date: addDays(today, -7), completed: true, sets: sets(60, 10) });
  s.gym.sessions.push({ id: id(), template_id: 1, name: "Push A", date: addDays(today, -1), completed: true, sets: sets(62.5, 8) });
}

function rollMock(s: Any, today: string) {
  s.missed ??= {};
  for (const t of s.tasks) {
    if (t.recurrence || !t.date || t.date >= today || s.completions[`${t.id}|`]) continue;
    const appt = s.categories.find((c: Any) => c.id === t.category_id)?.name === "Appointments";
    if (t.time || appt) s.missed[t.id] = t.date;
    else { delete s.missed[t.id]; s.rolled_over[t.id] = s.rolled_over[t.id] || t.date; t.date = today; }
  }
}
function missedMock(s: Any): Any[] {
  return Object.entries(s.missed as Record<string, string>).map(([id, d]) => occurrencesFor(s, d, d).find((o) => o.task_id === +id)).filter((o) => o && !o.done) as Any[];
}
function occurrencesFor(s: Any, date: string, today: string): Any[] {
  const wd = weekdayIndex(date);
  const out: Any[] = [];
  for (const t of s.tasks) {
    const r = t.recurrence;
    let on = false;
    if (r) on = r.kind === "daily" || (r.kind === "weekdays" && wd < 5) || (r.kind === "weekly" && r.days.includes(wd));
    else on = t.date === date;
    if (!on) continue;
    const done = !!s.completions[`${t.id}|${r ? date : ""}`];
    out.push({
      task_id: t.id, date, title: t.title, time: t.time, part_of_day: t.time ? (+t.time.slice(0, 2) < 12 ? "morning" : +t.time.slice(0, 2) < 17 ? "afternoon" : "evening") : t.part_of_day || "anytime",
      category: s.categories.find((c: Any) => c.id === t.category_id) ?? null, notes: t.notes, recurring: !!r,
      rolled_over_from: !r && s.rolled_over[t.id] && s.rolled_over[t.id] !== date ? s.rolled_over[t.id] : null, done,
      checklist: t.checklist.map((c: Any) => ({ id: c.id, text: c.text, done: !!s.checklist_done[`${t.id}|${date}|${c.id}`] })),
    });
  }
  out.sort((a, b) => (a.time ? 0 : 1) - (b.time ? 0 : 1) || (a.time ?? "").localeCompare(b.time ?? "") || PARTS.indexOf(a.part_of_day) - PARTS.indexOf(b.part_of_day));
  void today;
  return out;
}

function lastFor(s: Any, exId: number) {
  const ss = s.gym.sessions.filter((x: Any) => x.completed && x.sets.some((q: Any) => q.exercise_id === exId && q.done)).sort((a: Any, b: Any) => a.date.localeCompare(b.date));
  const last = ss[ss.length - 1];
  if (!last) return null;
  return { date: last.date, sets: last.sets.filter((q: Any) => q.exercise_id === exId && q.done).map((q: Any) => ({ weight: q.weight, reps: q.reps })) };
}

function planFor(s: Any, item: Any): Any {
  if (!item || item.rest) return { rest: true };
  const t = s.gym.templates.find((x: Any) => x.id === item.template_id);
  if (!t) return { rest: true };
  return {
    rest: false, template_id: t.id, name: t.name, kind: t.kind ?? "strength", note: t.note ?? "", options: t.options ?? [], minutes_min: t.minutes_min ?? null, minutes_max: t.minutes_max ?? null,
    exercises: t.exercises.map((e: Any) => {
      const last = lastFor(s, e.exercise_id);
      let sug: number | null = null, hint: string | null = null;
      if (last) {
        const w = last.sets[0]?.weight ?? 0;
        const all = last.sets.length >= e.sets && last.sets.every((q: Any) => q.reps >= e.rep_max && q.weight === w);
        sug = all ? w + 2.5 : w;
        hint = all ? `You hit ${e.rep_max} reps on every set last time, try ${sug} kg` : null;
      }
      if (!last && e.start_weight != null) { sug = e.start_weight; hint = `Starting weight: ${sug} kg`; }
      return { exercise_id: e.exercise_id, name: e.name, sets: e.sets, rep_min: e.rep_min, rep_max: e.rep_max, last, suggested_weight: sug, hint, assisted: false, start_weight: e.start_weight ?? null, note: e.note ?? "" };
    }),
  };
}

function weatherText(w: Any) {
  return w ? `${w.summary} with a high of ${w.temp_max}` : "";
}

function dayDetail(s: Any, ctx: Any, date: string): Any {
  const items = occurrencesFor(s, date, ctx.today);
  const rot = s.gym.rotation;
  let workout = null;
  if (date >= ctx.today && rot.items.length) {
    const n = Math.round((Date.parse(date) - Date.parse(ctx.today)) / 86400000);
    const doneToday = s.gym.sessions.find((x: Any) => x.completed && x.date === ctx.today);
    const idx = (rot.next_index + n - (doneToday ? 0 : 0)) % rot.items.length;
    workout = planFor(s, rot.items[idx]);
  }
  const hol = (ctx.holidays || []).find((h: Any) => h.date === date)?.name ?? null;
  const off = s.days_off.includes(date);
  const w = s.memory.work;
  const work = w.enabled && w.days.includes(weekdayIndex(date)) && !hol && !off ? { start: w.start, end: w.end } : null;
  return { date, weekday: longDay(date).split(" ")[0], items, workout, work, holiday: hol, day_off: off };
}

const LONG_TEXT = (name: string, w: Any, kind: string) =>
  kind === "evening"
    ? `Hi, it's ${name}. You made it through the day, and that is worth a quiet moment. The morning routine got done and the stand-up went ahead, and whatever is still open can happily wait until tomorrow without any fuss.\n\nTomorrow looks gentle, with the stand-up and a pull session in the gym. Nothing needs sorting tonight.\n\nLet's wind down. Dim the lights, put the phone somewhere out of reach, and let your shoulders drop. Sleep well.`
    : kind === "weekly"
      ? `It was a steady week. The morning routine kept turning up, the stand-ups happened, and you got to the gym when it mattered. Quiet days are still good days. Next week has a dentist booking to sort and the electricity bill, both small jobs that fit around work. You are moving towards the driving test one ordinary day at a time.`
      : `Good morning! ${weatherText(w) ? `${weatherText(w)}, so a day to dress for. ` : ""}It's a work day, so the morning is yours until ten and then the stand-up opens the office stretch.\n\nLooking back at yesterday, you watered the plants a bit late, but they're fine, and it counted. That kind of steady follow-through is exactly what keeps your driving test goal moving.\n\nToday breaks down into a few small groups. The morning routine is first, then the stand-up at ten. The afternoon is gentle. Groceries on the way home, and a call with your mum in the evening. The gym is a push session, with the weights nudged up a little where last time felt easy.\n\nIf I were ordering the day, I'd do the morning routine, stand-up, a short email clear-out, then the laundry on. Lunch, a walk if the rain eases, then the gym, groceries on the way home, dinner, and finally the call with your mum. That leaves your evening unhurried.\n\nTomorrow looks lighter, just the stand-up and a pull session, so don't worry about squeezing everything in today. One thing at a time, and you'll be right on track. Have a lovely day!`;

function route(s: Any, ctx: Any, method: string, path: string, body: Any): { body: Any; mutated: boolean } {
  const [p, qs = ""] = path.split("?");
  const q = new URLSearchParams(qs);
  const id = () => s.next_id++;
  let m: RegExpMatchArray | null;
  const M = (re: RegExp) => (m = p.match(re));
  const mut = (b: Any) => ({ body: b, mutated: true });
  const ro = (b: Any) => ({ body: b, mutated: false });
  const is = (mm: string) => method === mm;
  const cleanTask = (b: Any, old?: Any) => {
    if (!b.title || !String(b.title).trim()) throw bad("title is required");
    const t: Any = { id: old?.id ?? id(), title: String(b.title).trim(), date: b.recurrence ? null : b.date ?? null, time: b.time || null,
      part_of_day: b.time ? (+b.time.slice(0, 2) < 12 ? "morning" : +b.time.slice(0, 2) < 17 ? "afternoon" : "evening") : b.part_of_day || "anytime",
      category_id: b.category_id, category_auto: false, notes: b.notes || "", recurrence: b.recurrence ?? null,
      checklist: (b.checklist || []).map((c: Any) => ({ id: c.id ?? id(), text: c.text })) };
    if (b.category_id === "auto") {
      const text = (t.title + " " + t.notes).toLowerCase();
      const kw: [string, string][] = [["gym", "Exercise"], ["run", "Exercise"], ["email", "Work"], ["meeting", "Work"], ["buy", "Errands"], ["dentist", "Appointments"], ["call", "Social"], ["bill", "Admin & Money"], ["clean", "Chores"], ["study", "Study"]];
      const hit = kw.find(([k]) => text.includes(k))?.[1] ?? "Other";
      t.category_id = s.categories.find((c: Any) => c.name === hit)?.id ?? null;
      t.category_auto = true;
    }
    return t;
  };

  if (is("GET") && p === "/api/settings") return ro({ ...s.settings, gemini_api_key_set: !!ctx.has_key });
  if (is("PUT") && p === "/api/settings") {
    const b = { ...body }; delete b.gemini_api_key;
    if (b.timezone) { try { new Intl.DateTimeFormat("en", { timeZone: b.timezone }); } catch { throw bad("unknown timezone"); } }
    Object.assign(s.settings, b);
    return mut({ ...s.settings, gemini_api_key_set: !!ctx.has_key });
  }
  if (is("GET") && p === "/api/categories") return ro(s.categories);
  if (is("POST") && p === "/api/categories") { const c = { id: id(), name: body.name, color: body.color }; s.categories.push(c); return mut(c); }
  if (M(/^\/api\/categories\/(\d+)$/)) {
    const c = s.categories.find((x: Any) => x.id === +m![1]); if (!c) throw nf();
    if (is("PUT")) { Object.assign(c, body); return mut(c); }
    if (is("DELETE")) { s.categories = s.categories.filter((x: Any) => x !== c); s.tasks.forEach((t: Any) => { if (t.category_id === c.id) t.category_id = null; }); return mut({}); }
  }
  if (is("GET") && p === "/api/tasks") return ro(s.tasks);
  if (is("POST") && p === "/api/tasks") { const t = cleanTask(body); s.tasks.push(t); return mut(t); }
  if (M(/^\/api\/tasks\/(\d+)\/categorise-prompt$/)) {
    const t = s.tasks.find((x: Any) => x.id === +m![1]); if (!t) throw nf();
    return ro({ system_prompt: "Pick one category name.", user_prompt: t.title, names: s.categories.map((c: Any) => c.name) });
  }
  if (M(/^\/api\/tasks\/(\d+)\/category$/) && is("PUT")) {
    const t = s.tasks.find((x: Any) => x.id === +m![1]); const c = s.categories.find((x: Any) => x.name.toLowerCase() === String(body.name).toLowerCase());
    if (!t || !c) throw nf(); t.category_id = c.id; t.category_auto = !!body.auto; return mut(t);
  }
  if (M(/^\/api\/tasks\/(\d+)$/)) {
    const i = s.tasks.findIndex((x: Any) => x.id === +m![1]); if (i < 0) throw nf();
    if (is("GET")) return ro(s.tasks[i]);
    if (is("PUT")) { s.tasks[i] = cleanTask(body, s.tasks[i]); if (s.missed && (!s.tasks[i].date || s.tasks[i].date >= ctx.today)) delete s.missed[s.tasks[i].id]; return mut(s.tasks[i]); }
    if (is("DELETE")) { if (s.missed) delete s.missed[s.tasks[i].id]; s.tasks.splice(i, 1); return mut({}); }
  }
  if (is("GET") && p === "/api/days") {
    const start = q.get("start")!, n = +(q.get("days") || 1);
    // roll over unfinished one-offs when today is covered
    if (start <= ctx.today && addDays(start, n - 1) >= ctx.today) rollMock(s, ctx.today);
    return mut(Array.from({ length: n }, (_, i) => dayDetail(s, ctx, addDays(start, i))));
  }
  if (is("GET") && p === "/api/missed") { rollMock(s, ctx.today); return mut(missedMock(s)); }
  if (M(/^\/api\/missed\/(\d+)\/(today|dismiss)$/) && is("POST")) {
    const t = s.tasks.find((x: Any) => x.id === +m![1]); if (!t || !s.missed[t.id]) throw nf();
    delete s.missed[t.id];
    if (m![2] === "today") { t.date = ctx.today; return mut(occurrencesFor(s, ctx.today, ctx.today).find((o) => o.task_id === t.id)); }
    s.completions[`${t.id}|`] = true; return mut({ ok: true });
  }
  if (is("POST") && p === "/api/occurrences/toggle") {
    const t = s.tasks.find((x: Any) => x.id === body.task_id); if (!t) throw nf();
    s.completions[`${t.id}|${t.recurrence ? body.date : ""}`] = !!body.done;
    return mut(occurrencesFor(s, body.date, ctx.today).find((o) => o.task_id === t.id));
  }
  if (is("POST") && p === "/api/occurrences/checklist") {
    s.checklist_done[`${body.task_id}|${body.date}|${body.item_id}`] = !!body.done;
    return mut(occurrencesFor(s, body.date, ctx.today).find((o) => o.task_id === body.task_id));
  }
  if (p === "/api/days-off") {
    if (is("POST")) { if (!s.days_off.includes(body.date)) s.days_off.push(body.date); return mut(s.days_off); }
    return ro(s.days_off);
  }
  if (M(/^\/api\/days-off\/(.+)$/) && is("DELETE")) { s.days_off = s.days_off.filter((d: string) => d !== m![1]); return mut(s.days_off); }
  if (is("GET") && p === "/api/goals") return ro(s.goals);
  if (is("POST") && p === "/api/goals") { if (!body.title) throw bad("title required"); const g = { id: id(), created: ctx.today, achieved_on: null, status: "active", why: "", deadline: null, ...body }; s.goals.push(g); return mut(g); }
  if (M(/^\/api\/goals\/(\d+)$/)) {
    const i = s.goals.findIndex((x: Any) => x.id === +m![1]); if (i < 0) throw nf();
    if (is("PUT")) { s.goals[i] = { ...s.goals[i], ...body, id: s.goals[i].id }; return mut(s.goals[i]); }
    if (is("DELETE")) { s.goals.splice(i, 1); return mut({}); }
  }
  // leisure
  if (p === "/api/leisure") {
    if (is("GET")) return ro([...s.leisure].sort((a: Any, b: Any) => ["current", "someday", "finished"].indexOf(a.status) - ["current", "someday", "finished"].indexOf(b.status)));
    if (is("POST")) {
      if (!body.title?.trim()) throw bad("title is required");
      const l = { id: id(), note: "", status: "someday", created: ctx.today, finished_on: null, last_suggested: null, blurb: "", ...body, title: body.title.trim() };
      if (l.status === "finished") l.finished_on = ctx.today;
      s.leisure.push(l); return mut(l);
    }
  }
  if (M(/^\/api\/leisure\/(\d+)\/blurb-prompt$/)) {
    const l = s.leisure.find((x: Any) => x.id === +m![1]); if (!l) throw nf();
    return ro({ id: l.id, title: l.title, system_prompt: "Write a spoiler-free blurb, or UNKNOWN.", user_prompt: `Title: ${l.title}\nWhat it is: ${l.kind}` });
  }
  if (M(/^\/api\/leisure\/(\d+)$/)) {
    const i = s.leisure.findIndex((x: Any) => x.id === +m![1]); if (i < 0) throw nf();
    if (is("PUT")) {
      const next = { ...s.leisure[i], ...body, id: s.leisure[i].id };
      next.finished_on = next.status === "finished" ? (s.leisure[i].finished_on ?? ctx.today) : null;
      s.leisure[i] = next; return mut(next);
    }
    if (is("DELETE")) { s.leisure.splice(i, 1); return mut({}); }
  }
  if (is("POST") && p === "/api/profile/apply") {
    if (body.v !== 1) throw bad("unsupported profile version");
    const r = { interests: false, notes: 0, work: false, weather: false, goals_added: 0, leisure_added: 0, tasks_added: 0 };
    if (typeof body.interests === "string") { s.memory.interests = body.interests.trim(); r.interests = true; }
    if (Array.isArray(body.notes)) { s.memory.notes = body.notes.map((t: string) => ({ id: id(), text: t.trim() })); r.notes = s.memory.notes.length; }
    if (body.work) { s.memory.work = { ...s.memory.work, ...body.work }; r.work = true; }
    if (body.weather) { s.memory.weather = { ...s.memory.weather, ...body.weather }; r.weather = true; }
    const has = (list: Any[], t: string) => list.some((x: Any) => x.title.trim().toLowerCase() === t.trim().toLowerCase());
    for (const g of body.add_goals ?? []) if (!has(s.goals, g.title)) { s.goals.push({ id: id(), created: ctx.today, achieved_on: null, status: "active", why: "", deadline: null, ...g }); r.goals_added++; }
    for (const l of body.add_leisure ?? []) if (!has(s.leisure, l.title)) { s.leisure.push({ id: id(), note: "", status: "someday", created: ctx.today, finished_on: null, last_suggested: null, blurb: "", ...l }); r.leisure_added++; }
    for (const t of body.add_tasks ?? []) {
      if (s.tasks.some((x: Any) => x.title.trim().toLowerCase() === t.title.trim().toLowerCase() && (x.date ?? null) === (t.date ?? null))) continue;
      const cat = t.category && t.category !== "auto" ? s.categories.find((c: Any) => c.name.toLowerCase() === String(t.category).toLowerCase()) : null;
      s.tasks.push({ id: id(), title: t.title, date: t.date ?? null, time: t.time ?? null, part_of_day: t.part_of_day ?? "anytime", category_id: cat?.id ?? null, category_auto: t.category === "auto", notes: t.notes ?? "", recurrence: t.recurrence ?? null, checklist: (t.checklist ?? []).map((text: string) => ({ id: id(), text })) });
      r.tasks_added++;
    }
    s.memory.profile_updated = body.updated;
    return mut(r);
  }
  if (is("POST") && p === "/api/profile/clear") {
    s.memory.interests = ""; s.memory.notes = []; s.memory.profile_updated = "";
    s.memory.work = { enabled: true, days: [0, 1, 2, 3, 4], start: "10:00", end: "17:00", holiday_region: "AU-NSW", lunch_minutes: 60, lunch_window_start: "12:00", lunch_window_end: "14:00" };
    s.memory.weather = { comfortable_max: 18, hot_above: 22, too_hot_above: 25, likes_rain: true, avoid_sunburn: true };
    return mut({ cleared: true });
  }
  if (is("PUT") && p === "/api/memory/interests") { s.memory.interests = String(body.text ?? "").trim(); return mut(s.memory); }
  // memory
  if (is("GET") && p === "/api/memory") return ro(s.memory);
  if (M(/^\/api\/memory\/(weather|work|rhythm)$/) && is("PUT")) {
    const k = m![1]; const next = { ...s.memory[k], ...body };
    if (k === "weather" && !(next.comfortable_max <= next.hot_above && next.hot_above <= next.too_hot_above)) throw bad("thresholds must be comfortable <= hot <= too hot");
    if (k === "work" && !(next.start < next.end)) throw bad("start must be before end");
    s.memory[k] = next; return mut(s.memory);
  }
  if (is("POST") && p === "/api/memory/notes") { if (!body.text?.trim()) throw bad("note text required"); s.memory.notes.push({ id: id(), text: body.text.trim() }); return mut(s.memory); }
  if (M(/^\/api\/memory\/notes\/(\d+)$/)) {
    const n = s.memory.notes.find((x: Any) => x.id === +m![1]); if (!n) throw nf();
    if (is("PUT")) n.text = body.text; else s.memory.notes = s.memory.notes.filter((x: Any) => x !== n);
    return mut(s.memory);
  }
  // gym
  if (p === "/api/exercises") { if (is("POST")) { const e = { id: id(), name: body.name }; s.gym.exercises.push(e); return mut(e); } return ro(s.gym.exercises); }
  if (M(/^\/api\/exercises\/(\d+)\/history$/)) {
    const ex = +m![1];
    return ro(s.gym.sessions.filter((x: Any) => x.completed).sort((a: Any, b: Any) => a.date.localeCompare(b.date)).flatMap((x: Any) => {
      const ds = x.sets.filter((q: Any) => q.exercise_id === ex && q.done); if (!ds.length) return [];
      const top = Math.max(...ds.map((q: Any) => q.weight)); const best = Math.max(...ds.filter((q: Any) => q.weight === top).map((q: Any) => q.reps));
      return [{ date: x.date, top_weight: top, best_reps: best, est_1rm: Math.round(top * (1 + best / 30) * 10) / 10 }];
    }));
  }
  if (p === "/api/templates") {
    if (is("POST")) { const t = { id: id(), name: body.name, kind: body.kind ?? "strength", note: body.note ?? "", options: body.options ?? [], minutes_min: body.minutes_min ?? null, minutes_max: body.minutes_max ?? null, exercises: (body.exercises || []).map((e: Any) => ({ ...e, name: s.gym.exercises.find((x: Any) => x.id === e.exercise_id)?.name })) }; s.gym.templates.push(t); return mut(t); }
    return ro(s.gym.templates);
  }
  if (M(/^\/api\/templates\/(\d+)$/)) {
    const i = s.gym.templates.findIndex((x: Any) => x.id === +m![1]); if (i < 0) throw nf();
    if (is("PUT")) { s.gym.templates[i] = { id: s.gym.templates[i].id, name: body.name, kind: body.kind ?? "strength", note: body.note ?? "", options: body.options ?? [], minutes_min: body.minutes_min ?? null, minutes_max: body.minutes_max ?? null, exercises: body.exercises.map((e: Any) => ({ ...e, name: s.gym.exercises.find((x: Any) => x.id === e.exercise_id)?.name })) }; return mut(s.gym.templates[i]); }
    if (is("DELETE")) { s.gym.templates.splice(i, 1); s.gym.rotation.items = s.gym.rotation.items.filter((x: Any) => x.rest || x.template_id !== +m![1]); return mut({}); }
  }
  if (p === "/api/rotation") {
    if (is("PUT")) { s.gym.rotation.items = body.items; s.gym.rotation.next_index = Math.min(s.gym.rotation.next_index, Math.max(0, body.items.length - 1)); return mut(s.gym.rotation); }
    return ro(s.gym.rotation);
  }
  if (is("GET") && p === "/api/gym/next") return ro(planFor(s, s.gym.rotation.items[s.gym.rotation.next_index]));
  if (is("POST") && p === "/api/gym/rest") { const r = s.gym.rotation; r.next_index = (r.next_index + 1) % Math.max(1, r.items.length); return mut(r); }
  if (is("POST") && (p === "/api/gym/skip-rest" || p === "/api/gym/skip")) {
    const r = s.gym.rotation; if (p.endsWith("-rest") && !r.items[r.next_index]?.rest) throw bad("the next rotation item is not a rest day");
    r.next_index = (r.next_index + 1) % Math.max(1, r.items.length); s.gym.next_since = ctx.today; return mut(r);
  }
  if (is("POST") && p === "/api/sessions") {
    const open = s.gym.sessions.find((x: Any) => !x.completed && x.date === ctx.today); if (open) return ro(open);
    const plan = planFor(s, { template_id: body.template_id }); if (plan.rest) throw nf("template not found");
    const sets: Any[] = [];
    for (const e of plan.exercises) for (let n = 1; n <= e.sets; n++) sets.push({ exercise_id: e.exercise_id, set_no: n, weight: e.suggested_weight ?? 0, reps: e.rep_max, done: false });
    const sess: Any = { id: id(), template_id: plan.template_id, name: plan.name, date: ctx.today, completed: false, sets };
    if (plan.kind === "cardio") sess.cardio = { activity: "", minutes: 30 };
    s.gym.sessions.push(sess); return mut(sess);
  }
  if (M(/^\/api\/sessions\/(\d+)\/sets$/) && is("PUT")) { const x = s.gym.sessions.find((y: Any) => y.id === +m![1]); if (!x) throw nf(); x.sets = body.sets; return mut(x); }
  if (M(/^\/api\/sessions\/(\d+)\/cardio$/) && is("PUT")) { const x = s.gym.sessions.find((y: Any) => y.id === +m![1]); if (!x) throw nf(); x.cardio = { activity: body.activity, minutes: body.minutes }; return mut(x); }
  if (is("GET") && p === "/api/state/migrate") return ro({ version: 8 });
  if (M(/^\/api\/sessions\/(\d+)\/complete$/) && is("POST")) {
    const x = s.gym.sessions.find((y: Any) => y.id === +m![1]); if (!x) throw nf(); x.completed = true;
    const r = s.gym.rotation; const n = r.items.length;
    for (let k = 0; k < n; k++) { const i = (r.next_index + k) % n; if (r.items[i].template_id === x.template_id) { r.next_index = (i + 1) % n; break; } }
    return mut(x);
  }
  // misc
  if (M(/^\/api\/weather-summary$/)) { const c = +(q.get("code") ?? 0); const [summary, icon] = WMO[c] ?? ["Cloudy", "cloud"]; return ro({ summary, icon }); }
  if (is("POST") && p === "/api/holidays/filter") return ro((body.raw || []).filter((h: Any) => h.global || (h.counties || []).includes(body.region)).map((h: Any) => ({ date: h.date, name: h.localName || h.name })));
  if (is("GET") && p === "/api/export") return ro(s);
  if (is("POST") && p === "/api/import") {
    if (!body || body.version !== 1 || !Array.isArray(body.tasks) || !body.settings) throw bad("This doesn't look like a backup file");
    Object.keys(s).forEach((k) => delete s[k]); Object.assign(s, body); return mut(s);
  }
  if (is("POST") && p === "/api/sample-data") { sampleData(s, ctx.today); return mut({ ok: true }); }
  if (is("GET") && p === "/api/briefing") {
    const kind = q.get("kind") || "morning", date = q.get("date") || ctx.today;
    const w = ctx.weather?.[date] ?? null;
    const day = occurrencesFor(s, date, ctx.today);
    return ro({ kind, date, weather: w, stats: { total: day.length, done: day.filter((o) => o.done).length, gym_sessions: s.gym.sessions.filter((x: Any) => x.completed).length },
      affirmation: "One thing at a time.", template_text: LONG_TEXT(s.settings.assistant_name, w, kind), cache_key: `mock-${kind}-${date}-${day.map((o) => +o.done).join("")}`,
      system_prompt: "Write the briefing.", user_prompt: "Day data: " + JSON.stringify(day.map((o) => o.title)), max_words: 450 });
  }
  if (is("GET") && p === "/api/digest") {
    const date = q.get("date") || ctx.today; const w = ctx.weather?.[date] ?? null;
    return ro({ date, weather: w, cache_key: `mock-digest-${date}`, system_prompt: "Digest as JSON.", user_prompt: "Day",
      template: { weather_comment: w ? `${w.summary}, ${w.temp_max} at most. ${w.precip_prob >= 40 ? "Nice to listen to the rain from your desk." : "A comfortable one."}` : null, overview: "A balanced day with a few small jobs around work." } });
  }
  throw nf(`${method} ${p} is not in the mock core`);
}

export function call(stateJson: string, method: string, path: string, bodyJson: string, ctxJson: string): string {
  const s = JSON.parse(stateJson), ctx = JSON.parse(ctxJson);
  const before = stateJson;
  try {
    const body = bodyJson && bodyJson !== "null" ? JSON.parse(bodyJson) : null;
    const r = route(s, ctx, method, path, body);
    const after = JSON.stringify(s);
    return JSON.stringify({ status: 200, body: r.body, state: after !== before ? s : null });
  } catch (e) {
    const status = e instanceof HttpError ? e.status : 500;
    return JSON.stringify({ status, body: { error: (e as Error).message }, state: null });
  }
}
