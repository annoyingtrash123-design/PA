//! Goals: medium/long-term context for the AI (no manual task linking).
use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, AppJson, Res};
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

pub const CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS goals (
  id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, horizon TEXT NOT NULL,
  deadline TEXT, why TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'active',
  achieved_on TEXT, created TEXT NOT NULL);";

#[derive(Serialize, Clone, Debug)]
pub struct Goal {
    pub id: i64,
    pub title: String,
    pub horizon: String,
    pub deadline: Option<String>,
    pub why: String,
    pub status: String,
    pub achieved_on: Option<String>,
    pub created: String,
}

#[derive(Deserialize)]
pub struct GoalIn {
    pub title: String,
    pub horizon: String,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub why: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub achieved_on: Option<String>,
}

const COLS: &str = "id,title,horizon,deadline,why,status,achieved_on,created";
const ORDER: &str = "ORDER BY CASE status WHEN 'active' THEN 0 WHEN 'achieved' THEN 1 ELSE 2 END,
  deadline IS NULL, deadline, created, id";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Goal> {
    Ok(Goal {
        id: r.get(0)?,
        title: r.get(1)?,
        horizon: r.get(2)?,
        deadline: r.get(3)?,
        why: r.get(4)?,
        status: r.get(5)?,
        achieved_on: r.get(6)?,
        created: r.get(7)?,
    })
}

pub fn list_all(conn: &Connection) -> rusqlite::Result<Vec<Goal>> {
    let mut s = conn.prepare(&format!("SELECT {COLS} FROM goals {ORDER}"))?;
    let v = s.query_map([], row)?.collect();
    v
}

pub fn list_active(conn: &Connection) -> rusqlite::Result<Vec<Goal>> {
    Ok(list_all(conn)?.into_iter().filter(|g| g.status == "active").collect())
}

pub fn achieved_between(conn: &Connection, from: NaiveDate, to: NaiveDate) -> rusqlite::Result<Vec<Goal>> {
    let (f, t) = (fmt_date(from), fmt_date(to));
    Ok(list_all(conn)?
        .into_iter()
        .filter(|g| g.status == "achieved" && g.achieved_on.as_deref().is_some_and(|a| a >= f.as_str() && a <= t.as_str()))
        .collect())
}

fn get(conn: &Connection, id: i64) -> Res<Goal> {
    conn.query_row(&format!("SELECT {COLS} FROM goals WHERE id=?1"), params![id], row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => AppError::not_found("goal not found"),
            o => o.into(),
        })
}

struct Clean {
    title: String,
    horizon: String,
    deadline: Option<String>,
    why: String,
    status: String,
    achieved_on: Option<String>,
}

fn clean(b: GoalIn) -> Res<Clean> {
    let title = b.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::bad("title is required"));
    }
    if b.horizon != "medium" && b.horizon != "long" {
        return Err(AppError::bad("horizon must be medium or long"));
    }
    let status = b.status.unwrap_or_else(|| "active".into());
    if !["active", "achieved", "archived"].contains(&status.as_str()) {
        return Err(AppError::bad("status must be active, achieved or archived"));
    }
    let date_field = |v: Option<String>, name: &str| -> Res<Option<String>> {
        match v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(s) => parse_date(&s).map(|d| Some(fmt_date(d))).ok_or_else(|| AppError::bad(format!("{name} must be YYYY-MM-DD"))),
        }
    };
    let deadline = date_field(b.deadline, "deadline")?;
    let mut achieved_on = date_field(b.achieved_on, "achieved_on")?;
    if status == "achieved" {
        achieved_on.get_or_insert_with(|| fmt_date(today()));
    } else {
        achieved_on = None;
    }
    Ok(Clean { title, horizon: b.horizon, deadline, why: b.why.unwrap_or_default().trim().to_string(), status, achieved_on })
}

pub async fn list(State(st): State<AppState>) -> Res<Json<Vec<Goal>>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(list_all(&conn)?))
}

pub async fn create(State(st): State<AppState>, AppJson(b): AppJson<GoalIn>) -> Res<Json<Goal>> {
    let c = clean(b)?;
    let conn = st.db.lock().unwrap();
    conn.execute(
        "INSERT INTO goals(title,horizon,deadline,why,status,achieved_on,created) VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![c.title, c.horizon, c.deadline, c.why, c.status, c.achieved_on, fmt_date(today())],
    )?;
    Ok(Json(get(&conn, conn.last_insert_rowid())?))
}

pub async fn update(State(st): State<AppState>, Path(id): Path<i64>, AppJson(b): AppJson<GoalIn>) -> Res<Json<Goal>> {
    let c = clean(b)?;
    let conn = st.db.lock().unwrap();
    let prev = get(&conn, id)?;
    // keep the original achieved date when it was already achieved and none was sent
    let achieved_on = if c.status == "achieved" && prev.status == "achieved" && c.achieved_on.as_deref() == Some(fmt_date(today()).as_str()) {
        prev.achieved_on.or(c.achieved_on)
    } else {
        c.achieved_on
    };
    conn.execute(
        "UPDATE goals SET title=?1,horizon=?2,deadline=?3,why=?4,status=?5,achieved_on=?6 WHERE id=?7",
        params![c.title, c.horizon, c.deadline, c.why, c.status, achieved_on, id],
    )?;
    Ok(Json(get(&conn, id)?))
}

