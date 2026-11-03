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
    let e = Exercise { id: env.st.new_id(), name };
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
}

#[derive(Deserialize)]
pub struct TemplateIn {
    name: String,
    #[serde(default)]
    exercises: Vec<TemplateExerciseIn>,
}

fn find_or_create_exercise(st: &mut State, name: &str) -> i64 {
    if let Some(e) = st.gym.exercises.iter().find(|e| e.name.eq_ignore_ascii_case(name)) {
        return e.id;
    }
    let id = st.new_id();
    st.gym.exercises.push(Exercise { id, name: name.to_string() });
    id
}

fn build_template_exercises(st: &mut State, list: &[TemplateExerciseIn]) -> Res<Vec<TemplateExercise>> {
    // validate everything first so nothing is half-written
    for e in list {
        if e.sets < 1 || e.rep_min < 1 || e.rep_max < e.rep_min {
            return Err(ApiErr::bad("sets and reps must be positive and rep_min <= rep_max"));
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
        let name = st.gym.exercises.iter().find(|x| x.id == id).map(|x| x.name.clone()).unwrap_or_default();
        out.push(TemplateExercise { exercise_id: id, name, sets: e.sets, rep_min: e.rep_min, rep_max: e.rep_max });
    }
    Ok(out)
}

pub fn create_template(env: &mut Env, b: TemplateIn) -> Res<Template> {
    if b.name.trim().is_empty() {
        return Err(ApiErr::bad("template name cannot be empty"));
    }
    let exercises = build_template_exercises(env.st, &b.exercises)?;
    let t = Template { id: env.st.new_id(), name: b.name.trim().to_string(), exercises };
    env.st.gym.templates.push(t.clone());
    env.changed = true;
    Ok(t)
}

pub fn update_template(env: &mut Env, id: i64, b: TemplateIn) -> Res<Template> {
    if b.name.trim().is_empty() {
        return Err(ApiErr::bad("template name cannot be empty"));
    }
    if !env.st.gym.templates.iter().any(|t| t.id == id) {
        return Err(ApiErr::not_found("template not found"));
    }
    let exercises = build_template_exercises(env.st, &b.exercises)?;
    let t = env.st.gym.templates.iter_mut().find(|t| t.id == id).unwrap();
    t.name = b.name.trim().to_string();
    t.exercises = exercises;
    let out = t.clone();
    env.changed = true;
    Ok(out)
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
    env.changed = true;
    Ok(get_rotation(env.st))
}

fn advance(st: &mut State, len: usize, to: usize) {
    st.gym.rotation.next_index = (to + 1) % len;
}

pub fn rest_taken(env: &mut Env) -> Res<Rotation> {
    let rot = get_rotation(env.st);
    match rot.items.get(rot.next_index) {
        Some(it) if it.rest => {
            advance(env.st, rot.items.len(), rot.next_index);
            env.st.gym.rest_taken_date = Some(fmt_date(env.today));
            env.changed = true;
            Ok(get_rotation(env.st))
        }
        _ => Err(ApiErr::bad("the next rotation item is not a rest day")),
    }
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
}

#[derive(Serialize, Clone, Debug)]
pub struct WorkoutPlan {
    pub rest: bool,
    pub template_id: Option<i64>,
    pub name: String,
    pub exercises: Vec<PlanExercise>,
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
pub fn suggest(last: &Last, rep_max: i64) -> (f64, String) {
    let w = last.sets.iter().map(|s| s.weight).fold(0.0, f64::max);
    let all_hit = !last.sets.is_empty() && last.sets.iter().all(|s| (s.weight - w).abs() < 1e-9 && s.reps >= rep_max);
    if all_hit {
        let up = w + 2.5;
        (up, format!("You hit {} reps on every set last time, try {} kg", rep_max, fmt_w(up)))
    } else {
        (w, format!("Stay at {} kg and aim for {} reps on every set", fmt_w(w), rep_max))
    }
}

pub fn rest_plan() -> WorkoutPlan {
    WorkoutPlan { rest: true, template_id: None, name: "Rest day".into(), exercises: vec![] }
}

pub fn plan_for_template(st: &State, tid: i64, before: Option<&str>) -> Option<WorkoutPlan> {
    let t = st.gym.templates.iter().find(|t| t.id == tid)?;
    let mut exercises = vec![];
    for e in &t.exercises {
        let last = last_for(st, e.exercise_id, before);
        let (sw, hint) = match &last {
            Some(l) => {
                let (w, h) = suggest(l, e.rep_max);
                (Some(w), Some(h))
            }
            None => (None, None),
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
        });
    }
    Some(WorkoutPlan { rest: false, template_id: Some(tid), name: t.name.clone(), exercises })
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
    let mut sets = vec![];
    for e in &plan.exercises {
        for n in 1..=e.sets {
            let (w, reps) = match (&e.last, e.suggested_weight) {
                (Some(l), Some(sw)) => {
                    let last_w = l.sets.iter().map(|s| s.weight).fold(0.0, f64::max);
                    let reps = if sw > last_w { e.rep_min } else { l.sets.get((n - 1) as usize).or(l.sets.last()).map(|s| s.reps).unwrap_or(e.rep_min) };
                    (sw, reps)
                }
                _ => (0.0, e.rep_min),
            };
            sets.push(SetRec { exercise_id: e.exercise_id, set_no: n, weight: w, reps, done: false });
        }
    }
    let s = Session { id: env.st.new_id(), template_id: Some(t.id), name: t.name, date: td, completed: false, sets };
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

pub fn session_complete(env: &mut Env, id: i64) -> Res<Session> {
    let s = session_get(env.st, id)?;
    if !s.completed {
        env.st.gym.sessions.iter_mut().find(|x| x.id == id).unwrap().completed = true;
        // advance the rotation past this template (searching forward from the current position)
        let rot = get_rotation(env.st);
        if let Some(tid) = s.template_id {
            let len = rot.items.len();
            if let Some(k) = (0..len).map(|k| (rot.next_index + k) % len).find(|&i| rot.items[i].template_id == Some(tid) && !rot.items[i].rest) {
                advance(env.st, len, k);
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
    pub top_weight: f64,
    pub best_reps: i64,
    pub est_1rm: f64,
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
    let mut out = vec![];
    for s in sessions {
        let mut sets: Vec<&SetRec> = s.sets.iter().filter(|x| x.exercise_id == id && x.done).collect();
        if sets.is_empty() {
            continue;
        }
        sets.sort_by_key(|x| x.set_no);
        let mut p: Option<HistoryPoint> = None;
        for x in sets {
            let e1 = x.weight * (1.0 + x.reps as f64 / 30.0);
            match p.as_mut() {
                None => p = Some(HistoryPoint { date: s.date.clone(), top_weight: x.weight, best_reps: x.reps, est_1rm: e1 }),
                Some(p) => {
                    if x.weight > p.top_weight {
                        p.top_weight = x.weight;
                        p.best_reps = x.reps;
                    } else if (x.weight - p.top_weight).abs() < 1e-9 && x.reps > p.best_reps {
                        p.best_reps = x.reps;
                    }
                    p.est_1rm = p.est_1rm.max(e1);
                }
            }
        }
        if let Some(mut p) = p {
            p.est_1rm = (p.est_1rm * 10.0).round() / 10.0;
            out.push(p);
        }
    }
    out
}

/// Completed sessions in [from, to] as (date, name).
pub fn sessions_between(st: &State, from: NaiveDate, to: NaiveDate) -> Vec<(String, String)> {
    let (f, t) = (fmt_date(from), fmt_date(to));
    let mut v: Vec<&Session> = st.gym.sessions.iter().filter(|s| s.completed && s.date >= f && s.date <= t).collect();
    v.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    v.into_iter().map(|s| (s.date.clone(), s.name.clone())).collect()
}

/// Exercises with a new best top weight during [from, to] versus everything before `from`.
pub fn weekly_prs(st: &State, from: NaiveDate, to: NaiveDate) -> Vec<(String, f64, f64)> {
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
        let this = history(st, e.id, Some(&f), Some(&t)).iter().map(|p| p.top_weight).fold(0.0, f64::max);
        let prev_to = fmt_date(from - Duration::days(1));
        let prev = history(st, e.id, None, Some(&prev_to)).iter().map(|p| p.top_weight).fold(0.0, f64::max);
        if prev > 0.0 && this > prev {
            out.push((e.name.clone(), this, prev));
        }
    }
    out
}
