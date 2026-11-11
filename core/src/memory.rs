//! Assistant memory: weather preferences, work, rhythm, free-text notes, days off.
use crate::env::Env;
use crate::error::{ApiErr, Res};
use crate::model::*;
use crate::util::*;
use serde::Deserialize;
use serde_json::{json, Value};

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

const DAY_NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// Lines describing the memory for a Gemini context.
pub fn context_block(m: &Memory) -> String {
    let w = &m.weather;
    let mut s = String::from("What you know about this person. This is private background that silently shapes your tone, advice and suggestions. Never say it back to them or explain your reasoning from it (no 'which is too hot for you', 'the weather you like', 'your comfort zone', 'since you work 10 to 5'); just act on it naturally, the way a good assistant who knows them would:\n");
    s.push_str(&format!(
        "  - Weather preferences: cool up to {} C is comfortable; they find it hot above {} C and too hot above {} C; {}; {}.\n",
        w.comfortable_max,
        w.hot_above,
        w.too_hot_above,
        if w.likes_rain { "they like rain" } else { "they do not especially like rain" },
        if w.avoid_sunburn { "they want to avoid sunburn (suggest sun cream or shade when UV is high)" } else { "sunburn is not a particular concern" },
    ));
    s.push_str(&format!(
        "  - Temperature outranks rain: if the day's high is above {} C it is hot for them (too hot above {} C), even when it rains; never call a hot rainy day pleasant or the kind of weather they like (say muggy, sticky, too warm instead).\n",
        w.hot_above, w.too_hot_above,
    ));
    let k = &m.work;
    if k.enabled {
        s.push_str(&format!(
            "  - Works {} to {} on {} (weekday work hours; not a task).\n",
            k.start,
            k.end,
            k.days.iter().map(|d| DAY_NAMES[(*d as usize) % 7]).collect::<Vec<_>>().join(", ")
        ));
        for (wd, h) in &k.weekday_hours {
            if let Ok(i) = wd.parse::<usize>() {
                s.push_str(&format!("  - On {}s they work {} to {} instead.\n", DAY_NAMES[i % 7], h.start, h.end));
            }
        }
        for r in &k.monthly_rules {
            let which = if r.nth == -1 { "last".to_string() } else { format!("#{}", r.nth) };
            s.push_str(&format!("  - On the {} {} of each month they work {} to {} instead.\n", which, DAY_NAMES[(r.weekday as usize) % 7], r.start, r.end));
        }
        s.push_str(&format!("  - Lunch break on work days: a flexible lunch break of about {} minutes, usually sometime {}. This is private background: never state or recite these times. If a short task would fit, suggest it softly and optionally, such as \"if you get a moment in the middle of the day, X would fit nicely\" or \"X could fit in around lunch if you have time\" (good for a quick call, a bill, booking an appointment or a quick shop); never an instruction like \"do X at lunch\", and never assume a fixed time).\n", k.lunch_minutes, lunch_window(&k.lunch_window_start, &k.lunch_window_end)));
    }
    let r = &m.rhythm;
    s.push_str(&format!("  - Usually wakes between {} and {} and goes to sleep between {} and {}; keep evening suggestions finishing comfortably before bedtime, and nudge toward winding down in time for sleep in the evening.\n", r.wake_from, r.wake_to, r.sleep_from, r.sleep_to));
    if !m.interests.trim().is_empty() {
        s.push_str(&format!("  - Interests (things they love or want to learn about; private background, never recite the list, only let it quietly shape suggestions and recommendations. Only suggest specific works, films, books, shows, music, games or places that are on their leisure list; never introduce new titles of your own, and never volunteer content in Chinese, Korean or Japanese unless it is on that list): {}\n", m.interests.trim()));
    }
    s.push_str("  - Well-read: knows history and politics well. Never over-explain or define basics, no potted history lessons; be straightforward and substantive about what happened and why it matters, adding a sharp bit of context only when it genuinely adds something. Delivery stays relaxed and chatty.\n");
    for n in &m.notes {
        s.push_str(&format!("  - Note: {}\n", n.text));
    }
    s
}

// ---------------------------------------------------------------- handlers

fn temp(name: &str, v: f64) -> Res<f64> {
    if !v.is_finite() || !(-30.0..=60.0).contains(&v) {
        return Err(ApiErr::bad(format!("{name} must be a number between -30 and 60")));
    }
    Ok(v)
}

