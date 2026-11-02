use crate::db::{self, fmt_date, today};
use crate::error::{AppError, AppJson, Res};
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

// ----------------------------------------------------------------- exercises

#[derive(Serialize)]
pub struct Exercise {
    pub id: i64,
    pub name: String,
}

#[derive(Deserialize)]
pub struct ExerciseIn {
    name: String,
}

fn get_exercise(conn: &Connection, id: i64) -> rusqlite::Result<Exercise> {
    conn.query_row("SELECT id,name FROM exercises WHERE id=?1", params![id], |r| {
        Ok(Exercise { id: r.get(0)?, name: r.get(1)? })
    })
}

pub fn find_or_create_exercise(conn: &Connection, name: &str) -> rusqlite::Result<i64> {
    if let Some(id) = conn
        .query_row("SELECT id FROM exercises WHERE name=?1", params![name], |r| r.get(0))
        .optional()?
    {
        return Ok(id);
    }
    conn.execute("INSERT INTO exercises(name) VALUES(?1)", params![name])?;
    Ok(conn.last_insert_rowid())
}

pub async fn list_exercises(State(st): State<AppState>) -> Res<Json<Vec<Exercise>>> {
    let conn = st.db.lock().unwrap();
    let mut s = conn.prepare("SELECT id,name FROM exercises ORDER BY name COLLATE NOCASE")?;
    let v = s.query_map([], |r| Ok(Exercise { id: r.get(0)?, name: r.get(1)? }))?.collect::<rusqlite::Result<_>>()?;
    Ok(Json(v))
}

fn exercise_name_ok(name: &str) -> Res<&str> {
    let n = name.trim();
    if n.is_empty() {
        return Err(AppError::bad("exercise name cannot be empty"));
    }
    Ok(n)
}

fn is_unique_violation(e: &rusqlite::Error) -> bool {
    matches!(e, rusqlite::Error::SqliteFailure(f, _) if f.code == rusqlite::ErrorCode::ConstraintViolation)
}

pub async fn create_exercise(State(st): State<AppState>, AppJson(b): AppJson<ExerciseIn>) -> Res<Json<Exercise>> {
    let name = exercise_name_ok(&b.name)?;
    let conn = st.db.lock().unwrap();
    match conn.execute("INSERT INTO exercises(name) VALUES(?1)", params![name]) {
        Ok(_) => Ok(Json(get_exercise(&conn, conn.last_insert_rowid())?)),
        Err(e) if is_unique_violation(&e) => Err(AppError::bad("an exercise with that name already exists")),
        Err(e) => Err(e.into()),
    }
}

pub async fn update_exercise(State(st): State<AppState>, Path(id): Path<i64>, AppJson(b): AppJson<ExerciseIn>) -> Res<Json<Exercise>> {
    let name = exercise_name_ok(&b.name)?;
    let conn = st.db.lock().unwrap();
    match conn.execute("UPDATE exercises SET name=?1 WHERE id=?2", params![name, id]) {
        Ok(0) => Err(AppError::not_found("exercise not found")),
        Ok(_) => Ok(Json(get_exercise(&conn, id)?)),
        Err(e) if is_unique_violation(&e) => Err(AppError::bad("an exercise with that name already exists")),
        Err(e) => Err(e.into()),
    }
}

pub async fn delete_exercise(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<Value>> {
    let conn = st.db.lock().unwrap();
    if conn.execute("DELETE FROM exercises WHERE id=?1", params![id])? == 0 {
        return Err(AppError::not_found("exercise not found"));
    }
    conn.execute("DELETE FROM template_exercises WHERE exercise_id=?1", params![id])?;
    Ok(Json(json!({"ok": true})))
}

// ----------------------------------------------------------------- templates

#[derive(Serialize, Clone)]
pub struct TemplateExercise {
    pub exercise_id: i64,
    pub name: String,
    pub sets: i64,
    pub rep_min: i64,
    pub rep_max: i64,
}

#[derive(Serialize)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub exercises: Vec<TemplateExercise>,
}

