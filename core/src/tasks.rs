//! Categories, tasks, recurrence, auto-categorisation, day views, rollover and occurrences.
use crate::env::Env;
use crate::error::{ApiErr, Res};
use crate::gym::{self, WorkoutPlan};
use crate::holidays;
use crate::model::*;
use crate::util::*;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

// ---------------------------------------------------------------- categories

#[derive(Deserialize)]
pub struct CategoryIn {
    name: String,
    color: String,
}

fn check_category(c: &CategoryIn) -> Res<()> {
    if c.name.trim().is_empty() {
        return Err(ApiErr::bad("category name cannot be empty"));
    }
    if c.color.trim().is_empty() {
        return Err(ApiErr::bad("category color cannot be empty"));
    }
    Ok(())
}

pub fn create_category(env: &mut Env, b: CategoryIn) -> Res<Category> {
    check_category(&b)?;
    let c = Category { id: env.st.new_id(), name: b.name.trim().into(), color: b.color.trim().into() };
    env.st.categories.push(c.clone());
    env.changed = true;
    Ok(c)
}

pub fn update_category(env: &mut Env, id: i64, b: CategoryIn) -> Res<Category> {
    check_category(&b)?;
    let c = env.st.categories.iter_mut().find(|c| c.id == id).ok_or_else(|| ApiErr::not_found("category not found"))?;
    c.name = b.name.trim().into();
    c.color = b.color.trim().into();
    env.changed = true;
    Ok(c.clone())
}

pub fn delete_category(env: &mut Env, id: i64) -> Res<Value> {
    let n = env.st.categories.len();
    env.st.categories.retain(|c| c.id != id);
    if env.st.categories.len() == n {
        return Err(ApiErr::not_found("category not found"));
    }
    for t in env.st.tasks.iter_mut().filter(|t| t.category_id == Some(id)) {
        t.category_id = None;
    }
    env.changed = true;
    Ok(json!({"ok": true}))
}

// ---------------------------------------------------------------- recurrence

impl Recurrence {
    pub fn matches(&self, d: NaiveDate) -> bool {
        let wd = d.weekday().num_days_from_monday() as u8;
        match self {
            Recurrence::Daily => true,
            Recurrence::Weekdays => wd < 5,
            Recurrence::Weekly { days } => days.contains(&wd),
        }
    }
}

const PARTS: [&str; 4] = ["morning", "afternoon", "evening", "anytime"];

pub fn part_order(p: &str) -> usize {
    PARTS.iter().position(|x| *x == p).unwrap_or(3)
}

fn derive_part(time: &str) -> &'static str {
    let h: u32 = time.get(..2).and_then(|x| x.parse().ok()).unwrap_or(0);
    if h < 12 {
        "morning"
    } else if h < 17 {
        "afternoon"
    } else {
        "evening"
    }
}

// ------------------------------------------------------- auto-categorisation

const KEYWORDS: &[(&str, &[&str])] = &[
    ("Chores", &["laundry", "hoover", "vacuum", "clean", "cleaning", "dishes", "dishwasher", "bins", "bin", "tidy", "iron", "ironing", "wash", "mop", "dust", "sweep", "declutter", "fix", "repair", "tap"]),
    ("Exercise", &["gym", "run", "running", "jog", "walk", "yoga", "swim", "workout", "cycle", "cycling", "stretch", "push", "pull", "leg", "legs", "pilates", "hike", "exercise", "lift", "cardio"]),
    ("Study", &["study", "studying", "revise", "revision", "essay", "read chapter", "chapter", "lecture", "homework", "exam", "assignment", "course", "coursework", "revising", "dissertation", "thesis", "read about", "research", "learn", "learning", "lesson", "lessons", "class", "tutorial", "flashcards", "vocab", "korean", "japanese", "chinese", "mandarin", "jlpt", "topik"]),
    ("Work", &["meeting", "stand-up", "standup", "report", "email work", "client", "deadline", "presentation", "shift", "project", "sprint", "colleague", "boss", "compliance", "excel", "spreadsheet", "asx", "docs", "pinpoint", "audit", "agenda", "minutes", "timesheet", "roster", "work", "office", "academy", "newsroom", "stakeholder", "onboarding", "workshop", "training", "slides"]),
    ("Errands", &["shop", "shopping", "groceries", "grocery", "post office", "pick up", "collect", "buy", "return", "parcel", "pharmacy", "supermarket", "drop off"]),
    ("Appointments", &["dentist", "doctor", "gp", "appointment", "haircut", "vet", "optician", "checkup", "check-up", "physio", "hospital", "dermatologist"]),
    ("Social", &["call", "mum", "dad", "friend", "friends", "party", "dinner with", "drinks", "birthday", "visit", "coffee with", "lunch with", "date night", "catch up", "brunch", "book club", "text", "reply to"]),
    ("Self-care", &["rest", "nap", "bath", "skincare", "meditate", "meditation", "journal", "read for fun", "read a novel", "read a book", "novel", "relax", "therapy", "massage", "sleep", "breathe", "morning routine"]),
    ("Admin & Money", &["bill", "bills", "pay", "bank", "tax", "budget", "form", "renew", "insurance", "rent", "council", "mortgage", "invoice", "passport", "visa", "savings", "subscription", "inquiries", "inquiry", "enquiries", "enquiry", "email", "emails", "sign up", "signup", "register", "apply"]),
];

