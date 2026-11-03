//! Gym: exercises, templates, rotation, workout projection, progression, sessions, history.
use crate::env::Env;
use crate::error::{ApiErr, Res};
use crate::model::*;
use crate::util::*;
use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ----------------------------------------------------------------- exercises

#[derive(Deserialize)]
pub struct ExerciseIn {
    name: String,
    #[serde(default)]
    assisted: Option<bool>,
}

fn exercise_name_ok(name: &str) -> Res<&str> {
    let n = name.trim();
    if n.is_empty() {
        return Err(ApiErr::bad("exercise name cannot be empty"));
    }
    Ok(n)
}

pub fn list_exercises(env: &Env) -> Value {
    let mut v = env.st.gym.exercises.clone();
    v.sort_by_key(|e| e.name.to_lowercase());
    json!(v)
}

fn name_taken(st: &State, name: &str, except: Option<i64>) -> bool {
    st.gym.exercises.iter().any(|e| Some(e.id) != except && e.name.eq_ignore_ascii_case(name))
}

pub fn create_exercise(env: &mut Env, b: ExerciseIn) -> Res<Exercise> {
    let name = exercise_name_ok(&b.name)?.to_string();
    if name_taken(env.st, &name, None) {
        return Err(ApiErr::bad("an exercise with that name already exists"));
    }
    let e = Exercise { id: env.st.new_id(), name, assisted: b.assisted.unwrap_or(false) };
    env.st.gym.exercises.push(e.clone());
    env.changed = true;
    Ok(e)
}

pub fn update_exercise(env: &mut Env, id: i64, b: ExerciseIn) -> Res<Exercise> {
    let name = exercise_name_ok(&b.name)?.to_string();
    if !env.st.gym.exercises.iter().any(|e| e.id == id) {
        return Err(ApiErr::not_found("exercise not found"));
    }
    if name_taken(env.st, &name, Some(id)) {
        return Err(ApiErr::bad("an exercise with that name already exists"));
    }
    let e = env.st.gym.exercises.iter_mut().find(|e| e.id == id).unwrap();
    e.name = name.clone();
    if let Some(a) = b.assisted {
        e.assisted = a;
    }
    let out = e.clone();
    for t in env.st.gym.templates.iter_mut() {
        for te in t.exercises.iter_mut().filter(|te| te.exercise_id == id) {
            te.name = name.clone();
        }
    }
    env.changed = true;
    Ok(out)
}

pub fn delete_exercise(env: &mut Env, id: i64) -> Res<Value> {
    let n = env.st.gym.exercises.len();
    env.st.gym.exercises.retain(|e| e.id != id);
    if env.st.gym.exercises.len() == n {
        return Err(ApiErr::not_found("exercise not found"));
    }
    for t in env.st.gym.templates.iter_mut() {
        t.exercises.retain(|te| te.exercise_id != id);
    }
    env.changed = true;
    Ok(json!({"ok": true}))
}

// ----------------------------------------------------------------- templates

#[derive(Deserialize)]
pub struct TemplateExerciseIn {
    #[serde(default)]
    exercise_id: Option<i64>,
    #[serde(default)]
    name: Option<String>,
    sets: i64,
    rep_min: i64,
    rep_max: i64,
    #[serde(default)]
    start_weight: Option<f64>,
    #[serde(default)]
    note: Option<String>,
    /// Also sets the (shared) exercise's assisted flag when given.
    #[serde(default)]
    assisted: Option<bool>,
}

#[derive(Deserialize)]
pub struct TemplateIn {
    name: String,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    exercises: Vec<TemplateExerciseIn>,
    #[serde(default)]
    options: Vec<String>,
    #[serde(default)]
    minutes_min: Option<i64>,
    #[serde(default)]
    minutes_max: Option<i64>,
}

