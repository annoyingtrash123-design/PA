//! Goals: medium/long-term context for the AI (no manual task linking), plus goal-linking helpers.
use crate::env::Env;
use crate::error::{ApiErr, Res};
use crate::model::Goal;
use crate::util::*;
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::{json, Value};

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

fn status_rank(s: &str) -> u8 {
    match s {
        "active" => 0,
        "achieved" => 1,
        _ => 2,
    }
}

/// All goals, active first, then by deadline (none last), created, id.
pub fn list_all(goals: &[Goal]) -> Vec<Goal> {
    let mut v = goals.to_vec();
    v.sort_by(|a, b| {
        status_rank(&a.status)
            .cmp(&status_rank(&b.status))
            .then(a.deadline.is_none().cmp(&b.deadline.is_none()))
            .then(a.deadline.cmp(&b.deadline))
            .then(a.created.cmp(&b.created))
            .then(a.id.cmp(&b.id))
    });
    v
}

pub fn list_active(goals: &[Goal]) -> Vec<Goal> {
    list_all(goals).into_iter().filter(|g| g.status == "active").collect()
}

pub fn achieved_between(goals: &[Goal], from: NaiveDate, to: NaiveDate) -> Vec<Goal> {
    let (f, t) = (fmt_date(from), fmt_date(to));
    list_all(goals).into_iter().filter(|g| g.status == "achieved" && g.achieved_on.as_deref().is_some_and(|a| a >= f.as_str() && a <= t.as_str())).collect()
}

pub fn list(env: &Env) -> Value {
    json!(list_all(&env.st.goals))
}

struct Clean {
    title: String,
    horizon: String,
    deadline: Option<String>,
    why: String,
    status: String,
    achieved_on: Option<String>,
}

fn clean(b: GoalIn, today: NaiveDate) -> Res<Clean> {
    let title = b.title.trim().to_string();
    if title.is_empty() {
        return Err(ApiErr::bad("title is required"));
    }
    if b.horizon != "medium" && b.horizon != "long" {
        return Err(ApiErr::bad("horizon must be medium or long"));
    }
    let status = b.status.unwrap_or_else(|| "active".into());
    if !["active", "achieved", "archived"].contains(&status.as_str()) {
        return Err(ApiErr::bad("status must be active, achieved or archived"));
    }
    let date_field = |v: Option<String>, name: &str| -> Res<Option<String>> {
        match v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
            None => Ok(None),
            Some(s) => parse_date(&s).map(|d| Some(fmt_date(d))).ok_or_else(|| ApiErr::bad(format!("{name} must be YYYY-MM-DD"))),
        }
    };
    let deadline = date_field(b.deadline, "deadline")?;
    let mut achieved_on = date_field(b.achieved_on, "achieved_on")?;
    if status == "achieved" {
        achieved_on.get_or_insert_with(|| fmt_date(today));
    } else {
        achieved_on = None;
    }
    Ok(Clean { title, horizon: b.horizon, deadline, why: b.why.unwrap_or_default().trim().to_string(), status, achieved_on })
}

pub fn create(env: &mut Env, b: GoalIn) -> Res<Goal> {
    let c = clean(b, env.today)?;
    let g = Goal { id: env.st.new_id(), title: c.title, horizon: c.horizon, deadline: c.deadline, why: c.why, status: c.status, achieved_on: c.achieved_on, created: fmt_date(env.today) };
    env.st.goals.push(g.clone());
    env.changed = true;
    Ok(g)
}

pub fn update(env: &mut Env, id: i64, b: GoalIn) -> Res<Goal> {
    let c = clean(b, env.today)?;
    let today = fmt_date(env.today);
    let g = env.st.goals.iter_mut().find(|g| g.id == id).ok_or_else(|| ApiErr::not_found("goal not found"))?;
    // keep the original achieved date when it was already achieved and none was sent
    let achieved_on = if c.status == "achieved" && g.status == "achieved" && c.achieved_on.as_deref() == Some(today.as_str()) {
        g.achieved_on.clone().or(c.achieved_on)
    } else {
        c.achieved_on
    };
    g.title = c.title;
    g.horizon = c.horizon;
    g.deadline = c.deadline;
    g.why = c.why;
    g.status = c.status;
    g.achieved_on = achieved_on;
    let out = g.clone();
    env.changed = true;
    Ok(out)
}

pub fn delete(env: &mut Env, id: i64) -> Res<Value> {
    let n = env.st.goals.len();
    env.st.goals.retain(|g| g.id != id);
    if env.st.goals.len() == n {
        return Err(ApiErr::not_found("goal not found"));
    }
    env.changed = true;
    Ok(json!({"ok": true}))
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

/// Stable text used in cache keys.
pub fn fingerprint(active: &[Goal], date: NaiveDate) -> String {
    active
        .iter()
        .map(|g| format!("{}|{}|{}|{:?}", g.title, g.why, g.deadline.as_deref().unwrap_or(""), days_to(g, date).map(|n| n.clamp(-1, 14))))
        .collect::<Vec<_>>()
        .join(";")
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
    if THEMES.iter().any(|(gk, tk)| has(&gt, gk) && has(&tt, tk)) {
        1
    } else {
        0
    }
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
