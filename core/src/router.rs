//! The REST-like router: `call(state, method, path, body, ctx) -> Response`.
use crate::briefing;
use crate::daynote;
use crate::digest;
use crate::env::Env;
use crate::error::{ApiErr, Res};
use crate::goals;
use crate::gym;
use crate::holidays;
use crate::memory;
use crate::model::*;
use crate::seed;
use crate::tasks;
use crate::util::*;
use crate::weather;
use chrono::NaiveDate;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Serialize, Debug)]
pub struct Response {
    pub status: u16,
    pub body: Value,
    pub state: Option<State>,
}

fn err_response(e: ApiErr) -> Response {
    Response { status: e.status, body: json!({"error": e.msg}), state: None }
}

fn parse<T: DeserializeOwned>(v: &Value) -> Res<T> {
    serde_json::from_value(v.clone()).map_err(|e| ApiErr::bad(format!("invalid request body: {e}")))
}

fn id_of(s: &str) -> Res<i64> {
    s.parse().map_err(|_| ApiErr::bad("id must be an integer"))
}

fn ok<T: Serialize>(v: T) -> Res<Value> {
    Ok(serde_json::to_value(v).unwrap_or(Value::Null))
}

fn parse_query(q: &str) -> HashMap<String, String> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| match p.split_once('=') {
            Some((k, v)) => (url_decode(k), url_decode(v)),
            None => (url_decode(p), String::new()),
        })
        .collect()
}

fn date_param(env: &Env, q: &HashMap<String, String>, key: &str) -> Res<NaiveDate> {
    match q.get(key).filter(|s| !s.is_empty()) {
        Some(s) => parse_date(s).ok_or_else(|| ApiErr::bad(format!("{key} must be YYYY-MM-DD"))),
        None => Ok(env.today),
    }
}

fn settings_json(env: &Env) -> Value {
    let mut v = serde_json::to_value(&env.st.settings).unwrap_or(json!({}));
    v["gemini_api_key_set"] = json!(env.ctx.has_key);
    v
}

fn put_settings(env: &mut Env, b: &Value) -> Res<Value> {
    let s = &mut env.st.settings;
    let mut n = s.clone();
    if !b.is_object() {
        return Err(ApiErr::bad("body must be a JSON object"));
    }
    let text = |k: &str| b.get(k).and_then(Value::as_str).map(|x| x.trim().to_string());
    if let Some(v) = text("assistant_name") {
        if v.is_empty() {
            return Err(ApiErr::bad("assistant_name cannot be empty"));
        }
        n.assistant_name = v;
    }
    if let Some(v) = text("user_name") {
        n.user_name = v;
    }
    if let Some(v) = b.get("voice").and_then(Value::as_str) {
        n.voice = v.trim().to_string();
    }
    if let Some(v) = text("location_name") {
        n.location_name = v;
    }
    if let Some(x) = b.get("latitude").filter(|x| !x.is_null()) {
        let lat = x.as_f64().ok_or_else(|| ApiErr::bad("latitude must be a number"))?;
        if !(-90.0..=90.0).contains(&lat) {
            return Err(ApiErr::bad("latitude must be between -90 and 90"));
        }
        n.latitude = lat;
    }
    if let Some(x) = b.get("longitude").filter(|x| !x.is_null()) {
        let lon = x.as_f64().ok_or_else(|| ApiErr::bad("longitude must be a number"))?;
        if !(-180.0..=180.0).contains(&lon) {
            return Err(ApiErr::bad("longitude must be between -180 and 180"));
        }
        n.longitude = lon;
    }
    if let Some(tz) = text("timezone") {
        let valid = !tz.is_empty() && tz.len() <= 64 && tz.chars().all(|c| c.is_ascii_alphanumeric() || "_+-/".contains(c));
        if !valid {
            return Err(ApiErr::bad("timezone must be an IANA name such as Australia/Sydney"));
        }
        n.timezone = tz;
    }
    // `gemini_api_key` is deliberately ignored: the UI keeps it in localStorage, never in State.
    *s = n;
    env.changed = true;
    Ok(settings_json(env))
}

fn export_import(env: &mut Env, b: &Value) -> Res<Value> {
    if !b.is_object() || !b.get("version").is_some_and(Value::is_number) {
        return Err(ApiErr::bad("this does not look like a backup file (missing version)"));
    }
    let mut st: State = parse(b)?;
    if st.version == 0 || st.version > STATE_VERSION {
        return Err(ApiErr::bad(format!("unsupported backup version {}", st.version)));
    }
    st.next_id = st.next_id.max(st.max_id() + 1);
    if !st.gym.rotation.items.is_empty() {
        st.gym.rotation.next_index %= st.gym.rotation.items.len();
    } else {
        st.gym.rotation.next_index = 0;
    }
    *env.st = st;
    env.changed = true;
    Ok(serde_json::to_value(&*env.st).unwrap_or(Value::Null))
}

