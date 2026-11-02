use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, AppJson, Res};
use crate::{gym, AppState};
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Datelike, Duration, NaiveDate, NaiveTime};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------- categories

#[derive(Serialize, Clone, Debug)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Deserialize)]
pub struct CategoryIn {
    name: String,
    color: String,
}

fn load_categories(conn: &Connection) -> rusqlite::Result<Vec<Category>> {
    let mut st = conn.prepare("SELECT id,name,color FROM categories ORDER BY id")?;
    let rows = st.query_map([], |r| Ok(Category { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? }))?;
    rows.collect()
}

fn get_category(conn: &Connection, id: i64) -> rusqlite::Result<Category> {
    conn.query_row("SELECT id,name,color FROM categories WHERE id=?1", params![id], |r| {
        Ok(Category { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? })
    })
}

fn check_category(c: &CategoryIn) -> Res<()> {
    if c.name.trim().is_empty() {
        return Err(AppError::bad("category name cannot be empty"));
    }
    if c.color.trim().is_empty() {
        return Err(AppError::bad("category color cannot be empty"));
    }
    Ok(())
}

pub async fn list_categories(State(st): State<AppState>) -> Res<Json<Vec<Category>>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(load_categories(&conn)?))
}

pub async fn create_category(State(st): State<AppState>, AppJson(b): AppJson<CategoryIn>) -> Res<Json<Category>> {
    check_category(&b)?;
    let conn = st.db.lock().unwrap();
    conn.execute("INSERT INTO categories(name,color) VALUES(?1,?2)", params![b.name.trim(), b.color.trim()])?;
    Ok(Json(get_category(&conn, conn.last_insert_rowid())?))
}

pub async fn update_category(State(st): State<AppState>, Path(id): Path<i64>, AppJson(b): AppJson<CategoryIn>) -> Res<Json<Category>> {
    check_category(&b)?;
    let conn = st.db.lock().unwrap();
    let n = conn.execute("UPDATE categories SET name=?1,color=?2 WHERE id=?3", params![b.name.trim(), b.color.trim(), id])?;
    if n == 0 {
        return Err(AppError::not_found("category not found"));
    }
    Ok(Json(get_category(&conn, id)?))
}

pub async fn delete_category(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<serde_json::Value>> {
    let conn = st.db.lock().unwrap();
    let n = conn.execute("DELETE FROM categories WHERE id=?1", params![id])?;
    if n == 0 {
        return Err(AppError::not_found("category not found"));
    }
    conn.execute("UPDATE tasks SET category_id=NULL WHERE category_id=?1", params![id])?;
    Ok(Json(serde_json::json!({"ok": true})))
}

// --------------------------------------------------------------------- tasks

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Recurrence {
    Daily,
    Weekdays,
    Weekly { days: Vec<u8> },
}

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

#[derive(Serialize, Clone, Debug)]
pub struct ChecklistItem {
    pub id: i64,
    pub text: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub date: Option<String>,
    pub time: Option<String>,
    pub part_of_day: String,
    pub category_id: Option<i64>,
    pub category_auto: bool,
    pub notes: String,
    pub recurrence: Option<Recurrence>,
    pub checklist: Vec<ChecklistItem>,
    #[serde(skip)]
    pub rolled_from: Option<String>,
}

#[derive(Deserialize)]
pub struct ChecklistIn {
    id: Option<i64>,
    text: String,
}

#[derive(Deserialize)]
pub struct TaskIn {
    title: String,
    date: Option<String>,
    time: Option<String>,
    part_of_day: Option<String>,
    #[serde(default)]
    category_id: Option<serde_json::Value>,
    #[serde(default)]
    notes: String,
    recurrence: Option<Recurrence>,
    #[serde(default)]
    checklist: Vec<ChecklistIn>,
}

const PARTS: [&str; 4] = ["morning", "afternoon", "evening", "anytime"];

pub fn part_order(p: &str) -> usize {
    PARTS.iter().position(|x| *x == p).unwrap_or(3)
}

fn derive_part(time: &str) -> &'static str {
    let h: u32 = time[..2].parse().unwrap_or(0);
    if h < 12 {
        "morning"
    } else if h < 17 {
        "afternoon"
    } else {
        "evening"
    }
}

fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let time: Option<String> = r.get(3)?;
    let part: Option<String> = r.get(4)?;
    let rec: Option<String> = r.get(7)?;
    let part_of_day = match &time {
        Some(t) => derive_part(t).to_string(),
        None => part.filter(|p| PARTS.contains(&p.as_str())).unwrap_or_else(|| "anytime".into()),
    };
    Ok(Task {
        id: r.get(0)?,
        title: r.get(1)?,
        date: r.get(2)?,
        time,
        part_of_day,
        category_id: r.get(5)?,
        notes: r.get(6)?,
        recurrence: rec.and_then(|s| serde_json::from_str(&s).ok()),
        checklist: vec![],
        rolled_from: r.get(8)?,
        category_auto: r.get::<_, i64>(9)? != 0,
    })
}

const TASK_COLS: &str = "id,title,date,time,part_of_day,category_id,notes,recurrence,rolled_from,category_auto";

fn load_checklists(conn: &Connection) -> rusqlite::Result<HashMap<i64, Vec<ChecklistItem>>> {
    let mut st = conn.prepare("SELECT id,task_id,text FROM checklist_items ORDER BY task_id,pos,id")?;
    let mut map: HashMap<i64, Vec<ChecklistItem>> = HashMap::new();
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(1)?, ChecklistItem { id: r.get(0)?, text: r.get(2)? })))?;
    for row in rows {
        let (tid, item) = row?;
        map.entry(tid).or_default().push(item);
    }
    Ok(map)
}

pub fn load_tasks(conn: &Connection) -> rusqlite::Result<Vec<Task>> {
    let mut checklists = load_checklists(conn)?;
    let mut st = conn.prepare(&format!("SELECT {TASK_COLS} FROM tasks ORDER BY id"))?;
    let rows = st.query_map([], row_to_task)?;
    let mut out = vec![];
    for t in rows {
        let mut t = t?;
        t.checklist = checklists.remove(&t.id).unwrap_or_default();
        out.push(t);
    }
    Ok(out)
}

fn get_task(conn: &Connection, id: i64) -> rusqlite::Result<Task> {
    let mut t = conn.query_row(&format!("SELECT {TASK_COLS} FROM tasks WHERE id=?1"), params![id], row_to_task)?;
    let mut st = conn.prepare("SELECT id,text FROM checklist_items WHERE task_id=?1 ORDER BY pos,id")?;
    t.checklist = st
        .query_map(params![id], |r| Ok(ChecklistItem { id: r.get(0)?, text: r.get(1)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(t)
}

/// Validated, normalised form of TaskIn.
struct Clean {
    title: String,
    date: Option<String>,
    time: Option<String>,
    part: Option<String>,
    notes: String,
    recurrence: Option<Recurrence>,
}

fn clean(_conn: &Connection, b: &TaskIn) -> Res<Clean> {
    let title = b.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::bad("title cannot be empty"));
    }
    let time = match b.time.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(t) => Some(
            NaiveTime::parse_from_str(t, "%H:%M")
                .map_err(|_| AppError::bad("time must be HH:MM (24h)"))?
                .format("%H:%M")
                .to_string(),
        ),
    };
    let part = match b.part_of_day.as_deref().map(str::trim) {
        None | Some("") => None,
        Some(p) if PARTS.contains(&p) => Some(p.to_string()),
        Some(_) => return Err(AppError::bad("part_of_day must be morning, afternoon, evening or anytime")),
    };
    let mut recurrence = b.recurrence.clone();
    if let Some(Recurrence::Weekly { days }) = &mut recurrence {
        if days.is_empty() || days.iter().any(|d| *d > 6) {
            return Err(AppError::bad("weekly recurrence needs days between 0 (Mon) and 6 (Sun)"));
        }
        days.sort_unstable();
        days.dedup();
    }
    let date = match (&recurrence, b.date.as_deref().map(str::trim)) {
        (Some(_), _) => None,
        (None, None) | (None, Some("")) => return Err(AppError::bad("one-off tasks need a date (or give a recurrence)")),
        (None, Some(d)) => Some(fmt_date(parse_date(d).ok_or_else(|| AppError::bad("date must be YYYY-MM-DD"))?)),
    };
    Ok(Clean { title, date, time, part, notes: b.notes.clone(), recurrence })
}

enum CatSpec {
    Manual(Option<i64>),
    Auto,
}