#[derive(Deserialize)]
pub struct TemplateExerciseIn {
    exercise_id: Option<i64>,
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

fn template_exercises(conn: &Connection, tid: i64) -> rusqlite::Result<Vec<TemplateExercise>> {
    let mut s = conn.prepare(
        "SELECT te.exercise_id, e.name, te.sets, te.rep_min, te.rep_max FROM template_exercises te
         JOIN exercises e ON e.id=te.exercise_id WHERE te.template_id=?1 ORDER BY te.pos, te.id",
    )?;
    let v = s
        .query_map(params![tid], |r| {
            Ok(TemplateExercise { exercise_id: r.get(0)?, name: r.get(1)?, sets: r.get(2)?, rep_min: r.get(3)?, rep_max: r.get(4)? })
        })?
        .collect();
    v
}

fn get_template(conn: &Connection, id: i64) -> rusqlite::Result<Template> {
    let name: String = conn.query_row("SELECT name FROM templates WHERE id=?1", params![id], |r| r.get(0))?;
    Ok(Template { id, name, exercises: template_exercises(conn, id)? })
}

fn write_template_exercises(conn: &Connection, tid: i64, list: &[TemplateExerciseIn]) -> Res<()> {
    // validate first so we never leave a half-written template
    let mut resolved = vec![];
    for e in list {
        if e.sets < 1 || e.rep_min < 1 || e.rep_max < e.rep_min {
            return Err(AppError::bad("sets and reps must be positive and rep_min <= rep_max"));
        }
        let ex = match (e.exercise_id, e.name.as_deref().map(str::trim).filter(|n| !n.is_empty())) {
            (Some(id), _) => {
                get_exercise(conn, id).map_err(|_| AppError::bad(format!("exercise {id} does not exist")))?;
                id
            }
            (None, Some(n)) => find_or_create_exercise(conn, n)?,
            _ => return Err(AppError::bad("each template exercise needs exercise_id")),
        };
        resolved.push((ex, e));
    }
    conn.execute("DELETE FROM template_exercises WHERE template_id=?1", params![tid])?;
    for (pos, (ex, e)) in resolved.into_iter().enumerate() {
        conn.execute(
            "INSERT INTO template_exercises(template_id,exercise_id,pos,sets,rep_min,rep_max) VALUES(?1,?2,?3,?4,?5,?6)",
            params![tid, ex, pos as i64, e.sets, e.rep_min, e.rep_max],
        )?;
    }
    Ok(())
}

pub async fn list_templates(State(st): State<AppState>) -> Res<Json<Vec<Template>>> {
    let conn = st.db.lock().unwrap();
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM templates ORDER BY id")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Json(ids.into_iter().map(|i| get_template(&conn, i)).collect::<rusqlite::Result<_>>()?))
}

pub async fn create_template(State(st): State<AppState>, AppJson(b): AppJson<TemplateIn>) -> Res<Json<Template>> {
    if b.name.trim().is_empty() {
        return Err(AppError::bad("template name cannot be empty"));
    }
    let conn = st.db.lock().unwrap();
    conn.execute("INSERT INTO templates(name) VALUES(?1)", params![b.name.trim()])?;
    let id = conn.last_insert_rowid();
    if let Err(e) = write_template_exercises(&conn, id, &b.exercises) {
        conn.execute("DELETE FROM templates WHERE id=?1", params![id])?;
        return Err(e);
    }
    Ok(Json(get_template(&conn, id)?))
}

pub async fn update_template(State(st): State<AppState>, Path(id): Path<i64>, AppJson(b): AppJson<TemplateIn>) -> Res<Json<Template>> {
    if b.name.trim().is_empty() {
        return Err(AppError::bad("template name cannot be empty"));
    }
    let conn = st.db.lock().unwrap();
    get_template(&conn, id).map_err(|_| AppError::not_found("template not found"))?;
    write_template_exercises(&conn, id, &b.exercises)?;
    conn.execute("UPDATE templates SET name=?1 WHERE id=?2", params![b.name.trim(), id])?;
    Ok(Json(get_template(&conn, id)?))
}

pub async fn delete_template(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<Value>> {
    let conn = st.db.lock().unwrap();
    if conn.execute("DELETE FROM templates WHERE id=?1", params![id])? == 0 {
        return Err(AppError::not_found("template not found"));
    }
    conn.execute("DELETE FROM template_exercises WHERE template_id=?1", params![id])?;
    let mut items = load_rotation_items(&conn)?;
    let before = items.len();
    items.retain(|i| i.template_id != Some(id));
    if items.len() != before {
        let next = rotation_next(&conn)?;
        save_rotation(&conn, &items, next)?;
    }
    Ok(Json(json!({"ok": true})))
}

// ------------------------------------------------------------------ rotation

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RotItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest: bool,
}

#[derive(Serialize)]
pub struct Rotation {
    pub items: Vec<RotItem>,
    pub next_index: usize,
}

#[derive(Deserialize)]
pub struct RotationIn {
    items: Vec<RotItem>,
    next_index: Option<usize>,
}

fn load_rotation_items(conn: &Connection) -> rusqlite::Result<Vec<RotItem>> {
    let mut s = conn.prepare("SELECT template_id,rest FROM rotation_items ORDER BY pos")?;
    let v = s.query_map([], |r| Ok(RotItem { template_id: r.get(0)?, rest: r.get::<_, i64>(1)? != 0 }))?.collect();
    v
}

fn rotation_next(conn: &Connection) -> rusqlite::Result<usize> {
    Ok(db::get_setting(conn, "rotation_next")?.and_then(|v| v.parse().ok()).unwrap_or(0))
}

pub fn save_rotation(conn: &Connection, items: &[RotItem], next: usize) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM rotation_items", [])?;
    for (i, it) in items.iter().enumerate() {
        conn.execute(
            "INSERT INTO rotation_items(pos,template_id,rest) VALUES(?1,?2,?3)",
            params![i as i64, if it.rest { None } else { it.template_id }, it.rest as i64],
        )?;
    }
    let next = if items.is_empty() { 0 } else { next % items.len() };
    db::set_setting(conn, "rotation_next", &next.to_string())
}