fn words(s: &str) -> String {
    let w: Vec<String> = s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect();
    format!(" {} ", w.join(" "))
}

/// Keyword match against existing category names only. Best score wins, ties go to table order.
pub fn keyword_category(text: &str, cats: &[Category]) -> Option<i64> {
    // the task brain's concept decides first (the same match gives the duration and the wording)
    if let Some(name) = crate::taskbrain::category_for(text) {
        if let Some(c) = cats.iter().find(|c| c.name.trim().eq_ignore_ascii_case(name)) {
            return Some(c.id);
        }
    }
    let hay = words(text);
    let mut best: Option<(usize, i64)> = None;
    for (name, kws) in KEYWORDS {
        let Some(cat) = cats.iter().find(|c| c.name.trim().eq_ignore_ascii_case(name)) else { continue };
        let score = kws.iter().filter(|k| hay.contains(&words(k))).count();
        if score > 0 && best.is_none_or(|(s, _)| score > s) {
            best = Some((score, cat.id));
        }
    }
    best.map(|(_, id)| id)
}

/// Tasks still filed under "Other" get another look with the current keyword table (it grows over time).
/// A task the person filed somewhere else themselves is never touched. Returns true if anything changed.
pub fn recategorise_other(st: &mut State) -> bool {
    let Some(other) = st.categories.iter().find(|c| c.name.trim().eq_ignore_ascii_case("other")).map(|c| c.id) else { return false };
    let mut changed = false;
    let cats = st.categories.clone();
    for t in st.tasks.iter_mut().filter(|t| t.category_id == Some(other)) {
        if let Some(id) = keyword_category(&t.title, &cats).or_else(|| keyword_category(&format!("{} {}", t.title, t.notes), &cats)) {
            if id != other {
                t.category_id = Some(id);
                changed = true;
            }
        }
    }
    changed
}

fn other_category(st: &mut State) -> i64 {
    if let Some(c) = st.categories.iter().find(|c| c.name.trim().eq_ignore_ascii_case("other")) {
        return c.id;
    }
    let id = st.new_id();
    st.categories.push(Category { id, name: "Other".into(), color: "#8C847D".into() });
    id
}

/// Keyword categorisation with the "Other" fallback (creates "Other" if it does not exist).
pub fn auto_category(st: &mut State, title: &str, notes: &str) -> i64 {
    let chosen = keyword_category(title, &st.categories).or_else(|| keyword_category(&format!("{title} {notes}"), &st.categories));
    match chosen {
        Some(id) => id,
        None => other_category(st),
    }
}

// --------------------------------------------------------------------- tasks

#[derive(Deserialize)]
pub struct ChecklistIn {
    #[serde(default)]
    pub(crate) id: Option<i64>,
    pub(crate) text: String,
}

#[derive(Deserialize)]
pub struct TaskIn {
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) date: Option<String>,
    #[serde(default)]
    pub(crate) time: Option<String>,
    #[serde(default)]
    pub(crate) part_of_day: Option<String>,
    #[serde(default)]
    pub(crate) category_id: Option<Value>,
    #[serde(default)]
    pub(crate) notes: String,
    #[serde(default)]
    pub(crate) recurrence: Option<Recurrence>,
    #[serde(default)]
    pub(crate) checklist: Vec<ChecklistIn>,
    /// how long it takes in minutes, when the person says so (absent or null: Miranda guesses)
    #[serde(default)]
    pub(crate) minutes: Option<u32>,
}