fn parse_cat_spec(conn: &Connection, v: &Option<serde_json::Value>) -> Res<CatSpec> {
    use serde_json::Value;
    match v {
        None | Some(Value::Null) => Ok(CatSpec::Manual(None)),
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("auto") => Ok(CatSpec::Auto),
        Some(Value::Number(n)) => {
            let cid = n.as_i64().ok_or_else(|| AppError::bad("category_id must be a number, null or \"auto\""))?;
            if get_category(conn, cid).is_err() {
                return Err(AppError::bad("category_id does not exist"));
            }
            Ok(CatSpec::Manual(Some(cid)))
        }
        Some(_) => Err(AppError::bad("category_id must be a number, null or \"auto\"")),
    }
}

// ------------------------------------------------------- auto-categorisation

const KEYWORDS: &[(&str, &[&str])] = &[
    ("Chores", &["laundry", "hoover", "vacuum", "clean", "cleaning", "dishes", "dishwasher", "bins", "bin", "tidy", "iron", "ironing", "wash", "mop", "dust", "sweep", "declutter", "fix", "repair", "tap"]),
    ("Exercise", &["gym", "run", "running", "jog", "walk", "yoga", "swim", "workout", "cycle", "cycling", "stretch", "push", "pull", "leg", "legs", "pilates", "hike", "exercise", "lift", "cardio"]),
    ("Study", &["study", "revise", "revision", "essay", "read chapter", "chapter", "lecture", "homework", "exam", "assignment", "course", "coursework", "revising", "dissertation", "thesis"]),
    ("Work", &["meeting", "stand-up", "standup", "report", "email work", "email", "emails", "client", "deadline", "presentation", "shift", "project", "invoice", "sprint", "colleague", "boss"]),
    ("Errands", &["shop", "shopping", "groceries", "grocery", "post office", "pick up", "collect", "buy", "return", "parcel", "pharmacy", "supermarket", "drop off"]),
    ("Appointments", &["dentist", "doctor", "gp", "appointment", "haircut", "vet", "optician", "checkup", "check-up", "physio", "hospital", "dermatologist"]),
    ("Social", &["call", "mum", "dad", "friend", "friends", "party", "dinner with", "drinks", "birthday", "visit", "coffee with", "lunch with", "date night", "catch up", "brunch"]),
    ("Self-care", &["rest", "nap", "bath", "skincare", "meditate", "meditation", "journal", "read", "relax", "therapy", "massage", "sleep", "breathe", "morning routine"]),
    ("Admin & Money", &["bill", "bills", "pay", "bank", "tax", "budget", "form", "renew", "insurance", "rent", "council", "mortgage", "invoice", "passport", "visa", "savings", "subscription"]),
];

fn words(s: &str) -> String {
    let w: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_lowercase())
        .collect();
    format!(" {} ", w.join(" "))
}

/// Keyword match against existing category names only. Best score wins, ties go to table order.
fn keyword_category(text: &str, cats: &[Category]) -> Option<i64> {
    let hay = words(text);
    let mut best: Option<(usize, i64)> = None;
    for (name, kws) in KEYWORDS {
        let Some(cat) = cats.iter().find(|c| c.name.trim().eq_ignore_ascii_case(name)) else { continue };
        let score = kws.iter().filter(|k| hay.contains(&words(k))).count();
        if score > 0 && best.map_or(true, |(s, _)| score > s) {
            best = Some((score, cat.id));
        }
    }
    best.map(|(_, id)| id)
}

fn other_category(conn: &Connection, cats: &[Category]) -> rusqlite::Result<i64> {
    if let Some(c) = cats.iter().find(|c| c.name.trim().eq_ignore_ascii_case("other")) {
        return Ok(c.id);
    }
    conn.execute("INSERT INTO categories(name,color) VALUES('Other','#8C847D')", [])?;
    Ok(conn.last_insert_rowid())
}

async fn gemini_category(st: &AppState, key: &str, title: &str, notes: &str, cats: &[Category]) -> Option<i64> {
    let names: Vec<&str> = cats.iter().map(|c| c.name.as_str()).collect();
    let system = "You categorise to-do tasks. Reply with exactly one category name from the given list, copied exactly, and nothing else.";
    let user = format!("Task title: {title}\nNotes: {}\nCategories: {}\nWhich one category fits best?", if notes.trim().is_empty() { "(none)" } else { notes.trim() }, names.join(", "));
    match crate::briefing::gemini_raw(st, key, system, &user, 0.0, 1).await {
        Ok(reply) => {
            let r = reply.trim().trim_matches(|c: char| c == '"' || c == '\'' || c == '.' || c == '*' || c.is_whitespace());
            let hit = cats.iter().find(|c| c.name.trim().eq_ignore_ascii_case(r)).map(|c| c.id);
            if hit.is_none() {
                tracing::warn!("gemini category reply {r:?} not in list, using keywords");
            }
            hit
        }
        Err(e) => {
            tracing::warn!("gemini categorisation failed, using keywords: {e}");
            None
        }
    }
}

