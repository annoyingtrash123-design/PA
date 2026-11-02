//! Assistant memory: weather preferences and free-text notes about the user.
//! Stored as JSON in the settings table; absent keys mean the defaults.
use crate::db;
use crate::error::{AppError, AppJson, Res};
use crate::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

const WEATHER_KEY: &str = "memory_weather";
const WORK_KEY: &str = "memory_work";
const DAYS_OFF_KEY: &str = "days_off";
const RHYTHM_KEY: &str = "memory_rhythm";
const NOTES_KEY: &str = "memory_notes";
const NEXT_ID_KEY: &str = "memory_next_id";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WeatherPrefs {
    pub comfortable_max: f64,
    pub hot_above: f64,
    pub too_hot_above: f64,
    pub likes_rain: bool,
    pub avoid_sunburn: bool,
}

impl Default for WeatherPrefs {
    fn default() -> Self {
        WeatherPrefs { comfortable_max: 18.0, hot_above: 22.0, too_hot_above: 25.0, likes_rain: true, avoid_sunburn: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    pub id: i64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkPrefs {
    pub enabled: bool,
    /// 0 = Monday ... 6 = Sunday
    pub days: Vec<u32>,
    pub start: String,
    pub end: String,
    pub holiday_region: String,
    /// Length of the (flexible-time) lunch break on work days, minutes.
    #[serde(default = "default_lunch")]
    pub lunch_minutes: u32,
}

fn default_lunch() -> u32 {
    60
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rhythm {
    pub wake_from: String,
    pub wake_to: String,
    pub sleep_from: String,
    pub sleep_to: String,
}

impl Default for Rhythm {
    fn default() -> Self {
        Rhythm { wake_from: "06:00".into(), wake_to: "07:00".into(), sleep_from: "23:00".into(), sleep_to: "00:00".into() }
    }
}

impl Default for WorkPrefs {
    fn default() -> Self {
        WorkPrefs { enabled: true, days: vec![0, 1, 2, 3, 4], start: "10:00".into(), end: "17:00".into(), holiday_region: "AU-NSW".into(), lunch_minutes: 60 }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct Memory {
    pub weather: WeatherPrefs,
    pub work: WorkPrefs,
    pub rhythm: Rhythm,
    pub notes: Vec<Note>,
}

fn default_notes() -> Vec<Note> {
    vec![Note { id: 1, text: "I work from an office most weekdays".into() }]
}

pub fn load(conn: &Connection) -> rusqlite::Result<Memory> {
    let weather = db::get_setting(conn, WEATHER_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let notes = db::get_setting(conn, NOTES_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(default_notes);
    let work = db::get_setting(conn, WORK_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    let rhythm = db::get_setting(conn, RHYTHM_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    Ok(Memory { weather, work, rhythm, notes })
}

fn save_notes(conn: &Connection, notes: &[Note]) -> rusqlite::Result<()> {
    db::set_setting(conn, NOTES_KEY, &serde_json::to_string(notes).unwrap_or_else(|_| "[]".into()))
}

/// Stable text used in cache keys.
pub fn fingerprint(m: &Memory) -> String {
    let w = &m.weather;
    let mut s = format!("{}|{}|{}|{}|{}", w.comfortable_max, w.hot_above, w.too_hot_above, w.likes_rain, w.avoid_sunburn);
    s.push_str(&format!("\nwork {:?} {:?}", m.work, m.rhythm));
    for n in &m.notes {
        s.push('\n');
        s.push_str(&n.text);
    }
    s
}

/// Lines describing the memory for a Gemini context.
pub fn context_block(m: &Memory) -> String {
    let w = &m.weather;
    let mut s = String::from("What you know about this person (use as background; weave in naturally only where it fits, never recite it):\n");
    s.push_str(&format!(
        "  - Weather preferences: cool up to {} C is comfortable; they find it hot above {} C and too hot above {} C; {}; {}.\n",
        w.comfortable_max,
        w.hot_above,
        w.too_hot_above,
        if w.likes_rain { "they like rain" } else { "they do not especially like rain" },
        if w.avoid_sunburn { "they want to avoid sunburn (suggest sun cream or shade when UV is high)" } else { "sunburn is not a particular concern" },
    ));
    let k = &m.work;
    if k.enabled {
        s.push_str(&format!("  - Works {} to {} on {} (weekday work hours; not a task).\n", k.start, k.end, k.days.iter().map(|d| ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][(*d as usize) % 7]).collect::<Vec<_>>().join(", ")));
    }
    if k.enabled {
        s.push_str(&format!("  - Lunch break on work days: about {} minutes, at a flexible time (good for a quick call, a bill, booking an appointment or a quick shop; never assume a fixed time).\n", k.lunch_minutes));
    }
    let r = &m.rhythm;
    s.push_str(&format!("  - Usually wakes between {} and {} and goes to sleep between {} and {}; keep evening suggestions finishing comfortably before bedtime, and nudge toward winding down in time for sleep in the evening.\n", r.wake_from, r.wake_to, r.sleep_from, r.sleep_to));
    for n in &m.notes {
        s.push_str(&format!("  - Note: {}\n", n.text));
    }
    s
}

// ---------------------------------------------------------------- handlers

#[derive(Deserialize)]
pub struct WeatherIn {
    comfortable_max: Option<f64>,
    hot_above: Option<f64>,
    too_hot_above: Option<f64>,
    likes_rain: Option<bool>,
    avoid_sunburn: Option<bool>,
}

#[derive(Deserialize)]
pub struct NoteIn {
    text: String,
}

fn temp(name: &str, v: f64) -> Res<f64> {
    if !v.is_finite() || !(-30.0..=60.0).contains(&v) {
        return Err(AppError::bad(format!("{name} must be a number between -30 and 60")));
    }
    Ok(v)
}

fn note_text(t: &str) -> Res<String> {
    let t = t.trim();
    if t.is_empty() {
        return Err(AppError::bad("text cannot be empty"));
    }
    if t.chars().count() > 500 {
        return Err(AppError::bad("text must be at most 500 characters"));
    }
    Ok(t.to_string())
}

pub async fn get(State(st): State<AppState>) -> Res<Json<Memory>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(load(&conn)?))
}

pub async fn put_weather(State(st): State<AppState>, AppJson(b): AppJson<WeatherIn>) -> Res<Json<Memory>> {
    let conn = st.db.lock().unwrap();
    let cur = load(&conn)?.weather;
    let w = WeatherPrefs {
        comfortable_max: temp("comfortable_max", b.comfortable_max.unwrap_or(cur.comfortable_max))?,
        hot_above: temp("hot_above", b.hot_above.unwrap_or(cur.hot_above))?,
        too_hot_above: temp("too_hot_above", b.too_hot_above.unwrap_or(cur.too_hot_above))?,
        likes_rain: b.likes_rain.unwrap_or(cur.likes_rain),
        avoid_sunburn: b.avoid_sunburn.unwrap_or(cur.avoid_sunburn),
    };
    if !(w.comfortable_max <= w.hot_above && w.hot_above <= w.too_hot_above) {
        return Err(AppError::bad("thresholds must satisfy comfortable_max <= hot_above <= too_hot_above"));
    }
    db::set_setting(&conn, WEATHER_KEY, &serde_json::to_string(&w).unwrap())?;
    Ok(Json(load(&conn)?))
}

pub async fn add_note(State(st): State<AppState>, AppJson(b): AppJson<NoteIn>) -> Res<Json<Memory>> {
    let text = note_text(&b.text)?;
    let conn = st.db.lock().unwrap();
    let mut notes = load(&conn)?.notes;
    let stored: Option<i64> = db::get_setting(&conn, NEXT_ID_KEY)?.and_then(|s| s.parse().ok());
    let id = stored.unwrap_or(0).max(notes.iter().map(|n| n.id).max().unwrap_or(0) + 1);
    notes.push(Note { id, text });
    save_notes(&conn, &notes)?;
    db::set_setting(&conn, NEXT_ID_KEY, &(id + 1).to_string())?;
    Ok(Json(load(&conn)?))
}

pub async fn update_note(State(st): State<AppState>, Path(id): Path<i64>, AppJson(b): AppJson<NoteIn>) -> Res<Json<Memory>> {
    let text = note_text(&b.text)?;
    let conn = st.db.lock().unwrap();
    let mut notes = load(&conn)?.notes;
    let n = notes.iter_mut().find(|n| n.id == id).ok_or_else(|| AppError::not_found("note not found"))?;
    n.text = text;
    save_notes(&conn, &notes)?;
    Ok(Json(load(&conn)?))
}

pub async fn delete_note(State(st): State<AppState>, Path(id): Path<i64>) -> Res<Json<Memory>> {
    let conn = st.db.lock().unwrap();
    let mut notes = load(&conn)?.notes;
    let before = notes.len();
    notes.retain(|n| n.id != id);
    if notes.len() == before {
        return Err(AppError::not_found("note not found"));
    }
    save_notes(&conn, &notes)?;
    Ok(Json(load(&conn)?))
}

// ------------------------------------------------------------ work + days off

fn hm(name: &str, v: &str) -> Res<u32> {
    let bad = || AppError::bad(format!("{name} must be HH:MM"));
    let (h, m) = v.split_once(':').ok_or_else(bad)?;
    if h.len() != 2 || m.len() != 2 {
        return Err(bad());
    }
    let (h, m): (u32, u32) = (h.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?);
    if h > 23 || m > 59 {
        return Err(bad());
    }
    Ok(h * 60 + m)
}

#[derive(Deserialize)]
pub struct WorkIn {
    enabled: Option<bool>,
    days: Option<Vec<u32>>,
    start: Option<String>,
    end: Option<String>,
    holiday_region: Option<String>,
    lunch_minutes: Option<u32>,
}

pub async fn put_work(State(st): State<AppState>, AppJson(b): AppJson<WorkIn>) -> Res<Json<Memory>> {
    let conn = st.db.lock().unwrap();
    let cur = load(&conn)?.work;
    let mut w = WorkPrefs {
        enabled: b.enabled.unwrap_or(cur.enabled),
        days: b.days.unwrap_or(cur.days),
        start: b.start.unwrap_or(cur.start),
        end: b.end.unwrap_or(cur.end),
        holiday_region: b.holiday_region.map(|s| s.trim().to_string()).unwrap_or(cur.holiday_region),
        lunch_minutes: b.lunch_minutes.unwrap_or(cur.lunch_minutes),
    };
    if w.lunch_minutes > 240 {
        return Err(AppError::bad("lunch_minutes must be between 0 and 240"));
    }
    if w.days.iter().any(|d| *d > 6) {
        return Err(AppError::bad("days must be numbers 0 (Monday) to 6 (Sunday)"));
    }
    w.days.sort_unstable();
    w.days.dedup();
    if hm("start", &w.start)? >= hm("end", &w.end)? {
        return Err(AppError::bad("start must be before end"));
    }
    if w.holiday_region.len() > 20 || !w.holiday_region.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::bad("holiday_region must look like AU-NSW (or empty for none)"));
    }
    db::set_setting(&conn, WORK_KEY, &serde_json::to_string(&w).unwrap())?;
    Ok(Json(load(&conn)?))
}

pub fn days_off(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    Ok(db::get_setting(conn, DAYS_OFF_KEY)?.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default())
}

fn save_days_off(conn: &Connection, v: &[String]) -> rusqlite::Result<()> {
    db::set_setting(conn, DAYS_OFF_KEY, &serde_json::to_string(v).unwrap())
}

pub async fn days_off_get(State(st): State<AppState>) -> Res<Json<Vec<String>>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(days_off(&conn)?))
}

#[derive(Deserialize)]
pub struct DateIn {
    date: String,
}

pub async fn days_off_add(State(st): State<AppState>, AppJson(b): AppJson<DateIn>) -> Res<Json<Vec<String>>> {
    let d = db::parse_date(b.date.trim()).ok_or_else(|| AppError::bad("date must be YYYY-MM-DD"))?;
    let conn = st.db.lock().unwrap();
    let mut v = days_off(&conn)?;
    let ds = db::fmt_date(d);
    if !v.contains(&ds) {
        v.push(ds);
        v.sort();
        save_days_off(&conn, &v)?;
    }
    Ok(Json(v))
}

pub async fn days_off_delete(State(st): State<AppState>, Path(date): Path<String>) -> Res<Json<Vec<String>>> {
    let conn = st.db.lock().unwrap();
    let mut v = days_off(&conn)?;
    v.retain(|x| *x != date);
    save_days_off(&conn, &v)?;
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct RhythmIn {
    wake_from: Option<String>,
    wake_to: Option<String>,
    sleep_from: Option<String>,
    sleep_to: Option<String>,
}

pub async fn put_rhythm(State(st): State<AppState>, AppJson(b): AppJson<RhythmIn>) -> Res<Json<Memory>> {
    let conn = st.db.lock().unwrap();
    let cur = load(&conn)?.rhythm;
    let r = Rhythm {
        wake_from: b.wake_from.unwrap_or(cur.wake_from),
        wake_to: b.wake_to.unwrap_or(cur.wake_to),
        sleep_from: b.sleep_from.unwrap_or(cur.sleep_from),
        sleep_to: b.sleep_to.unwrap_or(cur.sleep_to),
    };
    for (n, v) in [("wake_from", &r.wake_from), ("wake_to", &r.wake_to), ("sleep_from", &r.sleep_from), ("sleep_to", &r.sleep_to)] {
        hm(n, v)?;
    }
    if hm("wake_from", &r.wake_from)? > hm("wake_to", &r.wake_to)? {
        return Err(AppError::bad("wake_from must not be after wake_to"));
    }
    db::set_setting(&conn, RHYTHM_KEY, &serde_json::to_string(&r).unwrap())?;
    Ok(Json(load(&conn)?))
}