fn find_or_create_exercise(st: &mut State, name: &str) -> i64 {
    if let Some(e) = st.gym.exercises.iter().find(|e| e.name.eq_ignore_ascii_case(name)) {
        return e.id;
    }
    let id = st.new_id();
    st.gym.exercises.push(Exercise { id, name: name.to_string(), assisted: false });
    id
}

fn build_template_exercises(st: &mut State, list: &[TemplateExerciseIn]) -> Res<Vec<TemplateExercise>> {
    // validate everything first so nothing is half-written
    for e in list {
        if e.sets < 1 || e.rep_min < 1 || e.rep_max < e.rep_min {
            return Err(ApiErr::bad("sets and reps must be positive and rep_min <= rep_max"));
        }
        if e.start_weight.is_some_and(|w| !w.is_finite() || w < 0.0) {
            return Err(ApiErr::bad("start_weight must be a non-negative number"));
        }
        match (e.exercise_id, e.name.as_deref().map(str::trim).filter(|n| !n.is_empty())) {
            (Some(id), _) => {
                if !st.gym.exercises.iter().any(|x| x.id == id) {
                    return Err(ApiErr::bad(format!("exercise {id} does not exist")));
                }
            }
            (None, Some(_)) => {}
            _ => return Err(ApiErr::bad("each template exercise needs exercise_id")),
        }
    }
    let mut out = vec![];
    for e in list {
        let id = match (e.exercise_id, e.name.as_deref().map(str::trim).filter(|n| !n.is_empty())) {
            (Some(id), _) => id,
            (None, Some(n)) => find_or_create_exercise(st, n),
            _ => unreachable!(),
        };
        if let Some(a) = e.assisted {
            if let Some(x) = st.gym.exercises.iter_mut().find(|x| x.id == id) {
                x.assisted = a;
            }
        }
        let name = st.gym.exercises.iter().find(|x| x.id == id).map(|x| x.name.clone()).unwrap_or_default();
        out.push(TemplateExercise {
            exercise_id: id,
            name,
            sets: e.sets,
            rep_min: e.rep_min,
            rep_max: e.rep_max,
            start_weight: e.start_weight,
            note: e.note.as_deref().unwrap_or("").trim().to_string(),
        });
    }
    Ok(out)
}

/// Validated fields shared by create and update (everything but the id).
fn build_template(st: &mut State, id: i64, b: &TemplateIn) -> Res<Template> {
    if b.name.trim().is_empty() {
        return Err(ApiErr::bad("template name cannot be empty"));
    }
    let kind = b.kind.as_deref().map(str::trim).filter(|k| !k.is_empty()).unwrap_or("strength").to_string();
    if kind != "strength" && kind != "cardio" {
        return Err(ApiErr::bad("kind must be strength or cardio"));
    }
    let note = b.note.as_deref().unwrap_or("").trim().to_string();
    if kind == "cardio" {
        let mut options: Vec<String> = vec![];
        for o in b.options.iter().map(|o| o.trim()).filter(|o| !o.is_empty()) {
            if !options.iter().any(|x| x.eq_ignore_ascii_case(o)) {
                options.push(o.to_string());
            }
        }
        if options.is_empty() {
            return Err(ApiErr::bad("a cardio template needs at least one option"));
        }
        let (lo, hi) = (b.minutes_min.unwrap_or(20), b.minutes_max.unwrap_or(40));
        if lo < 1 || hi < lo || hi > 600 {
            return Err(ApiErr::bad("minutes must be positive and minutes_min <= minutes_max"));
        }
        return Ok(Template { id, name: b.name.trim().to_string(), kind, note, exercises: vec![], options, minutes_min: Some(lo), minutes_max: Some(hi) });
    }
    let exercises = build_template_exercises(st, &b.exercises)?;
    Ok(Template { id, name: b.name.trim().to_string(), kind, note, exercises, options: vec![], minutes_min: None, minutes_max: None })
}

pub fn create_template(env: &mut Env, b: TemplateIn) -> Res<Template> {
    let id = env.st.next_id;
    let t = build_template(env.st, id, &b)?;
    env.st.new_id();
    env.st.gym.templates.push(t.clone());
    env.changed = true;
    Ok(t)
}