fn note_text(t: &str) -> Res<String> {
    let t = t.trim();
    if t.is_empty() {
        return Err(ApiErr::bad("text cannot be empty"));
    }
    if t.chars().count() > 500 {
        return Err(ApiErr::bad("text must be at most 500 characters"));
    }
    Ok(t.to_string())
}

fn hm(name: &str, v: &str) -> Res<u32> {
    let bad = || ApiErr::bad(format!("{name} must be HH:MM"));
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
pub struct WeatherIn {
    pub comfortable_max: Option<f64>,
    pub hot_above: Option<f64>,
    pub too_hot_above: Option<f64>,
    pub likes_rain: Option<bool>,
    pub avoid_sunburn: Option<bool>,
}

pub fn put_weather(env: &mut Env, b: WeatherIn) -> Res<Memory> {
    let cur = env.st.memory.weather.clone();
    let w = WeatherPrefs {
        comfortable_max: temp("comfortable_max", b.comfortable_max.unwrap_or(cur.comfortable_max))?,
        hot_above: temp("hot_above", b.hot_above.unwrap_or(cur.hot_above))?,
        too_hot_above: temp("too_hot_above", b.too_hot_above.unwrap_or(cur.too_hot_above))?,
        likes_rain: b.likes_rain.unwrap_or(cur.likes_rain),
        avoid_sunburn: b.avoid_sunburn.unwrap_or(cur.avoid_sunburn),
    };
    if !(w.comfortable_max <= w.hot_above && w.hot_above <= w.too_hot_above) {
        return Err(ApiErr::bad("thresholds must satisfy comfortable_max <= hot_above <= too_hot_above"));
    }
    env.st.memory.weather = w;
    env.changed = true;
    Ok(env.st.memory.clone())
}

#[derive(Deserialize)]
pub struct WorkIn {
    enabled: Option<bool>,
    days: Option<Vec<u32>>,
    start: Option<String>,
    end: Option<String>,
    holiday_region: Option<String>,
    lunch_minutes: Option<u32>,
    lunch_window_start: Option<String>,
    lunch_window_end: Option<String>,
    weekday_hours: Option<std::collections::BTreeMap<String, HourSpan>>,
    monthly_rules: Option<Vec<MonthlyRule>>,
}

pub fn put_work(env: &mut Env, b: WorkIn) -> Res<Memory> {
    let cur = env.st.memory.work.clone();
    let mut w = WorkPrefs {
        enabled: b.enabled.unwrap_or(cur.enabled),
        days: b.days.unwrap_or(cur.days),
        start: b.start.unwrap_or(cur.start),
        end: b.end.unwrap_or(cur.end),
        holiday_region: b.holiday_region.map(|s| s.trim().to_string()).unwrap_or(cur.holiday_region),
        lunch_minutes: b.lunch_minutes.unwrap_or(cur.lunch_minutes),
        lunch_window_start: b.lunch_window_start.unwrap_or(cur.lunch_window_start),
        lunch_window_end: b.lunch_window_end.unwrap_or(cur.lunch_window_end),
        weekday_hours: b.weekday_hours.unwrap_or(cur.weekday_hours),
        monthly_rules: b.monthly_rules.unwrap_or(cur.monthly_rules),
    };
    for (k, h) in &w.weekday_hours {
        if !matches!(k.parse::<u32>(), Ok(0..=6)) || k.len() != 1 {
            return Err(ApiErr::bad("weekday_hours keys must be \"0\" (Monday) to \"6\" (Sunday)"));
        }
        if hm("weekday_hours start", &h.start)? >= hm("weekday_hours end", &h.end)? {
            return Err(ApiErr::bad("weekday_hours start must be before end"));
        }
    }
    for r in &w.monthly_rules {
        if !(1..=5).contains(&r.nth) && r.nth != -1 {
            return Err(ApiErr::bad("monthly_rules nth must be 1 to 5, or -1 for the last"));
        }
        if r.weekday > 6 {
            return Err(ApiErr::bad("monthly_rules weekday must be 0 (Monday) to 6 (Sunday)"));
        }
        if hm("monthly_rules start", &r.start)? >= hm("monthly_rules end", &r.end)? {
            return Err(ApiErr::bad("monthly_rules start must be before end"));
        }
    }
    if w.lunch_minutes > 240 {
        return Err(ApiErr::bad("lunch_minutes must be between 0 and 240"));
    }
    if w.days.iter().any(|d| *d > 6) {
        return Err(ApiErr::bad("days must be numbers 0 (Monday) to 6 (Sunday)"));
    }
    w.days.sort_unstable();
    w.days.dedup();
    if hm("start", &w.start)? >= hm("end", &w.end)? {
        return Err(ApiErr::bad("start must be before end"));
    }
    if hm("lunch_window_start", &w.lunch_window_start)? >= hm("lunch_window_end", &w.lunch_window_end)? {
        return Err(ApiErr::bad("lunch_window_start must be before lunch_window_end"));
    }
    if w.holiday_region.len() > 20 || !w.holiday_region.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(ApiErr::bad("holiday_region must look like AU-NSW (or empty for none)"));
    }
    env.st.memory.work = w;
    env.changed = true;
    Ok(env.st.memory.clone())
}