pub async fn delete(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<serde_json::Value>> {
    let conn = st.db.lock().unwrap();
    let n = conn.execute("DELETE FROM goals WHERE id=?1", params![id])?;
    if n == 0 {
        return Err(AppError::not_found("goal not found"));
    }
    Ok(Json(serde_json::json!({"ok": true})))
}

// ------------------------------------------------------- AI context helpers

pub fn days_to(g: &Goal, from: NaiveDate) -> Option<i64> {
    g.deadline.as_deref().and_then(parse_date).map(|d| (d - from).num_days())
}

/// Natural, number-light way to say how far a deadline is.
pub fn away_phrase(days: i64) -> &'static str {
    match days {
        i64::MIN..=-1 => "already past",
        0 => "today",
        1 => "tomorrow",
        2..=4 => "just a few days away",
        5..=9 => "about a week away",
        10..=17 => "a couple of weeks away",
        18..=45 => "a few weeks away",
        46..=100 => "a couple of months away",
        _ => "a few months away",
    }
}

/// Lines describing goals for a Gemini context.
pub fn context_block(active: &[Goal], achieved: &[Goal], date: NaiveDate) -> String {
    let mut s = String::new();
    if active.is_empty() {
        s.push_str("Active goals: none set\n");
    } else {
        s.push_str("Active goals (only connect a task or workout to a goal when it plausibly relates; do not force it):\n");
        for g in active {
            let dl = match (&g.deadline, days_to(g, date)) {
                (Some(d), Some(n)) => format!("deadline {d}, {}", away_phrase(n)),
                _ => "no deadline".into(),
            };
            let why = if g.why.is_empty() { String::new() } else { format!("; why: {}", g.why) };
            s.push_str(&format!("  - {} ({} term; {dl}{why})\n", g.title, g.horizon));
        }
    }
    if !achieved.is_empty() {
        s.push_str("Goals achieved this week (celebrate these):\n");
        for g in achieved {
            s.push_str(&format!("  - {}\n", g.title));
        }
    }
    s
}

/// Stable text used to invalidate cached day notes when goals change.
pub fn fingerprint(active: &[Goal], date: NaiveDate) -> String {
    active.iter().map(|g| format!("{}|{}|{}|{:?}", g.title, g.why, g.deadline.as_deref().unwrap_or(""), days_to(g, date).map(|n| n.clamp(-1, 14)))).collect::<Vec<_>>().join(";")
}

const STOP: [&str; 14] = ["with", "that", "this", "from", "have", "more", "your", "into", "just", "keep", "when", "over", "than", "them"];

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_string()).collect()
}

fn stem(w: &str) -> &str {
    w.strip_suffix('s').filter(|x| x.len() >= 4).unwrap_or(w)
}

/// (goal-side words, task-side words): a shared theme links a goal to a task. Words match with a
/// short inflection only ("workout" does not match "work").
const THEMES: [(&[&str], &[&str]); 5] = [
    (&["strong", "stronger", "strength", "bench", "squat", "deadlift", "lift", "fit", "fitter", "fitness", "gym", "muscle", "workout", "exercis", "health", "running", "marathon"],
     &["gym", "push", "pull", "legs", "bench", "press", "squat", "deadlift", "lift", "workout", "walk", "run", "exercis", "stretch", "row", "curl", "session"]),
    (&["study", "exam", "course", "learn", "degree", "certif", "revis"],
     &["study", "revis", "course", "lecture", "read", "essay", "exam", "learn", "homework"]),
    (&["work", "career", "promotion", "report", "project", "job", "admin", "inbox", "email", "reply"],
     &["report", "stand", "project", "email", "reply", "submit", "meeting", "invoice", "inbox"]),
    (&["routine", "organis", "organiz", "calm", "habit", "tidy", "chore", "loose", "ends"],
     &["routine", "plan", "laundry", "bill", "tap", "grocer", "chore", "dentist", "clean", "tidy", "pay", "book"]),
    (&["people", "family", "friend", "social", "mum", "visit", "relationship", "connect"],
     &["call", "mum", "coffee", "alex", "dinner", "friend", "family", "visit", "lunch"]),
];

fn word_match(w: &str, k: &str) -> bool {
    w.starts_with(k) && w.len() <= k.len() + 2
}

/// 0 = unrelated, 1 = shared theme, 2 = a meaningful word in common.
fn score(g: &Goal, text: &str) -> u8 {
    let gt = tokens(&format!("{} {}", g.title, g.why));
    let tt = tokens(text);
    for a in &gt {
        if a.len() >= 5 && !STOP.contains(&a.as_str()) && tt.iter().any(|b| b.len() >= 5 && stem(a) == stem(b)) {
            return 2;
        }
    }
    let has = |ws: &[String], keys: &[&str]| ws.iter().any(|w| keys.iter().any(|k| word_match(w, k)));
    if THEMES.iter().any(|(gk, tk)| has(&gt, gk) && has(&tt, tk)) { 1 } else { 0 }
}

/// Best goal link among `(label, match_text)` pairs. Items are tried in the order given (callers put the
/// most interesting first); within an item the strongest-scoring goal wins, earlier goals break ties.
/// Returns the goal and the label of the thing that links to it.
pub fn find_link<'a>(goals: &'a [Goal], items: &[(String, String)]) -> Option<(&'a Goal, String)> {
    for (label, text) in items {
        let mut best: Option<(u8, &Goal)> = None;
        for g in goals {
            let sc = score(g, text);
            if sc > 0 && best.is_none_or(|(b, _)| sc > b) {
                best = Some((sc, g));
            }
        }
        if let Some((_, g)) = best {
            return Some((g, label.clone()));
        }
    }
    None
}

/// The goal title up to a colon, e.g. "Get stronger" for "Get stronger: bench 80 kg by spring".
pub fn short_title(g: &Goal) -> &str {
    g.title.split(':').next().unwrap_or(&g.title).trim()
}

/// "your goal, Get stronger" - safe to end a sentence with.
pub fn goal_ref(g: &Goal) -> String {
    format!("your goal, {}", short_title(g))
}