/// Resolve the category for a task body. Never fails because of Gemini.
async fn resolve_category(st: &AppState, spec: CatSpec, title: &str, notes: &str) -> Res<(Option<i64>, bool)> {
    if let CatSpec::Manual(id) = spec {
        return Ok((id, false));
    }
    let (cats, key) = {
        let conn = st.db.lock().unwrap();
        (load_categories(&conn)?, crate::settings::gemini_key(&conn))
    };
    let mut chosen = None;
    if let Some(k) = key {
        if !cats.is_empty() {
            chosen = gemini_category(st, &k, title, notes, &cats).await;
        }
    }
    if chosen.is_none() {
        chosen = keyword_category(title, &cats).or_else(|| keyword_category(&format!("{title} {notes}"), &cats));
    }
    let id = match chosen {
        Some(id) => id,
        None => {
            let conn = st.db.lock().unwrap();
            // re-read in case categories changed while we were waiting
            let cats = load_categories(&conn)?;
            other_category(&conn, &cats)?
        }
    };
    Ok((Some(id), true))
}

fn sync_checklist(conn: &Connection, task_id: i64, items: &[ChecklistIn]) -> rusqlite::Result<()> {
    let existing: HashSet<i64> = {
        let mut st = conn.prepare("SELECT id FROM checklist_items WHERE task_id=?1")?;
        st.query_map(params![task_id], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
    };
    let mut keep = HashSet::new();
    for (pos, it) in items.iter().filter(|i| !i.text.trim().is_empty()).enumerate() {
        match it.id.filter(|i| existing.contains(i)) {
            Some(id) => {
                conn.execute("UPDATE checklist_items SET text=?1,pos=?2 WHERE id=?3", params![it.text.trim(), pos as i64, id])?;
                keep.insert(id);
            }
            None => {
                conn.execute(
                    "INSERT INTO checklist_items(task_id,text,pos) VALUES(?1,?2,?3)",
                    params![task_id, it.text.trim(), pos as i64],
                )?;
                keep.insert(conn.last_insert_rowid());
            }
        }
    }
    for id in existing.difference(&keep) {
        conn.execute("DELETE FROM checklist_items WHERE id=?1", params![id])?;
        conn.execute("DELETE FROM checklist_done WHERE task_id=?1 AND item_id=?2", params![task_id, id])?;
    }
    Ok(())
}

pub async fn list_tasks(State(st): State<AppState>) -> Res<Json<Vec<Task>>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(load_tasks(&conn)?))
}

pub async fn get_task_h(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<Task>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(get_task(&conn, id).map_err(|_| AppError::not_found("task not found"))?))
}

pub async fn create_task(State(st): State<AppState>, AppJson(b): AppJson<TaskIn>) -> Res<Json<Task>> {
    let (c, spec) = {
        let conn = st.db.lock().unwrap();
        (clean(&conn, &b)?, parse_cat_spec(&conn, &b.category_id)?)
    };
    let (category_id, auto) = resolve_category(&st, spec, &c.title, &c.notes).await?;
    let conn = st.db.lock().unwrap();
    let rec = c.recurrence.as_ref().map(|r| serde_json::to_string(r).unwrap());
    conn.execute(
        "INSERT INTO tasks(title,date,time,part_of_day,category_id,notes,recurrence,category_auto) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![c.title, c.date, c.time, c.part, category_id, c.notes, rec, auto],
    )?;
    let id = conn.last_insert_rowid();
    sync_checklist(&conn, id, &b.checklist)?;
    Ok(Json(get_task(&conn, id)?))
}