#[derive(Deserialize)]
pub struct RhythmIn {
    wake_from: Option<String>,
    wake_to: Option<String>,
    sleep_from: Option<String>,
    sleep_to: Option<String>,
}

pub fn put_rhythm(env: &mut Env, b: RhythmIn) -> Res<Memory> {
    let cur = env.st.memory.rhythm.clone();
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
        return Err(ApiErr::bad("wake_from must not be after wake_to"));
    }
    env.st.memory.rhythm = r;
    env.changed = true;
    Ok(env.st.memory.clone())
}

#[derive(Deserialize)]
pub struct InterestsIn {
    text: String,
}

pub fn put_interests(env: &mut Env, b: InterestsIn) -> Res<Memory> {
    let t = b.text.trim();
    if t.chars().count() > 6000 {
        return Err(ApiErr::bad("text must be at most 6000 characters"));
    }
    env.st.memory.interests = t.to_string();
    env.changed = true;
    Ok(env.st.memory.clone())
}

#[derive(Deserialize)]
pub struct NoteIn {
    text: String,
}

pub fn add_note(env: &mut Env, b: NoteIn) -> Res<Memory> {
    let text = note_text(&b.text)?;
    let id = env.st.new_id();
    env.st.memory.notes.push(Note { id, text });
    env.changed = true;
    Ok(env.st.memory.clone())
}

pub fn update_note(env: &mut Env, id: i64, b: NoteIn) -> Res<Memory> {
    let text = note_text(&b.text)?;
    let n = env.st.memory.notes.iter_mut().find(|n| n.id == id).ok_or_else(|| ApiErr::not_found("note not found"))?;
    n.text = text;
    env.changed = true;
    Ok(env.st.memory.clone())
}

pub fn delete_note(env: &mut Env, id: i64) -> Res<Memory> {
    let before = env.st.memory.notes.len();
    env.st.memory.notes.retain(|n| n.id != id);
    if env.st.memory.notes.len() == before {
        return Err(ApiErr::not_found("note not found"));
    }
    env.changed = true;
    Ok(env.st.memory.clone())
}

pub fn days_off_add(env: &mut Env, b: &Value) -> Res<Value> {
    let raw = b["date"].as_str().unwrap_or("").trim();
    let d = parse_date(raw).ok_or_else(|| ApiErr::bad("date must be YYYY-MM-DD"))?;
    let ds = fmt_date(d);
    if !env.st.days_off.contains(&ds) {
        env.st.days_off.push(ds);
        env.st.days_off.sort();
        env.changed = true;
    }
    Ok(json!(env.st.days_off))
}

pub fn days_off_delete(env: &mut Env, date: &str) -> Value {
    let n = env.st.days_off.len();
    env.st.days_off.retain(|x| x != date);
    if env.st.days_off.len() != n {
        env.changed = true;
    }
    json!(env.st.days_off)
}

// ---------------------------------------------------------------- private profile

#[derive(Deserialize)]
pub struct ProfileGoal {
    title: String,
    horizon: String,
    #[serde(default)]
    why: Option<String>,
    #[serde(default)]
    deadline: Option<String>,
}

#[derive(Deserialize)]
pub struct ProfileLeisure {
    kind: String,
    title: String,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    status: Option<String>,
}

#[derive(Deserialize)]
pub struct ProfileTask {
    title: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    part_of_day: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    recurrence: Option<Recurrence>,
    #[serde(default)]
    checklist: Vec<String>,
}

#[derive(Deserialize)]
pub struct ProfileIn {
    v: u32,
    updated: String,
    #[serde(default)]
    interests: Option<String>,
    #[serde(default)]
    notes: Option<Vec<String>>,
    #[serde(default)]
    work: Option<WorkIn>,
    #[serde(default)]
    weather: Option<WeatherIn>,
    #[serde(default)]
    add_goals: Vec<ProfileGoal>,
    #[serde(default)]
    add_leisure: Vec<ProfileLeisure>,
    #[serde(default)]
    add_tasks: Vec<ProfileTask>,
}