/// The person's own figure wins; otherwise what they taught Miranda for a task of the same name.
fn minutes_for(st: &mut State, title: &str, given: Option<u32>) -> Option<u32> {
    let key = title.trim().to_lowercase();
    match given.filter(|m| (1..=1440).contains(m)) {
        Some(m) => {
            st.learned_minutes.insert(key, m);
            Some(m)
        }
        None => st.learned_minutes.get(&key).copied(),
    }
}

struct Clean {
    title: String,
    date: Option<String>,
    time: Option<String>,
    part: String,
    notes: String,
    recurrence: Option<Recurrence>,
}

fn clean(b: &TaskIn) -> Res<Clean> {
    let title = b.title.trim().to_string();
    if title.is_empty() {
        return Err(ApiErr::bad("title cannot be empty"));
    }
    let time = match b.time.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(t) => Some(NaiveTime::parse_from_str(t, "%H:%M").map_err(|_| ApiErr::bad("time must be HH:MM (24h)"))?.format("%H:%M").to_string()),
    };
    let part = match b.part_of_day.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(p) if PARTS.contains(&p) => Some(p.to_string()),
        Some(_) => return Err(ApiErr::bad("part_of_day must be morning, afternoon, evening or anytime")),
    };
    let mut recurrence = b.recurrence.clone();
    if let Some(Recurrence::Weekly { days }) = &mut recurrence {
        if days.is_empty() || days.iter().any(|d| *d > 6) {
            return Err(ApiErr::bad("weekly recurrence needs days between 0 (Mon) and 6 (Sun)"));
        }
        days.sort_unstable();
        days.dedup();
    }
    let date = match (&recurrence, b.date.as_deref().map(str::trim)) {
        (Some(_), _) => None,
        (None, None) | (None, Some("")) => return Err(ApiErr::bad("one-off tasks need a date (or give a recurrence)")),
        (None, Some(d)) => Some(fmt_date(parse_date(d).ok_or_else(|| ApiErr::bad("date must be YYYY-MM-DD"))?)),
    };
    let part_of_day = match &time {
        Some(t) => derive_part(t).to_string(),
        None => part.unwrap_or_else(|| "anytime".into()),
    };
    Ok(Clean { title, date, time, part: part_of_day, notes: b.notes.clone(), recurrence })
}

enum CatSpec {
    Manual(Option<i64>),
    Auto,
}

fn parse_cat_spec(st: &State, v: &Option<Value>) -> Res<CatSpec> {
    match v {
        None | Some(Value::Null) => Ok(CatSpec::Manual(None)),
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("auto") => Ok(CatSpec::Auto),
        Some(Value::Number(n)) => {
            let cid = n.as_i64().ok_or_else(|| ApiErr::bad("category_id must be a number, null or \"auto\""))?;
            if !st.categories.iter().any(|c| c.id == cid) {
                return Err(ApiErr::bad("category_id does not exist"));
            }
            Ok(CatSpec::Manual(Some(cid)))
        }
        Some(_) => Err(ApiErr::bad("category_id must be a number, null or \"auto\"")),
    }
}

fn sync_checklist(st: &mut State, task_idx: usize, items: &[ChecklistIn]) {
    let tid = st.tasks[task_idx].id;
    let existing: HashSet<i64> = st.tasks[task_idx].checklist.iter().map(|c| c.id).collect();
    let mut out: Vec<ChecklistItem> = vec![];
    for it in items.iter().filter(|i| !i.text.trim().is_empty()) {
        let id = match it.id.filter(|i| existing.contains(i) && !out.iter().any(|o| o.id == *i)) {
            Some(id) => id,
            None => st.new_id(),
        };
        out.push(ChecklistItem { id, text: it.text.trim().to_string() });
    }
    let keep: HashSet<i64> = out.iter().map(|c| c.id).collect();
    for id in existing.difference(&keep) {
        st.checklist_done.retain(|k, _| {
            let mut p = k.split('|');
            !(p.next() == Some(&tid.to_string()) && p.nth(1) == Some(&id.to_string()))
        });
    }
    st.tasks[task_idx].checklist = out;
}

fn clear_done_state(st: &mut State, tid: i64) {
    let prefix = format!("{tid}|");
    st.completions.retain(|k, _| !k.starts_with(&prefix));
    st.checklist_done.retain(|k, _| !k.starts_with(&prefix));
}

pub fn list_tasks(env: &Env) -> Value {
    json!(env.st.tasks)
}