pub fn update_template(env: &mut Env, id: i64, b: TemplateIn) -> Res<Template> {
    if !env.st.gym.templates.iter().any(|t| t.id == id) {
        return Err(ApiErr::not_found("template not found"));
    }
    let nt = build_template(env.st, id, &b)?;
    let t = env.st.gym.templates.iter_mut().find(|t| t.id == id).unwrap();
    *t = nt.clone();
    env.changed = true;
    Ok(nt)
}

pub fn delete_template(env: &mut Env, id: i64) -> Res<Value> {
    let n = env.st.gym.templates.len();
    env.st.gym.templates.retain(|t| t.id != id);
    if env.st.gym.templates.len() == n {
        return Err(ApiErr::not_found("template not found"));
    }
    let rot = &mut env.st.gym.rotation;
    let before = rot.items.len();
    rot.items.retain(|i| i.template_id != Some(id));
    if rot.items.len() != before {
        rot.next_index = if rot.items.is_empty() { 0 } else { rot.next_index % rot.items.len() };
        env.st.gym.next_since = Some(fmt_date(env.today));
    }
    env.changed = true;
    Ok(json!({"ok": true}))
}

// ------------------------------------------------------------------ rotation

#[derive(Deserialize)]
pub struct RotationIn {
    items: Vec<RotItem>,
    #[serde(default)]
    next_index: Option<usize>,
}

pub fn get_rotation(st: &State) -> Rotation {
    let items = st.gym.rotation.items.clone();
    let next_index = if items.is_empty() { 0 } else { st.gym.rotation.next_index % items.len() };
    Rotation { items, next_index }
}

pub fn put_rotation(env: &mut Env, b: RotationIn) -> Res<Rotation> {
    for it in &b.items {
        match (it.rest, it.template_id) {
            (true, _) => {}
            (false, Some(t)) => {
                if !env.st.gym.templates.iter().any(|x| x.id == t) {
                    return Err(ApiErr::bad(format!("template {t} does not exist")));
                }
            }
            (false, None) => return Err(ApiErr::bad("each rotation item needs template_id or rest:true")),
        }
    }
    let next = b.next_index.unwrap_or(env.st.gym.rotation.next_index);
    let items: Vec<RotItem> = b.items.into_iter().map(|i| RotItem { template_id: if i.rest { None } else { i.template_id }, rest: i.rest }).collect();
    env.st.gym.rotation.next_index = if items.is_empty() { 0 } else { next % items.len() };
    env.st.gym.rotation.items = items;
    env.st.gym.next_since = Some(fmt_date(env.today));
    env.changed = true;
    Ok(get_rotation(env.st))
}

fn advance(st: &mut State, len: usize, to: usize, today: NaiveDate) {
    st.gym.rotation.next_index = (to + 1) % len;
    st.gym.next_since = Some(fmt_date(today));
}

/// Per-call housekeeping: days that pass while a rest item is next count as that rest day (one rest item
/// per elapsed day). Workouts always wait. Returns true when the state changed.
pub fn roll_missed_days(st: &mut State, today: NaiveDate) -> bool {
    let td = fmt_date(today);
    let len = st.gym.rotation.items.len();
    let since = match st.gym.next_since.as_deref().and_then(parse_date) {
        Some(s) => s,
        None => {
            st.gym.next_since = Some(td);
            return true;
        }
    };
    if len == 0 || since >= today {
        return false;
    }
    let mut days = (today - since).num_days();
    let mut taken = false;
    while days > 0 && st.gym.rotation.items[st.gym.rotation.next_index % len].rest {
        let i = st.gym.rotation.next_index % len;
        advance(st, len, i, today);
        taken = true;
        days -= 1;
    }
    if taken {
        // the missed day WAS the rest, so today goes straight to the next workout
        st.gym.rest_taken_date = today.pred_opt().map(fmt_date);
    }
    taken
}