fn handle(env: &mut Env, method: &str, path: &str, q: &HashMap<String, String>, body: &Value) -> Res<Value> {
    let p = path.strip_prefix("/api").unwrap_or(path);
    let segs: Vec<&str> = p.trim_matches('/').split('/').filter(|s| !s.is_empty()).collect();
    match (method, segs.as_slice()) {
        // settings
        ("GET", ["settings"]) => Ok(settings_json(env)),
        ("PUT", ["settings"]) => put_settings(env, body),
        // categories
        ("GET", ["categories"]) => ok(&env.st.categories),
        ("POST", ["categories"]) => ok(tasks::create_category(env, parse(body)?)?),
        ("PUT", ["categories", id]) => ok(tasks::update_category(env, id_of(id)?, parse(body)?)?),
        ("DELETE", ["categories", id]) => tasks::delete_category(env, id_of(id)?),
        // tasks
        ("GET", ["tasks"]) => Ok(tasks::list_tasks(env)),
        ("POST", ["tasks"]) => ok(tasks::create_task(env, parse(body)?)?),
        ("GET", ["tasks", id]) => ok(tasks::get_task(env, id_of(id)?)?),
        ("PUT", ["tasks", id]) => ok(tasks::update_task(env, id_of(id)?, parse(body)?)?),
        ("DELETE", ["tasks", id]) => tasks::delete_task(env, id_of(id)?),
        ("GET", ["tasks", id, "categorise-prompt"]) => tasks::categorise_prompt(env, id_of(id)?),
        ("PUT", ["tasks", id, "category"]) => ok(tasks::set_category(env, id_of(id)?, body)?),
        // days and occurrences
        ("GET", ["days"]) => tasks::days(env, q),
        ("POST", ["occurrences", "toggle"]) => ok(tasks::toggle(env, parse(body)?)?),
        ("POST", ["occurrences", "checklist"]) => ok(tasks::toggle_checklist(env, parse(body)?)?),
        // gym
        ("GET", ["exercises"]) => Ok(gym::list_exercises(env)),
        ("POST", ["exercises"]) => ok(gym::create_exercise(env, parse(body)?)?),
        ("PUT", ["exercises", id]) => ok(gym::update_exercise(env, id_of(id)?, parse(body)?)?),
        ("DELETE", ["exercises", id]) => gym::delete_exercise(env, id_of(id)?),
        ("GET", ["exercises", id, "history"]) => ok(gym::exercise_history(env.st, id_of(id)?)?),
        ("GET", ["templates"]) => ok(&env.st.gym.templates),
        ("POST", ["templates"]) => ok(gym::create_template(env, parse(body)?)?),
        ("PUT", ["templates", id]) => ok(gym::update_template(env, id_of(id)?, parse(body)?)?),
        ("DELETE", ["templates", id]) => gym::delete_template(env, id_of(id)?),
        ("GET", ["rotation"]) => ok(gym::get_rotation(env.st)),
        ("PUT", ["rotation"]) => ok(gym::put_rotation(env, parse(body)?)?),
        ("GET", ["gym", "next"]) => ok(gym::next(env.st)),
        ("POST", ["gym", "rest"]) => ok(gym::rest_taken(env)?),
        ("GET", ["sessions"]) => Ok(gym::session_list(env.st, q)),
        ("POST", ["sessions"]) => ok(gym::session_create(env, parse(body)?)?),
        ("GET", ["sessions", id]) => ok(gym::session_get(env.st, id_of(id)?)?),
        ("PUT", ["sessions", id, "sets"]) => ok(gym::session_sets(env, id_of(id)?, parse(body)?)?),
        ("POST", ["sessions", id, "complete"]) => ok(gym::session_complete(env, id_of(id)?)?),
        // goals
        ("GET", ["goals"]) => Ok(goals::list(env)),
        ("POST", ["goals"]) => ok(goals::create(env, parse(body)?)?),
        ("PUT", ["goals", id]) => ok(goals::update(env, id_of(id)?, parse(body)?)?),
        ("DELETE", ["goals", id]) => goals::delete(env, id_of(id)?),
        // memory
        ("GET", ["memory"]) => ok(&env.st.memory),
        ("PUT", ["memory", "weather"]) => ok(memory::put_weather(env, parse(body)?)?),
        ("PUT", ["memory", "work"]) => ok(memory::put_work(env, parse(body)?)?),
        ("PUT", ["memory", "rhythm"]) => ok(memory::put_rhythm(env, parse(body)?)?),
        ("POST", ["memory", "notes"]) => ok(memory::add_note(env, parse(body)?)?),
        ("PUT", ["memory", "notes", id]) => ok(memory::update_note(env, id_of(id)?, parse(body)?)?),
        ("DELETE", ["memory", "notes", id]) => ok(memory::delete_note(env, id_of(id)?)?),
        ("GET", ["days-off"]) => ok(&env.st.days_off),
        ("POST", ["days-off"]) => memory::days_off_add(env, body),
        ("DELETE", ["days-off", date]) => Ok(memory::days_off_delete(env, date)),
        // prepare variants of the AI endpoints
        ("GET", ["briefing"]) => {
            let kind = briefing::parse_kind(q.get("kind").map(String::as_str))?;
            let date = date_param(env, q, "date")?;
            briefing::briefing(env, kind, date)
        }
        ("GET", ["digest"]) => {
            let date = date_param(env, q, "date")?;
            digest::digest(env, date)
        }
        ("GET", ["day-note"]) => {
            let date = date_param(env, q, "date")?;
            daynote::day_note(env, date)
        }
        // helpers
        ("POST", ["holidays", "filter"]) => {
            let region = body["region"].as_str().unwrap_or("").trim().to_string();
            let list: Vec<Value> = holidays::filter(&region, &body["raw"]).into_iter().map(|(d, n)| json!({"date": d, "name": n})).collect();
            Ok(json!(list))
        }
        ("GET", ["weather-summary"]) => {
            let code: i64 = q.get("code").and_then(|c| c.trim().parse::<f64>().ok()).map(|c| c as i64).ok_or_else(|| ApiErr::bad("code must be a number"))?;
            Ok(json!({"summary": weather::wmo_summary(code), "icon": weather::wmo_icon(code)}))
        }
        ("POST", ["sample-data"]) => Ok(seed::sample_data(env)),
        ("GET", ["export"]) => Ok(serde_json::to_value(&*env.st).unwrap_or(Value::Null)),
        ("POST", ["import"]) => export_import(env, body),
        _ => Err(ApiErr::not_found("no such endpoint")),
    }
}