pub async fn update_task(State(st): State<AppState>, Path(id): Path<i64>, AppJson(b): AppJson<TaskIn>) -> Res<Json<Task>> {
    let (old, c, spec) = {
        let conn = st.db.lock().unwrap();
        let old = get_task(&conn, id).map_err(|_| AppError::not_found("task not found"))?;
        (old, clean(&conn, &b)?, parse_cat_spec(&conn, &b.category_id)?)
    };
    let (category_id, auto) = resolve_category(&st, spec, &c.title, &c.notes).await?;
    let conn = st.db.lock().unwrap();
    if get_task(&conn, id).is_err() {
        return Err(AppError::not_found("task not found"));
    }
    let rec = c.recurrence.as_ref().map(|r| serde_json::to_string(r).unwrap());
    // keep the rollover marker only while the date is unchanged
    let rolled = if c.date == old.date { old.rolled_from } else { None };
    conn.execute(
        "UPDATE tasks SET title=?1,date=?2,time=?3,part_of_day=?4,category_id=?5,notes=?6,recurrence=?7,rolled_from=?8,category_auto=?9 WHERE id=?10",
        params![c.title, c.date, c.time, c.part, category_id, c.notes, rec, rolled, auto, id],
    )?;
    // switching between one-off and recurring invalidates old done state
    if old.recurrence.is_some() != c.recurrence.is_some() {
        conn.execute("DELETE FROM completions WHERE task_id=?1", params![id])?;
        conn.execute("DELETE FROM checklist_done WHERE task_id=?1", params![id])?;
    }
    sync_checklist(&conn, id, &b.checklist)?;
    Ok(Json(get_task(&conn, id)?))
}