pub fn rest_taken(env: &mut Env) -> Res<Rotation> {
    let rot = get_rotation(env.st);
    match rot.items.get(rot.next_index) {
        Some(it) if it.rest => {
            advance(env.st, rot.items.len(), rot.next_index, env.today);
            env.st.gym.rest_taken_date = Some(fmt_date(env.today));
            env.changed = true;
            Ok(get_rotation(env.st))
        }
        _ => Err(ApiErr::bad("the next rotation item is not a rest day")),
    }
}

/// "Train today instead": move past the next rest item without recording a rest taken today.
pub fn skip_rest(env: &mut Env) -> Res<Rotation> {
    let rot = get_rotation(env.st);
    match rot.items.get(rot.next_index) {
        Some(it) if it.rest => {
            advance(env.st, rot.items.len(), rot.next_index, env.today);
            if env.st.gym.rest_taken_date.as_deref() == Some(fmt_date(env.today).as_str()) {
                env.st.gym.rest_taken_date = None;
            }
            env.changed = true;
            Ok(get_rotation(env.st))
        }
        _ => Err(ApiErr::bad("the next rotation item is not a rest day")),
    }
}

/// Escape hatch: move past the next item (workout or rest) without logging anything.
pub fn skip_next(env: &mut Env) -> Res<Rotation> {
    let rot = get_rotation(env.st);
    if rot.items.is_empty() {
        return Err(ApiErr::bad("there is no rotation set up"));
    }
    advance(env.st, rot.items.len(), rot.next_index, env.today);
    env.changed = true;
    Ok(get_rotation(env.st))
}

// --------------------------------------------------------------------- plans

#[derive(Serialize, Clone, Debug)]
pub struct LastSet {
    pub weight: f64,
    pub reps: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Last {
    pub date: String,
    pub sets: Vec<LastSet>,
}

#[derive(Serialize, Clone, Debug)]
pub struct PlanExercise {
    pub exercise_id: i64,
    pub name: String,
    pub sets: i64,
    pub rep_min: i64,
    pub rep_max: i64,
    pub last: Option<Last>,
    pub suggested_weight: Option<f64>,
    pub hint: Option<String>,
    pub assisted: bool,
    pub start_weight: Option<f64>,
    pub note: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct WorkoutPlan {
    pub rest: bool,
    pub template_id: Option<i64>,
    pub name: String,
    pub kind: String,
    pub note: String,
    pub exercises: Vec<PlanExercise>,
    pub options: Vec<String>,
    pub minutes_min: Option<i64>,
    pub minutes_max: Option<i64>,
}

impl WorkoutPlan {
    pub fn is_cardio(&self) -> bool {
        self.kind == "cardio"
    }
}

pub fn fmt_w(w: f64) -> String {
    if (w - w.round()).abs() < 1e-9 {
        format!("{}", w.round() as i64)
    } else {
        format!("{:.1}", w)
    }
}

/// The most recent completed session (before `before`, if given) that has done sets for the exercise.
fn last_for(st: &State, exercise_id: i64, before: Option<&str>) -> Option<Last> {
    let s = st
        .gym
        .sessions
        .iter()
        .filter(|s| s.completed && before.is_none_or(|b| s.date.as_str() < b) && s.sets.iter().any(|x| x.exercise_id == exercise_id && x.done))
        .max_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)))?;
    let mut sets: Vec<&SetRec> = s.sets.iter().filter(|x| x.exercise_id == exercise_id && x.done).collect();
    sets.sort_by_key(|x| x.set_no);
    Some(Last { date: s.date.clone(), sets: sets.into_iter().map(|x| LastSet { weight: x.weight, reps: x.reps }).collect() })
}