pub fn get_task(env: &Env, id: i64) -> Res<Task> {
    env.st.tasks.iter().find(|t| t.id == id).cloned().ok_or_else(|| ApiErr::not_found("task not found"))
}

pub fn create_task(env: &mut Env, b: TaskIn) -> Res<Task> {
    let c = clean(&b)?;
    let spec = parse_cat_spec(env.st, &b.category_id)?;
    let (category_id, auto) = match spec {
        CatSpec::Manual(id) => (id, false),
        CatSpec::Auto => (Some(auto_category(env.st, &c.title, &c.notes)), true),
    };
    let id = env.st.new_id();
    let minutes = minutes_for(env.st, &c.title, b.minutes);
    env.st.tasks.push(Task {
        id,
        minutes,
        title: c.title,
        date: c.date,
        time: c.time,
        part_of_day: c.part,
        category_id,
        category_auto: auto,
        notes: c.notes,
        recurrence: c.recurrence,
        checklist: vec![],
    });
    let idx = env.st.tasks.len() - 1;
    sync_checklist(env.st, idx, &b.checklist);
    env.changed = true;
    Ok(env.st.tasks[idx].clone())
}

/// `GET /api/tasks/guess?title=`: how Miranda reads a title, so the person can see it and correct it:
/// how she would say it, where she would file it, and how long she thinks it takes.
pub fn guess(env: &Env, title: &str) -> Value {
    let t = title.trim();
    if t.is_empty() {
        return json!({"says": null, "category": null, "takes": null, "learned": null});
    }
    let cat = keyword_category(t, &env.st.categories).and_then(|id| env.st.categories.iter().find(|c| c.id == id)).map(|c| c.name.clone());
    let learned = env.st.learned_minutes.get(&t.to_lowercase()).copied();
    let takes = crate::taskbrain::typical_for(t).map(|ty| format!("{}{}", crate::taskbrain::length_text(&ty), if ty.outing { ", plus getting there" } else { "" }));
    json!({"says": crate::taskvoice::task_phrase(t).text(), "category": cat, "takes": takes, "learned": learned})
}

/// `PUT /api/tasks/{id}/part` {"part_of_day"}: just the part of the day (the morning check-in).
pub fn set_part(env: &mut Env, id: i64, b: &Value) -> Res<Task> {
    let part = b["part_of_day"].as_str().unwrap_or("");
    if !["morning", "afternoon", "evening", "anytime"].contains(&part) {
        return Err(ApiErr::bad("part_of_day must be morning, afternoon, evening or anytime"));
    }
    let t = env.st.tasks.iter_mut().find(|t| t.id == id).ok_or_else(|| ApiErr::not_found("task not found"))?;
    t.part_of_day = part.to_string();
    let out = t.clone();
    env.changed = true;
    Ok(out)
}

pub fn update_task(env: &mut Env, id: i64, b: TaskIn) -> Res<Task> {
    let idx = env.st.tasks.iter().position(|t| t.id == id).ok_or_else(|| ApiErr::not_found("task not found"))?;
    let c = clean(&b)?;
    let spec = parse_cat_spec(env.st, &b.category_id)?;
    let (category_id, auto) = match spec {
        CatSpec::Manual(id) => (id, false),
        CatSpec::Auto => (Some(auto_category(env.st, &c.title, &c.notes)), true),
    };
    let old_rec = env.st.tasks[idx].recurrence.is_some();
    let old_date = env.st.tasks[idx].date.clone();
    {
        // an explicit figure is learned; clearing it (null) also forgets what was learned for that name
        let minutes = match b.minutes {
            Some(_) => minutes_for(env.st, &c.title, b.minutes),
            None => {
                env.st.learned_minutes.remove(&c.title.trim().to_lowercase());
                None
            }
        };
        let t = &mut env.st.tasks[idx];
        t.minutes = minutes;
        t.title = c.title;
        t.time = c.time;
        t.part_of_day = c.part;
        t.category_id = category_id;
        t.category_auto = auto;
        t.notes = c.notes;
        t.recurrence = c.recurrence;
        t.date = c.date.clone();
    }
    // keep the rollover marker only while the date is unchanged
    if c.date != old_date {
        env.st.rolled_over.remove(&id.to_string());
    }
    // a new date (today or later) or a repeat makes it no longer missed
    if env.st.tasks[idx].recurrence.is_some() || c.date.as_deref().is_some_and(|d| d >= fmt_date(env.today).as_str()) {
        env.st.missed.remove(&id.to_string());
    }
    if old_rec != env.st.tasks[idx].recurrence.is_some() {
        clear_done_state(env.st, id);
        env.st.rolled_over.remove(&id.to_string());
    }
    sync_checklist(env.st, idx, &b.checklist);
    env.changed = true;
    Ok(env.st.tasks[idx].clone())
}