/// Endpoints that never look at the date context.
fn needs_today(method: &str, path: &str) -> bool {
    let p = path.strip_prefix("/api").unwrap_or(path).trim_matches('/');
    !matches!((method, p), ("GET", "weather-summary") | ("POST", "holidays/filter") | ("GET", "export") | ("POST", "import") | ("GET", "settings") | ("GET", "categories") | ("GET", "memory"))
}

/// Typed entry point; [`crate::call`] wraps it into a JSON string.
pub fn call_typed(state: &str, method: &str, path: &str, body: &str, ctx: &str) -> Response {
    let method = method.to_ascii_uppercase();
    let (path_only, query) = path.split_once('?').unwrap_or((path, ""));
    let q = parse_query(query);
    let body_v: Value = if body.trim().is_empty() {
        json!({})
    } else {
        match serde_json::from_str(body) {
            Ok(v) => v,
            Err(e) => return err_response(ApiErr::bad(format!("invalid JSON body: {e}"))),
        }
    };
    let ctx_v: Ctx = if ctx.trim().is_empty() {
        Ctx::default()
    } else {
        match serde_json::from_str(ctx) {
            Ok(c) => c,
            Err(e) => return err_response(ApiErr::bad(format!("invalid ctx: {e}"))),
        }
    };
    // import replaces the state wholesale, so an unreadable current state must not block it
    let importing = method == "POST" && path_only.trim_end_matches('/').ends_with("/import");
    let mut st: State = if importing && serde_json::from_str::<State>(state).is_err() {
        State::default()
    } else {
        match serde_json::from_str(state) {
            Ok(s) => s,
            Err(e) => return err_response(ApiErr::bad(format!("invalid state: {e}"))),
        }
    };
    let today = match parse_date(&ctx_v.today) {
        Some(d) => d,
        None if !needs_today(&method, path_only) => NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
        None => return err_response(ApiErr::bad("ctx.today must be YYYY-MM-DD")),
    };
    let mut env = Env { st: &mut st, ctx: &ctx_v, today, changed: false };
    match handle(&mut env, &method, path_only, &q, &body_v) {
        Ok(v) => {
            let changed = env.changed;
            Response { status: 200, body: v, state: changed.then_some(st) }
        }
        Err(e) => err_response(e),
    }
}