pub fn get_rotation(conn: &Connection) -> rusqlite::Result<Rotation> {
    let items = load_rotation_items(conn)?;
    let next = rotation_next(conn)?;
    let next_index = if items.is_empty() { 0 } else { next % items.len() };
    Ok(Rotation { items, next_index })
}

pub async fn rotation_get(State(st): State<AppState>) -> Res<Json<Rotation>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(get_rotation(&conn)?))
}

pub async fn rotation_put(State(st): State<AppState>, AppJson(b): AppJson<RotationIn>) -> Res<Json<Rotation>> {
    let conn = st.db.lock().unwrap();
    for it in &b.items {
        match (it.rest, it.template_id) {
            (true, _) => {}
            (false, Some(t)) => {
                get_template(&conn, t).map_err(|_| AppError::bad(format!("template {t} does not exist")))?;
            }
            (false, None) => return Err(AppError::bad("each rotation item needs template_id or rest:true")),
        }
    }
    let next = b.next_index.unwrap_or(rotation_next(&conn)?);
    save_rotation(&conn, &b.items, next)?;
    Ok(Json(get_rotation(&conn)?))
}

fn advance(conn: &Connection, rot: &Rotation, to: usize) -> rusqlite::Result<()> {
    db::set_setting(conn, "rotation_next", &((to + 1) % rot.items.len()).to_string())
}