/// Progression rule: all sets at the same weight W with reps >= rep_max -> W + 2.5, else W.
/// Assisted (counterweight) exercises go the other way: W - 2.5 (never below 0).
pub fn suggest(last: &Last, rep_max: i64, assisted: bool) -> (f64, String) {
    let w = last.sets.iter().map(|s| s.weight).fold(0.0, f64::max);
    let all_hit = !last.sets.is_empty() && last.sets.iter().all(|s| (s.weight - w).abs() < 1e-9 && s.reps >= rep_max);
    if assisted {
        if all_hit && w > 0.0 {
            let down = (w - 2.5).max(0.0);
            return (down, "You hit every rep last time, try 2.5 kg less counterweight".to_string());
        }
        return (w, format!("Stay at {} kg counterweight and aim for {} reps on every set", fmt_w(w), rep_max));
    }
    if all_hit {
        let up = w + 2.5;
        (up, format!("You hit {} reps on every set last time, try {} kg", rep_max, fmt_w(up)))
    } else {
        (w, format!("Stay at {} kg and aim for {} reps on every set", fmt_w(w), rep_max))
    }
}

pub fn rest_plan() -> WorkoutPlan {
    WorkoutPlan { rest: true, template_id: None, name: "Rest day".into(), kind: "strength".into(), note: String::new(), exercises: vec![], options: vec![], minutes_min: None, minutes_max: None }
}

pub fn plan_for_template(st: &State, tid: i64, before: Option<&str>) -> Option<WorkoutPlan> {
    let t = st.gym.templates.iter().find(|t| t.id == tid)?;
    let mut exercises = vec![];
    for e in &t.exercises {
        let last = last_for(st, e.exercise_id, before);
        let assisted = st.gym.exercises.iter().any(|x| x.id == e.exercise_id && x.assisted);
        let (sw, hint) = match (&last, e.start_weight) {
            (Some(l), _) => {
                let (w, h) = suggest(l, e.rep_max, assisted);
                (Some(w), Some(h))
            }
            (None, Some(w)) => (Some(w), Some(format!("Starting weight: {} kg", fmt_w(w)))),
            (None, None) => (None, None),
        };
        exercises.push(PlanExercise {
            exercise_id: e.exercise_id,
            name: e.name.clone(),
            sets: e.sets,
            rep_min: e.rep_min,
            rep_max: e.rep_max,
            last,
            suggested_weight: sw,
            hint,
            assisted,
            start_weight: e.start_weight,
            note: e.note.clone(),
        });
    }
    Some(WorkoutPlan {
        rest: false,
        template_id: Some(tid),
        name: t.name.clone(),
        kind: t.kind.clone(),
        note: t.note.clone(),
        exercises,
        options: t.options.clone(),
        minutes_min: t.minutes_min,
        minutes_max: t.minutes_max,
    })
}

fn plan_for_item(st: &State, it: &RotItem, before: Option<&str>) -> WorkoutPlan {
    match (it.rest, it.template_id) {
        (false, Some(t)) => plan_for_template(st, t, before).unwrap_or_else(rest_plan),
        _ => rest_plan(),
    }
}

/// (id, template_id, name) of the latest completed session on a date.
pub fn completed_session_on(st: &State, d: NaiveDate) -> Option<(i64, Option<i64>, String)> {
    let ds = fmt_date(d);
    st.gym.sessions.iter().filter(|s| s.date == ds && s.completed).max_by_key(|s| s.id).map(|s| (s.id, s.template_id, s.name.clone()))
}

/// The cardio record of the latest completed session on a date, if it was a cardio one.
pub fn cardio_done_on(st: &State, d: NaiveDate) -> Option<Cardio> {
    let ds = fmt_date(d);
    st.gym.sessions.iter().filter(|s| s.date == ds && s.completed).max_by_key(|s| s.id).and_then(|s| s.cardio.clone())
}

/// A natural phrase for a cardio session: "a swim", "40 minutes on the rowing machine".
pub fn cardio_phrase(c: &Cardio) -> String {
    let a = c.activity.trim().to_lowercase();
    let m = c.minutes;
    if a.starts_with("swim") {
        "a swim".to_string()
    } else if a.contains("walk") || a.contains("hik") {
        format!("{m} minutes of intense walking")
    } else if a.contains("machine") {
        format!("{m} minutes on the {a}")
    } else if a.is_empty() {
        format!("{m} minutes of cardio")
    } else {
        format!("{m} minutes of {a}")
    }
}