pub fn delete_task(env: &mut Env, id: i64) -> Res<Value> {
    let n = env.st.tasks.len();
    env.st.tasks.retain(|t| t.id != id);
    if env.st.tasks.len() == n {
        return Err(ApiErr::not_found("task not found"));
    }
    clear_done_state(env.st, id);
    env.st.rolled_over.remove(&id.to_string());
    env.st.missed.remove(&id.to_string());
    env.changed = true;
    Ok(json!({"ok": true}))
}

pub const CATEGORISE_SYSTEM: &str = "You categorise to-do tasks. Reply with exactly one category name from the given list, copied exactly, and nothing else.";

/// `GET /api/tasks/{id}/categorise-prompt`
pub fn categorise_prompt(env: &Env, id: i64) -> Res<Value> {
    let t = get_task(env, id)?;
    let names: Vec<&str> = env.st.categories.iter().map(|c| c.name.as_str()).collect();
    let user = format!(
        "Task title: {}\nNotes: {}\nCategories: {}\nWhich one category fits best?",
        t.title,
        if t.notes.trim().is_empty() { "(none)" } else { t.notes.trim() },
        names.join(", ")
    );
    Ok(json!({"system_prompt": CATEGORISE_SYSTEM, "user_prompt": user, "names": names}))
}

/// `PUT /api/tasks/{id}/category` body `{"name","auto"}`
pub fn set_category(env: &mut Env, id: i64, b: &Value) -> Res<Task> {
    let name = b["name"].as_str().unwrap_or("").trim().trim_matches(|c: char| c == '"' || c == '\'' || c == '.' || c == '*').trim().to_string();
    let auto = b["auto"].as_bool().unwrap_or(true);
    let cat = env.st.categories.iter().find(|c| c.name.trim().eq_ignore_ascii_case(&name)).map(|c| c.id).ok_or_else(|| ApiErr::bad("no category with that name"))?;
    let t = env.st.tasks.iter_mut().find(|t| t.id == id).ok_or_else(|| ApiErr::not_found("task not found"))?;
    t.category_id = Some(cat);
    t.category_auto = auto;
    env.changed = true;
    Ok(t.clone())
}

// --------------------------------------------------------------- occurrences