pub async fn rest_taken(State(st): State<AppState>) -> Res<Json<Rotation>> {
    let conn = st.db.lock().unwrap();
    let rot = get_rotation(&conn)?;
    match rot.items.get(rot.next_index) {
        Some(it) if it.rest => {
            advance(&conn, &rot, rot.next_index)?;
            db::set_setting(&conn, "rest_taken_date", &fmt_date(today()))?;
            Ok(Json(get_rotation(&conn)?))
        }
        _ => Err(AppError::bad("the next rotation item is not a rest day")),
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

fn last_for(conn: &Connection, exercise_id: i64, before: Option<&str>) -> rusqlite::Result<Option<Last>> {
    let row: Option<(i64, String)> = conn
        .query_row(
            "SELECT s.id,s.date FROM sessions s JOIN session_sets ss ON ss.session_id=s.id
             WHERE s.completed=1 AND ss.exercise_id=?1 AND ss.done=1 AND (?2 IS NULL OR s.date < ?2)
             ORDER BY s.date DESC, s.id DESC LIMIT 1",
            params![exercise_id, before],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((sid, date)) = row else { return Ok(None) };
    let mut s = conn.prepare("SELECT weight,reps FROM session_sets WHERE session_id=?1 AND exercise_id=?2 AND done=1 ORDER BY set_no")?;
    let sets = s
        .query_map(params![sid, exercise_id], |r| Ok(LastSet { weight: r.get(0)?, reps: r.get(1)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(Some(Last { date, sets }))
}

/// Progression rule: all sets at the same weight W with reps >= rep_max -> W + 2.5, else W.
fn suggest(last: &Last, rep_max: i64) -> (f64, String) {
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

pub fn plan_for_template(conn: &Connection, tid: i64, before: Option<&str>) -> rusqlite::Result<WorkoutPlan> {
    let t = get_template(conn, tid)?;
    let mut exercises = vec![];
    for e in t.exercises {
        let last = last_for(conn, e.exercise_id, before)?;
        let (sw, hint) = match &last {
            Some(l) => {
                let (w, h) = suggest(l, e.rep_max);
                (Some(w), Some(h))
            }
            None => (None, None),
        };
        exercises.push(PlanExercise {
            exercise_id: e.exercise_id,
            name: e.name,
            sets: e.sets,
            rep_min: e.rep_min,
            rep_max: e.rep_max,
            last,
            suggested_weight: sw,
            hint,
        });
    }
    Ok(WorkoutPlan { rest: false, template_id: Some(tid), name: t.name, exercises })
}

fn plan_for_item(conn: &Connection, it: &RotItem, before: Option<&str>) -> rusqlite::Result<WorkoutPlan> {
    match (it.rest, it.template_id) {
        (false, Some(t)) => plan_for_template(conn, t, before),
        _ => Ok(rest_plan()),
    }
}

pub fn completed_session_on(conn: &Connection, d: NaiveDate) -> rusqlite::Result<Option<(i64, Option<i64>, String)>> {
    conn.query_row(
        "SELECT id,template_id,name FROM sessions WHERE date=?1 AND completed=1 ORDER BY id DESC LIMIT 1",
        params![fmt_date(d)],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )
    .optional()
}

/// Workout projection per day (None for past days or when no rotation is set up).
pub fn project(conn: &Connection, start: NaiveDate, n: i64, today: NaiveDate) -> rusqlite::Result<Vec<Option<WorkoutPlan>>> {
    let rot = get_rotation(conn)?;
    let mut out = vec![];
    if rot.items.is_empty() {
        return Ok((0..n).map(|_| None).collect());
    }
    let fixed_today: Option<WorkoutPlan> = match completed_session_on(conn, today)? {
        Some((_, Some(tid), _)) => Some(plan_for_template(conn, tid, Some(&fmt_date(today)))?),
        Some(_) => None,
        None if db::get_setting(conn, "rest_taken_date")?.as_deref() == Some(fmt_date(today).as_str()) => Some(rest_plan()),
        None => None,
    };
    let len = rot.items.len() as i64;
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
        let idx = ((rot.next_index as i64 + offset) % len) as usize;
        let before = if d == today { Some(fmt_date(today)) } else { None };
        out.push(Some(plan_for_item(conn, &rot.items[idx], before.as_deref())?));
    }
    Ok(out)
}

pub async fn next(State(st): State<AppState>) -> Res<Json<WorkoutPlan>> {
    let conn = st.db.lock().unwrap();
    let rot = get_rotation(&conn)?;
    let plan = match rot.items.get(rot.next_index) {
        Some(it) => plan_for_item(&conn, it, None)?,
        None => rest_plan(),
    };
    Ok(Json(plan))
}

// ------------------------------------------------------------------ sessions

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetRec {
    pub exercise_id: i64,
    pub set_no: i64,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub reps: i64,
    #[serde(default)]
    pub done: bool,
}

#[derive(Serialize)]
pub struct Session {
    pub id: i64,
    pub template_id: Option<i64>,
    pub name: String,
    pub date: String,
    pub completed: bool,
    pub sets: Vec<SetRec>,
}

fn get_session(conn: &Connection, id: i64) -> rusqlite::Result<Session> {
    let mut s = conn.query_row("SELECT id,template_id,name,date,completed FROM sessions WHERE id=?1", params![id], |r| {
        Ok(Session { id: r.get(0)?, template_id: r.get(1)?, name: r.get(2)?, date: r.get(3)?, completed: r.get::<_, i64>(4)? != 0, sets: vec![] })
    })?;
    let mut st = conn.prepare("SELECT exercise_id,set_no,weight,reps,done FROM session_sets WHERE session_id=?1 ORDER BY rowid")?;
    s.sets = st
        .query_map(params![id], |r| {
            Ok(SetRec { exercise_id: r.get(0)?, set_no: r.get(1)?, weight: r.get(2)?, reps: r.get(3)?, done: r.get::<_, i64>(4)? != 0 })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(s)
}

#[derive(Deserialize)]
pub struct SessionIn {
    template_id: i64,
}

#[derive(Deserialize)]
pub struct SetsIn {
    sets: Vec<SetRec>,
}

pub async fn session_create(State(st): State<AppState>, AppJson(b): AppJson<SessionIn>) -> Res<Json<Session>> {
    let conn = st.db.lock().unwrap();
    let t = get_template(&conn, b.template_id).map_err(|_| AppError::bad("template does not exist"))?;
    let td = fmt_date(today());
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM sessions WHERE date=?1 AND completed=0 AND template_id=?2 ORDER BY id DESC LIMIT 1",
            params![td, t.id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(Json(get_session(&conn, id)?));
    }
    let plan = plan_for_template(&conn, t.id, Some(&td))?;
    conn.execute("INSERT INTO sessions(template_id,name,date,completed) VALUES(?1,?2,?3,0)", params![t.id, t.name, td])?;
    let sid = conn.last_insert_rowid();
    for e in &plan.exercises {
        for n in 1..=e.sets {
            let (w, reps) = match (&e.last, e.suggested_weight) {
                (Some(l), Some(sw)) => {
                    let last_w = l.sets.iter().map(|s| s.weight).fold(0.0, f64::max);
                    let reps = if sw > last_w {
                        e.rep_min
                    } else {
                        l.sets.get((n - 1) as usize).or(l.sets.last()).map(|s| s.reps).unwrap_or(e.rep_min)
                    };
                    (sw, reps)
                }
                _ => (0.0, e.rep_min),
            };
            conn.execute(
                "INSERT INTO session_sets(session_id,exercise_id,set_no,weight,reps,done) VALUES(?1,?2,?3,?4,?5,0)",
                params![sid, e.exercise_id, n, w, reps],
            )?;
        }
    }
    Ok(Json(get_session(&conn, sid)?))
}

pub async fn session_get(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<Session>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(get_session(&conn, id).map_err(|_| AppError::not_found("session not found"))?))
}

pub async fn session_list(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Res<Json<Vec<Session>>> {
    let conn = st.db.lock().unwrap();
    let limit: i64 = q.get("limit").and_then(|s| s.parse().ok()).unwrap_or(30).clamp(1, 200);
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM sessions ORDER BY date DESC, id DESC LIMIT ?1")?
        .query_map(params![limit], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(Json(ids.into_iter().map(|i| get_session(&conn, i)).collect::<rusqlite::Result<_>>()?))
}

pub async fn session_sets(State(st): State<AppState>, Path(id): Path<i64>, AppJson(b): AppJson<SetsIn>) -> Res<Json<Session>> {
    let conn = st.db.lock().unwrap();
    get_session(&conn, id).map_err(|_| AppError::not_found("session not found"))?;
    if b.sets.iter().any(|s| s.set_no < 1 || s.reps < 0 || s.weight < 0.0 || !s.weight.is_finite()) {
        return Err(AppError::bad("sets need set_no >= 1 and non-negative weight/reps"));
    }
    conn.execute("DELETE FROM session_sets WHERE session_id=?1", params![id])?;
    for s in &b.sets {
        conn.execute(
            "INSERT OR REPLACE INTO session_sets(session_id,exercise_id,set_no,weight,reps,done) VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, s.exercise_id, s.set_no, s.weight, s.reps, s.done as i64],
        )?;
    }
    Ok(Json(get_session(&conn, id)?))
}

pub async fn session_complete(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<Session>> {
    let conn = st.db.lock().unwrap();
    let s = get_session(&conn, id).map_err(|_| AppError::not_found("session not found"))?;
    if !s.completed {
        conn.execute("UPDATE sessions SET completed=1 WHERE id=?1", params![id])?;
        // advance the rotation past this template (searching forward from the current position)
        let rot = get_rotation(&conn)?;
        if let Some(tid) = s.template_id {
            let len = rot.items.len();
            if let Some(k) = (0..len).map(|k| (rot.next_index + k) % len).find(|&i| rot.items[i].template_id == Some(tid) && !rot.items[i].rest) {
                advance(&conn, &rot, k)?;
            }
        }
    }
    Ok(Json(get_session(&conn, id)?))
}

#[derive(Serialize)]
pub struct HistoryPoint {
    pub date: String,
    pub top_weight: f64,
    pub best_reps: i64,
    pub est_1rm: f64,
}

pub async fn exercise_history(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<Vec<HistoryPoint>>> {
    let conn = st.db.lock().unwrap();
    get_exercise(&conn, id).map_err(|_| AppError::not_found("exercise not found"))?;
    Ok(Json(history(&conn, id, None, None)?))
}

/// Per-session history points for an exercise, optionally restricted to a date window.
pub fn history(conn: &Connection, id: i64, from: Option<&str>, to: Option<&str>) -> rusqlite::Result<Vec<HistoryPoint>> {
    let mut s = conn.prepare(
        "SELECT s.id,s.date,ss.weight,ss.reps FROM sessions s JOIN session_sets ss ON ss.session_id=s.id
         WHERE s.completed=1 AND ss.exercise_id=?1 AND ss.done=1
           AND (?2 IS NULL OR s.date>=?2) AND (?3 IS NULL OR s.date<=?3)
         ORDER BY s.date, s.id, ss.set_no",
    )?;
    let rows = s
        .query_map(params![id, from, to], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, f64>(2)?, r.get::<_, i64>(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out: Vec<(i64, HistoryPoint)> = vec![];
    for (sid, date, w, reps) in rows {
        let e1 = w * (1.0 + reps as f64 / 30.0);
        match out.last_mut() {
            Some((last_sid, p)) if *last_sid == sid => {
                if w > p.top_weight {
                    p.top_weight = w;
                    p.best_reps = reps;
                } else if (w - p.top_weight).abs() < 1e-9 && reps > p.best_reps {
                    p.best_reps = reps;
                }
                p.est_1rm = p.est_1rm.max(e1);
            }
            _ => out.push((sid, HistoryPoint { date, top_weight: w, best_reps: reps, est_1rm: e1 })),
        }
    }
    Ok(out
        .into_iter()
        .map(|(_, mut p)| {
            p.est_1rm = (p.est_1rm * 10.0).round() / 10.0;
            p
        })
        .collect())
}

/// Completed sessions in [from, to] as (date, name).
pub fn sessions_between(conn: &Connection, from: NaiveDate, to: NaiveDate) -> rusqlite::Result<Vec<(String, String)>> {
    let mut s = conn.prepare("SELECT date,name FROM sessions WHERE completed=1 AND date>=?1 AND date<=?2 ORDER BY date,id")?;
    let v = s.query_map(params![fmt_date(from), fmt_date(to)], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
    v
}

/// Exercises with a new best top weight during [from, to] versus everything before `from`.
pub fn weekly_prs(conn: &Connection, from: NaiveDate, to: NaiveDate) -> rusqlite::Result<Vec<(String, f64, f64)>> {
    let (f, t) = (fmt_date(from), fmt_date(to));
    let mut s = conn.prepare(
        "SELECT DISTINCT e.id,e.name FROM session_sets ss JOIN sessions s ON s.id=ss.session_id
         JOIN exercises e ON e.id=ss.exercise_id
         WHERE s.completed=1 AND ss.done=1 AND s.date>=?1 AND s.date<=?2 ORDER BY e.name",
    )?;
    let exs = s.query_map(params![f, t], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut out = vec![];
    for (id, name) in exs {
        let this = history(conn, id, Some(&f), Some(&t))?.iter().map(|p| p.top_weight).fold(0.0, f64::max);
        let prev_to = fmt_date(from - Duration::days(1));
        let prev = history(conn, id, None, Some(&prev_to))?.iter().map(|p| p.top_weight).fold(0.0, f64::max);
        if prev > 0.0 && this > prev {
            out.push((name, this, prev));
        }
    }
    Ok(out)
}