pub async fn delete_task(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<serde_json::Value>> {
    let conn = st.db.lock().unwrap();
    let n = conn.execute("DELETE FROM tasks WHERE id=?1", params![id])?;
    if n == 0 {
        return Err(AppError::not_found("task not found"));
    }
    for sql in [
        "DELETE FROM checklist_items WHERE task_id=?1",
        "DELETE FROM completions WHERE task_id=?1",
        "DELETE FROM checklist_done WHERE task_id=?1",
    ] {
        conn.execute(sql, params![id])?;
    }
    Ok(Json(serde_json::json!({"ok": true})))
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
    pub recurring: bool,
    pub rolled_over_from: Option<String>,
    pub done: bool,
    pub checklist: Vec<OccChecklist>,
}

#[derive(Serialize, Clone)]
pub struct Day {
    pub date: String,
    pub weekday: String,
    pub items: Vec<Occurrence>,
    pub workout: Option<gym::WorkoutPlan>,
}

/// Move unfinished one-off tasks dated before `today` to `today`.
pub fn rollover(conn: &Connection, today: NaiveDate) -> rusqlite::Result<()> {
    let t = fmt_date(today);
    conn.execute(
        "UPDATE tasks SET rolled_from=COALESCE(rolled_from,date), date=?1
         WHERE recurrence IS NULL AND date IS NOT NULL AND date < ?1
           AND NOT EXISTS (SELECT 1 FROM completions c WHERE c.task_id=tasks.id AND c.date='')",
        params![t],
    )?;
    Ok(())
}

/// Resolve occurrences for `n` days from `start` (no rollover, no workouts).
pub fn resolve_items(conn: &Connection, start: NaiveDate, n: i64) -> rusqlite::Result<Vec<(NaiveDate, Vec<Occurrence>)>> {
    let tasks = load_tasks(conn)?;
    let cats: HashMap<i64, Category> = load_categories(conn)?.into_iter().map(|c| (c.id, c)).collect();
    let end = start + Duration::days(n - 1);
    let (a, b) = (fmt_date(start), fmt_date(end));
    let done: HashSet<(i64, String)> = {
        let mut st = conn.prepare("SELECT task_id,date FROM completions WHERE date='' OR (date>=?1 AND date<=?2)")?;
        st.query_map(params![a, b], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?
    };
    let cdone: HashSet<(i64, String, i64)> = {
        let mut st = conn.prepare("SELECT task_id,date,item_id FROM checklist_done WHERE date='' OR (date>=?1 AND date<=?2)")?;
        st.query_map(params![a, b], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<rusqlite::Result<_>>()?
    };
    let mut out = vec![];
    for i in 0..n {
        let d = start + Duration::days(i);
        let ds = fmt_date(d);
        let mut items = vec![];
        for t in &tasks {
            let (matches, key) = match &t.recurrence {
                Some(r) => (r.matches(d), ds.clone()),
                None => (t.date.as_deref() == Some(ds.as_str()), String::new()),
            };
            if !matches {
                continue;
            }
            items.push(Occurrence {
                task_id: t.id,
                date: ds.clone(),
                title: t.title.clone(),
                time: t.time.clone(),
                part_of_day: t.part_of_day.clone(),
                category: t.category_id.and_then(|c| cats.get(&c).cloned()),
                notes: t.notes.clone(),
                recurring: t.recurrence.is_some(),
                rolled_over_from: if t.recurrence.is_none() { t.rolled_from.clone() } else { None },
                done: done.contains(&(t.id, key.clone())),
                checklist: t
                    .checklist
                    .iter()
                    .map(|c| OccChecklist { id: c.id, text: c.text.clone(), done: cdone.contains(&(t.id, key.clone(), c.id)) })
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
    Ok(out)
}

pub fn weekday_name(d: NaiveDate) -> String {
    d.format("%A").to_string()
}

/// Full day views, including the rollover (when today is in range) and workout projection.
pub fn build_days(conn: &Connection, start: NaiveDate, n: i64) -> rusqlite::Result<Vec<Day>> {
    let t = today();
    if start <= t && t < start + Duration::days(n) {
        rollover(conn, t)?;
    }
    let items = resolve_items(conn, start, n)?;
    let workouts = gym::project(conn, start, n, t)?;
    Ok(items
        .into_iter()
        .zip(workouts)
        .map(|((d, items), workout)| Day { date: fmt_date(d), weekday: weekday_name(d), items, workout })
        .collect())
}

pub async fn days(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Res<Json<Vec<Day>>> {
    let start = match q.get("start") {
        Some(s) => parse_date(s).ok_or_else(|| AppError::bad("start must be YYYY-MM-DD"))?,
        None => today(),
    };
    let n: i64 = match q.get("days") {
        Some(s) => s.parse().map_err(|_| AppError::bad("days must be an integer"))?,
        None => 7,
    };
    if !(1..=62).contains(&n) {
        return Err(AppError::bad("days must be between 1 and 62"));
    }
    let conn = st.db.lock().unwrap();
    Ok(Json(build_days(&conn, start, n)?))
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
fn locate(conn: &Connection, task_id: i64, date: &str) -> Res<(Task, String, NaiveDate)> {
    let task = get_task(conn, task_id).map_err(|_| AppError::not_found("task not found"))?;
    let d = parse_date(date).ok_or_else(|| AppError::bad("date must be YYYY-MM-DD"))?;
    match &task.recurrence {
        Some(r) => {
            if !r.matches(d) {
                return Err(AppError::bad("task does not occur on that date"));
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

fn occurrence(conn: &Connection, task_id: i64, d: NaiveDate) -> Res<Occurrence> {
    resolve_items(conn, d, 1)?
        .into_iter()
        .flat_map(|(_, v)| v)
        .find(|o| o.task_id == task_id)
        .ok_or_else(|| AppError::not_found("occurrence not found"))
}

pub async fn toggle(State(st): State<AppState>, AppJson(b): AppJson<ToggleIn>) -> Res<Json<Occurrence>> {
    let conn = st.db.lock().unwrap();
    let (_, key, d) = locate(&conn, b.task_id, &b.date)?;
    if b.done {
        conn.execute("INSERT OR IGNORE INTO completions(task_id,date) VALUES(?1,?2)", params![b.task_id, key])?;
    } else {
        conn.execute("DELETE FROM completions WHERE task_id=?1 AND date=?2", params![b.task_id, key])?;
    }
    Ok(Json(occurrence(&conn, b.task_id, d)?))
}

pub async fn toggle_checklist(State(st): State<AppState>, AppJson(b): AppJson<ChecklistToggleIn>) -> Res<Json<Occurrence>> {
    let conn = st.db.lock().unwrap();
    let (task, key, d) = locate(&conn, b.task_id, &b.date)?;
    if !task.checklist.iter().any(|c| c.id == b.item_id) {
        return Err(AppError::not_found("checklist item not found"));
    }
    if b.done {
        conn.execute(
            "INSERT OR IGNORE INTO checklist_done(task_id,date,item_id) VALUES(?1,?2,?3)",
            params![b.task_id, key, b.item_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM checklist_done WHERE task_id=?1 AND date=?2 AND item_id=?3",
            params![b.task_id, key, b.item_id],
        )?;
    }
    Ok(Json(occurrence(&conn, b.task_id, d)?))
}