#[derive(Serialize, Clone, Debug)]
pub struct OccChecklist {
    pub id: i64,
    pub text: String,
    pub done: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Occurrence {
    pub task_id: i64,
    pub date: String,
    pub title: String,
    pub time: Option<String>,
    pub part_of_day: String,
    pub category: Option<Category>,
    pub notes: String,
    /// how long the person says it takes (minutes); None: Miranda guesses
    pub minutes: Option<u32>,
    pub recurring: bool,
    pub rolled_over_from: Option<String>,
    pub done: bool,
    pub checklist: Vec<OccChecklist>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct WorkHours {
    pub start: String,
    pub end: String,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct DayInfo {
    pub work: Option<WorkHours>,
    pub holiday: Option<String>,
    pub day_off: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Day {
    pub date: String,
    pub weekday: String,
    pub items: Vec<Occurrence>,
    pub workout: Option<WorkoutPlan>,
    /// Work hours for the day (not a task); null on non-work days, days off and holidays.
    pub work: Option<WorkHours>,
    pub holiday: Option<String>,
    pub day_off: bool,
}

impl Day {
    pub fn info(&self) -> DayInfo {
        DayInfo { work: self.work.clone(), holiday: self.holiday.clone(), day_off: self.day_off }
    }
}

/// (work hours, public holiday name, is a day off) for a date.
pub fn day_info(st: &State, ctx: &Ctx, d: NaiveDate) -> DayInfo {
    let w = &st.memory.work;
    let ds = fmt_date(d);
    let off = st.days_off.contains(&ds);
    if !w.enabled {
        return DayInfo { work: None, holiday: None, day_off: off };
    }
    let holiday = holidays::lookup(ctx, &ds);
    let works = w.days.contains(&d.weekday().num_days_from_monday()) && holiday.is_none() && !off;
    DayInfo { work: works.then(|| resolve_hours(w, d)), holiday, day_off: off }
}

/// Hours for a work day: first matching monthly rule > weekday override > default start/end.
pub fn resolve_hours(w: &WorkPrefs, d: NaiveDate) -> WorkHours {
    let wd = d.weekday().num_days_from_monday();
    let nth = ((d.day() - 1) / 7 + 1) as i32;
    let is_last = d.day() + 7 > days_in_month(d);
    if let Some(r) = w.monthly_rules.iter().find(|r| r.weekday == wd && (r.nth == nth || (r.nth == -1 && is_last))) {
        return WorkHours { start: r.start.clone(), end: r.end.clone() };
    }
    if let Some(h) = w.weekday_hours.get(&wd.to_string()) {
        return WorkHours { start: h.start.clone(), end: h.end.clone() };
    }
    WorkHours { start: w.start.clone(), end: w.end.clone() }
}

fn days_in_month(d: NaiveDate) -> u32 {
    let (y, m) = if d.month() == 12 { (d.year() + 1, 1) } else { (d.year(), d.month() + 1) };
    NaiveDate::from_ymd_opt(y, m, 1).unwrap().pred_opt().unwrap().day()
}


/// Unfinished one-off tasks dated before `today`: untimed ones (only a part of the day, or anytime) move to
/// `today`; ones with a clock time keep their date and are marked in `st.missed`. Returns true if anything changed.
pub fn rollover(st: &mut State, today: NaiveDate) -> bool {
    let t = fmt_date(today);
    let mut changed = false;
    let done: HashSet<String> = st.completions.keys().cloned().collect();
    // drop marks of tasks that are gone, done, recurring or no longer in the past
    let before = st.missed.len();
    let keep: Vec<String> = st
        .tasks
        .iter()
        .filter(|k| k.recurrence.is_none() && k.date.as_deref().is_some_and(|d| d < t.as_str()) && !done.contains(&format!("{}|", k.id)))
        .map(|k| k.id.to_string())
        .collect();
    st.missed.retain(|k, _| keep.contains(k));
    changed |= st.missed.len() != before;
    let mut moved: Vec<(usize, String)> = vec![];
    let mut missed: Vec<(String, String)> = vec![];
    for (i, task) in st.tasks.iter().enumerate() {
        if task.recurrence.is_some() {
            continue;
        }
        let Some(d) = task.date.clone() else { continue };
        if d.as_str() < t.as_str() && !done.contains(&format!("{}|", task.id)) {
            // only things pinned to a clock time wait for a decision; anything with just a part of the day carries over
            if task.time.is_some() {
                missed.push((task.id.to_string(), d));
            } else {
                moved.push((i, d));
            }
        }
    }
    for (id, d) in missed {
        if st.missed.get(&id) != Some(&d) {
            st.missed.insert(id, d);
            changed = true;
        }
    }
    for (i, d) in moved {
        let id = st.tasks[i].id.to_string();
        st.missed.remove(&id);
        st.rolled_over.entry(id).or_insert(d);
        st.tasks[i].date = Some(t.clone());
        changed = true;
    }
    changed
}

/// Missed items (original date, as occurrences on that date), oldest first.
pub fn missed_list(st: &State) -> Vec<Occurrence> {
    let mut out: Vec<Occurrence> = st
        .missed
        .iter()
        .filter_map(|(id, d)| {
            let id: i64 = id.parse().ok()?;
            let o = occurrence(st, id, parse_date(d)?).ok()?;
            (!o.done).then_some(o)
        })
        .collect();
    out.sort_by(|a, b| a.date.cmp(&b.date).then(a.time.cmp(&b.time)).then(a.task_id.cmp(&b.task_id)));
    out
}

/// `GET /api/missed`
pub fn missed(env: &mut Env) -> Value {
    if rollover(env.st, env.today) {
        env.changed = true;
    }
    json!(missed_list(env.st))
}

fn take_missed(env: &mut Env, id: i64) -> Res<NaiveDate> {
    let d = env.st.missed.get(&id.to_string()).and_then(|d| parse_date(d)).ok_or_else(|| ApiErr::not_found("missed item not found"))?;
    env.st.tasks.iter().find(|t| t.id == id).ok_or_else(|| ApiErr::not_found("task not found"))?;
    Ok(d)
}

/// `POST /api/missed/{id}/today`: move to today, keeping the time.
pub fn missed_today(env: &mut Env, id: i64) -> Res<Occurrence> {
    take_missed(env, id)?;
    let t = fmt_date(env.today);
    if let Some(task) = env.st.tasks.iter_mut().find(|x| x.id == id) {
        task.date = Some(t.clone());
    }
    env.st.missed.remove(&id.to_string());
    env.st.rolled_over.remove(&id.to_string());
    env.changed = true;
    occurrence(env.st, id, env.today)
}

/// `POST /api/missed/{id}/dismiss`: counts as handled (done on its original date).
pub fn missed_dismiss(env: &mut Env, id: i64) -> Res<Value> {
    take_missed(env, id)?;
    env.st.completions.insert(format!("{id}|"), true);
    env.st.missed.remove(&id.to_string());
    env.changed = true;
    Ok(json!({"ok": true}))
}

/// Resolve occurrences for `n` days from `start` (no rollover, no workouts).
pub fn resolve_items(st: &State, start: NaiveDate, n: i64) -> Vec<(NaiveDate, Vec<Occurrence>)> {
    let mut out = vec![];
    for i in 0..n {
        let d = start + Duration::days(i);
        let ds = fmt_date(d);
        let mut items = vec![];
        for t in &st.tasks {
            let (matches, key) = match &t.recurrence {
                Some(r) => (r.matches(d), ds.clone()),
                None => (t.date.as_deref() == Some(ds.as_str()), String::new()),
            };
            if !matches {
                continue;
            }
            let tid = t.id;
            items.push(Occurrence {
                task_id: tid,
                date: ds.clone(),
                title: t.title.clone(),
                time: t.time.clone(),
                part_of_day: match &t.time {
                    Some(tm) => derive_part(tm).to_string(),
                    None => t.part_of_day.clone(),
                },
                category: t.category_id.and_then(|c| st.categories.iter().find(|x| x.id == c).cloned()),
                notes: t.notes.clone(),
                minutes: t.minutes,
                recurring: t.recurrence.is_some(),
                rolled_over_from: if t.recurrence.is_none() { st.rolled_over.get(&tid.to_string()).cloned() } else { None },
                done: st.completions.contains_key(&format!("{tid}|{key}")),
                checklist: t
                    .checklist
                    .iter()
                    .map(|c| OccChecklist { id: c.id, text: c.text.clone(), done: st.checklist_done.contains_key(&format!("{tid}|{key}|{}", c.id)) })
                    .collect(),
            });
        }
        items.sort_by(|a, b| match (&a.time, &b.time) {
            (Some(x), Some(y)) => x.cmp(y).then(a.task_id.cmp(&b.task_id)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => part_order(&a.part_of_day).cmp(&part_order(&b.part_of_day)).then(a.task_id.cmp(&b.task_id)),
        });
        out.push((d, items));
    }
    out
}

/// Full day views, including the rollover (when today is in range) and workout projection.
pub fn build_days(env: &mut Env, start: NaiveDate, n: i64) -> Vec<Day> {
    let t = env.today;
    if start <= t && t < start + Duration::days(n) && rollover(env.st, t) {
        env.changed = true;
    }
    let items = resolve_items(env.st, start, n);
    let workouts = gym::project(env.st, start, n, t);
    items
        .into_iter()
        .zip(workouts)
        .map(|((d, items), workout)| {
            let info = day_info(env.st, env.ctx, d);
            Day { date: fmt_date(d), weekday: weekday_name(d), items, workout, work: info.work, holiday: info.holiday, day_off: info.day_off }
        })
        .collect()
}

pub fn days(env: &mut Env, q: &std::collections::HashMap<String, String>) -> Res<Value> {
    let start = match q.get("start") {
        Some(s) => parse_date(s).ok_or_else(|| ApiErr::bad("start must be YYYY-MM-DD"))?,
        None => env.today,
    };
    let n: i64 = match q.get("days") {
        Some(s) => s.parse().map_err(|_| ApiErr::bad("days must be an integer"))?,
        None => 7,
    };
    if !(1..=62).contains(&n) {
        return Err(ApiErr::bad("days must be between 1 and 62"));
    }
    Ok(json!(build_days(env, start, n)))
}

#[derive(Deserialize)]
pub struct ToggleIn {
    task_id: i64,
    date: String,
    done: bool,
}

#[derive(Deserialize)]
pub struct ChecklistToggleIn {
    task_id: i64,
    date: String,
    item_id: i64,
    done: bool,
}

/// Returns (task, storage key date, effective occurrence date).
fn locate(st: &State, task_id: i64, date: &str) -> Res<(Task, String, NaiveDate)> {
    let task = st.tasks.iter().find(|t| t.id == task_id).cloned().ok_or_else(|| ApiErr::not_found("task not found"))?;
    let d = parse_date(date).ok_or_else(|| ApiErr::bad("date must be YYYY-MM-DD"))?;
    match &task.recurrence {
        Some(r) => {
            if !r.matches(d) {
                return Err(ApiErr::bad("task does not occur on that date"));
            }
            Ok((task, fmt_date(d), d))
        }
        None => {
            // one-off: the task's own (possibly rolled-over) date is authoritative
            let own = task.date.as_deref().and_then(parse_date).unwrap_or(d);
            Ok((task, String::new(), own))
        }
    }
}

fn occurrence(st: &State, task_id: i64, d: NaiveDate) -> Res<Occurrence> {
    resolve_items(st, d, 1).into_iter().flat_map(|(_, v)| v).find(|o| o.task_id == task_id).ok_or_else(|| ApiErr::not_found("occurrence not found"))
}

#[derive(Deserialize)]
pub struct MoveIn {
    task_id: i64,
    date: String,
}

/// Move one unfinished one-off task to the day after its date. Repeating tasks come back on their own.
fn move_one(st: &mut State, task_id: i64) -> Res<NaiveDate> {
    let done = st.completions.contains_key(&format!("{task_id}|"));
    let task = st.tasks.iter_mut().find(|t| t.id == task_id).ok_or_else(|| ApiErr::not_found("task not found"))?;
    if task.recurrence.is_some() {
        return Err(ApiErr::bad("Repeating tasks come back on their own, so they can't be moved."));
    }
    if done {
        return Err(ApiErr::bad("That one's already done."));
    }
    let from = task.date.as_deref().and_then(parse_date).ok_or_else(|| ApiErr::bad("That task has no date."))?;
    let to = from + Duration::days(1);
    task.date = Some(fmt_date(to));
    let id = task_id.to_string();
    st.missed.remove(&id);
    st.rolled_over.remove(&id);
    Ok(to)
}

/// `POST /api/occurrences/move` {task_id, date}: the task moves to the next day.
pub fn move_next(env: &mut Env, b: MoveIn) -> Res<Occurrence> {
    locate(env.st, b.task_id, &b.date)?;
    let to = move_one(env.st, b.task_id)?;
    env.changed = true;
    occurrence(env.st, b.task_id, to)
}

/// `POST /api/days/{date}/move-unfinished`: every unfinished one-off task on that day moves to the next day.
pub fn move_unfinished(env: &mut Env, date: &str) -> Res<Value> {
    let d = parse_date(date).ok_or_else(|| ApiErr::bad("date must be YYYY-MM-DD"))?;
    let ds = fmt_date(d);
    let ids: Vec<i64> = env
        .st
        .tasks
        .iter()
        .filter(|t| t.recurrence.is_none() && t.date.as_deref() == Some(ds.as_str()) && !env.st.completions.contains_key(&format!("{}|", t.id)))
        .map(|t| t.id)
        .collect();
    for id in &ids {
        move_one(env.st, *id)?;
    }
    if !ids.is_empty() {
        env.changed = true;
    }
    Ok(json!({"moved": ids.len(), "to": fmt_date(d + Duration::days(1))}))
}

pub fn toggle(env: &mut Env, b: ToggleIn) -> Res<Occurrence> {
    let (_, key, d) = locate(env.st, b.task_id, &b.date)?;
    let k = format!("{}|{key}", b.task_id);
    if b.done {
        env.st.completions.insert(k, true);
    } else {
        env.st.completions.remove(&k);
    }
    env.changed = true;
    occurrence(env.st, b.task_id, d)
}

pub fn toggle_checklist(env: &mut Env, b: ChecklistToggleIn) -> Res<Occurrence> {
    let (task, key, d) = locate(env.st, b.task_id, &b.date)?;
    if !task.checklist.iter().any(|c| c.id == b.item_id) {
        return Err(ApiErr::not_found("checklist item not found"));
    }
    let k = format!("{}|{key}|{}", b.task_id, b.item_id);
    if b.done {
        env.st.checklist_done.insert(k, true);
    } else {
        env.st.checklist_done.remove(&k);
    }
    env.changed = true;
    occurrence(env.st, b.task_id, d)
}