/// Workout projection per day (None for past days or when no rotation is set up).
pub fn project(st: &State, start: NaiveDate, n: i64, today: NaiveDate) -> Vec<Option<WorkoutPlan>> {
    let rot = get_rotation(st);
    if rot.items.is_empty() {
        return (0..n).map(|_| None).collect();
    }
    let td = fmt_date(today);
    let fixed_today: Option<WorkoutPlan> = match completed_session_on(st, today) {
        Some((_, Some(tid), _)) => plan_for_template(st, tid, Some(&td)),
        Some(_) => None,
        None if st.gym.rest_taken_date.as_deref() == Some(td.as_str()) => Some(rest_plan()),
        None => None,
    };
    let len = rot.items.len() as i64;
    let mut out = vec![];
    for i in 0..n {
        let d = start + Duration::days(i);
        if d < today {
            out.push(None);
            continue;
        }
        if d == today {
            if let Some(p) = &fixed_today {
                out.push(Some(p.clone()));
                continue;
            }
        }
        let offset = (d - today).num_days() - fixed_today.is_some() as i64;
        let idx = (rot.next_index as i64 + offset).rem_euclid(len) as usize;
        let before = if d == today { Some(td.clone()) } else { None };
        out.push(Some(plan_for_item(st, &rot.items[idx], before.as_deref())));
    }
    out
}

pub fn next(st: &State) -> WorkoutPlan {
    let rot = get_rotation(st);
    match rot.items.get(rot.next_index) {
        Some(it) => plan_for_item(st, it, None),
        None => rest_plan(),
    }
}

// ------------------------------------------------------------------ sessions

#[derive(Deserialize)]
pub struct SessionIn {
    template_id: i64,
}

#[derive(Deserialize)]
pub struct SetsIn {
    sets: Vec<SetRec>,
}

pub fn session_create(env: &mut Env, b: SessionIn) -> Res<Session> {
    let t = env.st.gym.templates.iter().find(|t| t.id == b.template_id).cloned().ok_or_else(|| ApiErr::bad("template does not exist"))?;
    let td = fmt_date(env.today);
    if let Some(s) = env.st.gym.sessions.iter().filter(|s| s.date == td && !s.completed && s.template_id == Some(t.id)).max_by_key(|s| s.id) {
        return Ok(s.clone());
    }
    let plan = plan_for_template(env.st, t.id, Some(&td)).ok_or_else(|| ApiErr::bad("template does not exist"))?;
    if t.kind == "cardio" {
        let (lo, hi) = (t.minutes_min.unwrap_or(1), t.minutes_max.unwrap_or(600));
        let cardio = Cardio { activity: String::new(), minutes: 30.clamp(lo, hi.max(lo)) };
        let s = Session { id: env.st.new_id(), template_id: Some(t.id), name: t.name, date: td, completed: false, sets: vec![], cardio: Some(cardio) };
        env.st.gym.sessions.push(s.clone());
        env.changed = true;
        return Ok(s);
    }
    let mut sets = vec![];
    for e in &plan.exercises {
        for n in 1..=e.sets {
            let (w, reps) = match (&e.last, e.suggested_weight) {
                (Some(l), Some(sw)) => {
                    let last_w = l.sets.iter().map(|s| s.weight).fold(0.0, f64::max);
                    let reps = if (sw - last_w).abs() > 1e-9 { e.rep_min } else { l.sets.get((n - 1) as usize).or(l.sets.last()).map(|s| s.reps).unwrap_or(e.rep_min) };
                    (sw, reps)
                }
                (None, Some(sw)) => (sw, e.rep_min),
                _ => (0.0, e.rep_min),
            };
            sets.push(SetRec { exercise_id: e.exercise_id, set_no: n, weight: w, reps, done: false });
        }
    }
    let s = Session { id: env.st.new_id(), template_id: Some(t.id), name: t.name, date: td, completed: false, sets, cardio: None };
    env.st.gym.sessions.push(s.clone());
    env.changed = true;
    Ok(s)
}