/// Applies a private profile. All-or-nothing: on any validation error the state is left untouched.
pub fn profile_apply(env: &mut Env, b: ProfileIn) -> Res<Value> {
    if b.v != 1 {
        return Err(ApiErr::bad("unsupported profile version"));
    }
    let updated = parse_date(b.updated.trim()).map(fmt_date).ok_or_else(|| ApiErr::bad("updated must be YYYY-MM-DD"))?;
    let backup = env.st.clone();
    let was_changed = env.changed;
    let r = apply_inner(env, b, updated);
    if r.is_err() {
        *env.st = backup;
        env.changed = was_changed;
    }
    r
}

fn apply_inner(env: &mut Env, b: ProfileIn, updated: String) -> Res<Value> {
    let has_interests = b.interests.is_some();
    if let Some(t) = b.interests {
        put_interests(env, InterestsIn { text: t })?;
    }
    let mut notes = 0;
    if let Some(list) = b.notes {
        let texts = list.iter().map(|t| note_text(t)).collect::<Res<Vec<_>>>()?;
        env.st.memory.notes.clear();
        for text in texts {
            let id = env.st.new_id();
            env.st.memory.notes.push(Note { id, text });
            notes += 1;
        }
    }
    let has_work = b.work.is_some();
    if let Some(w) = b.work {
        put_work(env, w)?;
    }
    let has_weather = b.weather.is_some();
    if let Some(w) = b.weather {
        put_weather(env, w)?;
    }
    let mut goals_added = 0;
    for g in b.add_goals {
        let dup = |t: &str| env.st.goals.iter().any(|x| x.title.trim().to_lowercase() == t.trim().to_lowercase());
        if dup(&g.title) {
            continue;
        }
        crate::goals::create(env, crate::goals::GoalIn { title: g.title, horizon: g.horizon, deadline: g.deadline, why: g.why, status: None, achieved_on: None })?;
        goals_added += 1;
    }
    let mut leisure_added = 0;
    for l in b.add_leisure {
        if env.st.leisure.iter().any(|x| x.title.trim().to_lowercase() == l.title.trim().to_lowercase()) {
            continue;
        }
        crate::leisure::create(env, crate::leisure::LeisureIn { kind: l.kind, title: l.title, note: l.note, status: l.status, finished_on: None, blurb: None })?;
        leisure_added += 1;
    }
    let mut tasks_added = 0;
    for t in b.add_tasks {
        let title = t.title.trim().to_lowercase();
        let date = t.date.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
        if env.st.tasks.iter().any(|x| x.title.trim().to_lowercase() == title && x.date == date) {
            continue;
        }
        let category_id = match t.category.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(c) if c.eq_ignore_ascii_case("auto") => Some(json!("auto")),
            Some(c) => {
                let cat = env.st.categories.iter().find(|x| x.name.trim().eq_ignore_ascii_case(c)).ok_or_else(|| ApiErr::bad(format!("unknown category \"{c}\"")))?;
                Some(json!(cat.id))
            }
        };
        let tin = crate::tasks::TaskIn {
            title: t.title,
            date,
            time: t.time,
            part_of_day: t.part_of_day,
            category_id,
            notes: t.notes.unwrap_or_default(),
            recurrence: t.recurrence,
            checklist: t.checklist.into_iter().map(|text| crate::tasks::ChecklistIn { id: None, text }).collect(),
            minutes: None,
        };
        crate::tasks::create_task(env, tin)?;
        tasks_added += 1;
    }
    env.st.memory.profile_updated = updated;
    env.changed = true;
    Ok(json!({"interests": has_interests, "notes": notes, "work": has_work, "weather": has_weather, "goals_added": goals_added, "leisure_added": leisure_added, "tasks_added": tasks_added}))
}

/// Clears interests, notes, work hours (back to defaults), weather preferences (defaults) and the profile date.
pub fn profile_clear(env: &mut Env) -> Value {
    let m = &mut env.st.memory;
    m.interests = String::new();
    m.notes.clear();
    m.work = WorkPrefs::default();
    m.weather = WeatherPrefs::default();
    m.profile_updated = String::new();
    env.changed = true;
    json!({"cleared": true})
}