pub fn session_get(st: &State, id: i64) -> Res<Session> {
    st.gym.sessions.iter().find(|s| s.id == id).cloned().ok_or_else(|| ApiErr::not_found("session not found"))
}

pub fn session_list(st: &State, q: &std::collections::HashMap<String, String>) -> Value {
    let limit: usize = q.get("limit").and_then(|s| s.parse::<i64>().ok()).unwrap_or(30).clamp(1, 200) as usize;
    let mut v: Vec<&Session> = st.gym.sessions.iter().collect();
    v.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
    v.truncate(limit);
    json!(v)
}

pub fn session_sets(env: &mut Env, id: i64, b: SetsIn) -> Res<Session> {
    session_get(env.st, id)?;
    if b.sets.iter().any(|s| s.set_no < 1 || s.reps < 0 || s.weight < 0.0 || !s.weight.is_finite()) {
        return Err(ApiErr::bad("sets need set_no >= 1 and non-negative weight/reps"));
    }
    let s = env.st.gym.sessions.iter_mut().find(|s| s.id == id).unwrap();
    let mut sets: Vec<SetRec> = vec![];
    for x in b.sets {
        sets.retain(|y| !(y.exercise_id == x.exercise_id && y.set_no == x.set_no));
        sets.push(x);
    }
    s.sets = sets;
    env.changed = true;
    Ok(s.clone())
}

#[derive(Deserialize)]
pub struct CardioIn {
    #[serde(default)]
    activity: String,
    minutes: i64,
}

pub fn session_cardio(env: &mut Env, id: i64, b: CardioIn) -> Res<Session> {
    let s = session_get(env.st, id)?;
    if s.cardio.is_none() {
        return Err(ApiErr::bad("this is not a cardio session"));
    }
    let activity = b.activity.trim().to_string();
    if activity.chars().count() > 80 {
        return Err(ApiErr::bad("activity is too long"));
    }
    if !(1..=600).contains(&b.minutes) {
        return Err(ApiErr::bad("minutes must be between 1 and 600"));
    }
    let s = env.st.gym.sessions.iter_mut().find(|s| s.id == id).unwrap();
    s.cardio = Some(Cardio { activity, minutes: b.minutes });
    env.changed = true;
    Ok(s.clone())
}

pub fn session_complete(env: &mut Env, id: i64) -> Res<Session> {
    let s = session_get(env.st, id)?;
    if s.cardio.as_ref().is_some_and(|c| c.activity.trim().is_empty()) && !s.completed {
        return Err(ApiErr::bad("pick what you did first"));
    }
    if !s.completed {
        env.st.gym.sessions.iter_mut().find(|x| x.id == id).unwrap().completed = true;
        // advance the rotation past this template (searching forward from the current position)
        let rot = get_rotation(env.st);
        if let Some(tid) = s.template_id {
            let len = rot.items.len();
            if let Some(k) = (0..len).map(|k| (rot.next_index + k) % len).find(|&i| rot.items[i].template_id == Some(tid) && !rot.items[i].rest) {
                advance(env.st, len, k, env.today);
            }
        }
        env.changed = true;
    }
    session_get(env.st, id)
}

// ------------------------------------------------------------------ history

#[derive(Serialize, Clone, Debug)]
pub struct HistoryPoint {
    pub date: String,
    /// For assisted exercises this is the LOWEST counterweight of the session (the best one).
    pub top_weight: f64,
    pub best_reps: i64,
    /// null for assisted exercises (a 1RM estimate makes no sense there).
    pub est_1rm: Option<f64>,
}

pub fn exercise_history(st: &State, id: i64) -> Res<Vec<HistoryPoint>> {
    if !st.gym.exercises.iter().any(|e| e.id == id) {
        return Err(ApiErr::not_found("exercise not found"));
    }
    Ok(history(st, id, None, None))
}

/// Per-session history points for an exercise, optionally restricted to a date window.
pub fn history(st: &State, id: i64, from: Option<&str>, to: Option<&str>) -> Vec<HistoryPoint> {
    let mut sessions: Vec<&Session> = st
        .gym
        .sessions
        .iter()
        .filter(|s| s.completed && from.is_none_or(|f| s.date.as_str() >= f) && to.is_none_or(|t| s.date.as_str() <= t))
        .collect();
    sessions.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    let assisted = st.gym.exercises.iter().any(|e| e.id == id && e.assisted);
    let mut out = vec![];
    for s in sessions {
        let mut sets: Vec<&SetRec> = s.sets.iter().filter(|x| x.exercise_id == id && x.done).collect();
        if sets.is_empty() {
            continue;
        }
        sets.sort_by_key(|x| x.set_no);
        let mut p: Option<(HistoryPoint, f64)> = None;
        for x in sets {
            let e1 = x.weight * (1.0 + x.reps as f64 / 30.0);
            match p.as_mut() {
                None => p = Some((HistoryPoint { date: s.date.clone(), top_weight: x.weight, best_reps: x.reps, est_1rm: None }, e1)),
                Some((p, best)) => {
                    let better = if assisted { x.weight < p.top_weight } else { x.weight > p.top_weight };
                    if better {
                        p.top_weight = x.weight;
                        p.best_reps = x.reps;
                    } else if (x.weight - p.top_weight).abs() < 1e-9 && x.reps > p.best_reps {
                        p.best_reps = x.reps;
                    }
                    *best = best.max(e1);
                }
            }
        }
        if let Some((mut p, best)) = p {
            p.est_1rm = if assisted { None } else { Some((best * 10.0).round() / 10.0) };
            out.push(p);
        }
    }
    out
}

/// Completed sessions in [from, to] as (date, name, cardio).
pub fn sessions_between(st: &State, from: NaiveDate, to: NaiveDate) -> Vec<(String, String, Option<Cardio>)> {
    let (f, t) = (fmt_date(from), fmt_date(to));
    let mut v: Vec<&Session> = st.gym.sessions.iter().filter(|s| s.completed && s.date >= f && s.date <= t).collect();
    v.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    v.into_iter().map(|s| (s.date.clone(), s.name.clone(), s.cardio.clone())).collect()
}

/// Exercises with a new best top weight during [from, to] versus everything before `from`.
/// Each entry is (name, now, was, assisted); for assisted exercises "better" means a lower counterweight.
pub fn weekly_prs(st: &State, from: NaiveDate, to: NaiveDate) -> Vec<(String, f64, f64, bool)> {
    let (f, t) = (fmt_date(from), fmt_date(to));
    let mut exs: Vec<&Exercise> = st
        .gym
        .exercises
        .iter()
        .filter(|e| st.gym.sessions.iter().any(|s| s.completed && s.date >= f && s.date <= t && s.sets.iter().any(|x| x.exercise_id == e.id && x.done)))
        .collect();
    exs.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = vec![];
    for e in exs {
        let best = |pts: Vec<HistoryPoint>| -> Option<f64> {
            let ws = pts.iter().map(|p| p.top_weight);
            if e.assisted { ws.reduce(f64::min) } else { ws.reduce(f64::max) }
        };
        let (Some(this), prev_to) = (best(history(st, e.id, Some(&f), Some(&t))), fmt_date(from - Duration::days(1))) else { continue };
        let Some(prev) = best(history(st, e.id, None, Some(&prev_to))) else { continue };
        let improved = if e.assisted { this < prev } else { prev > 0.0 && this > prev };
        if improved {
            out.push((e.name.clone(), this, prev, e.assisted));
        }
    }
    out
}
