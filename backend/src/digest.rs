//! Home-screen digest: a weather comment (forecast read through the saved weather preferences)
//! and an overview of the day in prose. Gemini when a key is set (cached), template otherwise.
use crate::briefing::{self, item_phrase, join, pick};
use crate::daynote::{self, cap, part_idx, PARTS};
use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, Res};
use crate::goals::{self, Goal};
use crate::gym::WorkoutPlan;
use crate::memory::{self, Memory};
use crate::tasks::{self, DayInfo, Occurrence};
use crate::weather::{self, Weather};
use crate::{settings, AppState};
use axum::extract::{Query, State};
use axum::Json;
use chrono::{Datelike, NaiveDate};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

pub const CREATE_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS digests (date TEXT NOT NULL, hash TEXT NOT NULL, weather_comment TEXT, overview TEXT NOT NULL, PRIMARY KEY (date, hash));";

pub fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// A warm one-liner about the work situation of a day, only when it is notable or useful.
pub fn frame_sentence(info: &DayInfo) -> Option<String> {
    if let Some(h) = &info.holiday {
        return Some(format!("It's a public holiday, {h}, so no work today."));
    }
    if info.day_off {
        return Some("It's a day off, so no work today.".into());
    }
    info.work.as_ref().map(|w| format!("Work runs from {} to {} today.", briefing::say_time(&w.start), briefing::say_time(&w.end)))
}

/// Line for Gemini contexts describing the work situation.
pub fn context_line(info: &DayInfo, label: &str) -> String {
    if let Some(h) = &info.holiday {
        format!("{label} work: none, it is a public holiday ({h}); mention it warmly, e.g. no work today\n")
    } else if info.day_off {
        format!("{label} work: none, it is a day off; mention it warmly\n")
    } else if let Some(w) = &info.work {
        format!(
            "{label} work: {} to {} (a fixed block, not a task). Frame the day as before work (the window between waking and the start), during work and after work; suggest admin or quick tasks before the start or after the end, and errands in the lunch break. Never schedule other things inside work hours.\n",
            w.start, w.end
        )
    } else {
        format!("{label} work: none (not a work day)\n")
    }
}

// ------------------------------------------------------------- weather comment

#[derive(PartialEq, Clone, Copy)]
enum Feel {
    Snow,
    Rain,
    TooHot,
    Hot,
    Warm,
    Comfy,
    Cold,
}

fn is_rainy(w: &Weather) -> bool {
    let s = w.summary.to_lowercase();
    ["rain", "drizzle", "shower", "thunder"].iter().any(|k| s.contains(k)) || w.precip_prob.is_some_and(|p| p >= 50)
}

fn feel(w: &Weather, p: &memory::WeatherPrefs) -> Feel {
    let t = w.temp_max as f64;
    if w.summary.to_lowercase().contains("snow") {
        Feel::Snow
    } else if is_rainy(w) {
        Feel::Rain
    } else if t > p.too_hot_above {
        Feel::TooHot
    } else if t > p.hot_above {
        Feel::Hot
    } else if t > p.comfortable_max {
        Feel::Warm
    } else if t <= 6.0 {
        Feel::Cold
    } else {
        Feel::Comfy
    }
}

enum Plans {
    Office,
    Desk,
    Outdoor(String),
    Gym,
    Off,
    None,
}

fn has_word(text: &str, words: &[&str]) -> bool {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|t| words.iter().any(|w| t == *w || (w.len() >= 5 && t.starts_with(w) && t.len() <= w.len() + 2)))
}

/// What the day looks like, but only where a memory note or a task actually suggests it.
fn plans(items: &[Occurrence], mem: &Memory, workout: &Option<WorkoutPlan>, info: &DayInfo) -> Plans {
    if let Some(o) = items.iter().find(|o| !o.done && has_word(&o.title, &["walk", "run", "park", "hike", "grocery", "groceries", "shopping", "errand", "errands", "picnic", "bbq", "barbecue"])) {
        return Plans::Outdoor(o.title.clone());
    }
    if info.work.is_some() {
        return if mem.notes.iter().any(|n| has_word(&n.text, &["office"])) { Plans::Office } else { Plans::Desk };
    }
    if info.holiday.is_some() || info.day_off {
        return Plans::Off;
    }
    if items.iter().any(|o| has_word(&o.title, &["work", "stand", "standup", "meeting", "report"])) {
        return Plans::Desk;
    }
    if workout.as_ref().is_some_and(|p| !p.rest) {
        return Plans::Gym;
    }
    Plans::None
}

pub fn weather_comment_template(w: &Weather, mem: &Memory, items: &[Occurrence], workout: &Option<WorkoutPlan>, info: &DayInfo, d: NaiveDate) -> String {
    let p = &mem.weather;
    let f = feel(w, p);
    let t = w.temp_max;
    let sunny_uv = w.uv_max.is_some_and(|u| u >= 3.0) && p.avoid_sunburn && !is_rainy(w);
    let pl = plans(items, mem, workout, info);
    let v = |o: Vec<String>| -> String { o[daynote::idx(d, 101 + t as u64, o.len())].clone() };
    let first = match f {
        Feel::Snow => v(vec![format!("Snow in the forecast, {t} degrees at most."), format!("It's a snowy one today, topping out at {t} degrees.")]),
        Feel::Rain if p.likes_rain => v(vec![
            format!("A proper rainy one, {t} degrees at most, and I know you don't mind that."),
            format!("Rain today, {t} degrees at best, which suits you just fine."),
            format!("Wet and grey at {t} degrees, the kind of weather you like."),
        ]),
        Feel::Rain => v(vec![
            format!("A wet one today, {t} degrees at most, so keep an umbrella close."),
            format!("Rain on the way and {t} degrees at best, so not the nicest of days outside."),
        ]),
        Feel::TooHot => v(vec![
            format!("It's a hot one, {t} degrees, which is past what you find comfortable. Keep water close and take it slowly in the afternoon."),
            format!("Properly hot today at {t} degrees, more than you enjoy, so go easy on yourself and keep cool."),
        ]),
        Feel::Hot => v(vec![
            format!("Warm and getting hot, {t} degrees at the top, so lighter clothes and plenty of water."),
            format!("A hot-ish {t} degrees today, a touch more than you like, so dress light."),
        ]),
        Feel::Warm => v(vec![
            format!("A mild, slightly warm {t} degrees, just above your sweet spot but easy to enjoy."),
            format!("Pleasantly warm at {t} degrees, a notch warmer than you like but nothing to fuss about."),
        ]),
        Feel::Comfy => v(vec![
            format!("Lovely and cool, {t} degrees at most, right in your comfort zone."),
            format!("A comfortable {t} degrees today, exactly the kind of cool you like."),
            format!("Nice weather for you, {t} degrees at most, cool and easy."),
        ]),
        Feel::Cold => v(vec![format!("A chilly one, {t} degrees at best, so layer up."), format!("Properly cold today, just {t} degrees at the top, so wrap up well.")]),
    };
    let second: Option<String> = if sunny_uv {
        Some(match f {
            Feel::TooHot | Feel::Hot => "The sun is strong too, so sun cream before you head out and shade where you can.".to_string(),
            _ => v(vec!["The UV is up, so sun cream if you'll be outside for a while.".into(), "Sun cream is worth it today, since the UV gets high.".into()]),
        })
    } else {
        match (&pl, f) {
            (Plans::Outdoor(t), Feel::Rain | Feel::Snow) => Some(format!("Maybe plan {t} around the weather, or take a proper coat.")),
            (Plans::Outdoor(t), Feel::TooHot | Feel::Hot) => Some(format!("Do {t} early or late, away from the heat.")),
            (Plans::Outdoor(t), _) => Some(format!("A good backdrop for {t}.")),
            (Plans::Office, Feel::Rain) if p.likes_rain => Some("Nice to listen to from your desk, though take an umbrella for the commute.".into()),
            (Plans::Office, Feel::Rain | Feel::Snow) => Some("Take an umbrella and a coat for the commute.".into()),
            (Plans::Office, Feel::TooHot | Feel::Hot) => Some("Sounds like an office day, so keep a bottle of water on your desk.".into()),
            (Plans::Office, _) => Some("Easy weather for the office commute.".into()),
            (Plans::Desk, Feel::Rain) if p.likes_rain => Some("A cosy backdrop for the work you have on.".into()),
            (Plans::Desk, Feel::TooHot | Feel::Hot) => Some("Keep cool while you get through your work.".into()),
            (Plans::Off, Feel::Rain | Feel::Snow) => Some("A lovely excuse for a slow day off.".into()),
            (Plans::Off, Feel::TooHot | Feel::Hot) => Some("Good day off for staying cool and doing very little.".into()),
            (Plans::Off, _) => Some("A nice backdrop for a day off.".into()),
            (Plans::Gym, Feel::TooHot | Feel::Hot) => Some("Drink plenty before the gym.".into()),
            (Plans::Gym, Feel::Rain) => Some("At least the rain gives the gym an easy excuse to be the main outing.".into()),
            (_, Feel::Rain) if p.likes_rain => Some("Cosy if you're home, and nice to listen to from the window.".into()),
            _ => None,
        }
    };
    match second {
        Some(s) => format!("{first} {s}"),
        None => first,
    }
}

// ---------------------------------------------------------------- overview

fn minutes(t: &str) -> u32 {
    let h: u32 = t.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(12);
    let m: u32 = t.get(3..5).and_then(|x| x.parse().ok()).unwrap_or(0);
    h * 60 + m
}

/// Bucket of the day: 0 = early, 1 = middle, 2 = late. On work days that is before / during / after work.
fn bucket(o: &Occurrence, info: &DayInfo) -> Option<usize> {
    if let (Some(w), Some(t)) = (&info.work, &o.time) {
        let m = minutes(t);
        return Some(if m < minutes(&w.start) { 0 } else if m < minutes(&w.end) { 1 } else { 2 });
    }
    part_idx(o)
}

pub fn overview_template(items: &[Occurrence], goals: &[Goal], workout: &Option<WorkoutPlan>, info: &DayInfo, d: NaiveDate) -> String {
    let pick_ = |o: &[&'static str], salt: u64| -> &'static str { pick(o, d, salt) };
    let frame = if info.holiday.is_some() || info.day_off { frame_sentence(info) } else { None };
    let lead = |s: String| -> String {
        match &frame {
            Some(f) => format!("{f} {s}"),
            None => s,
        }
    };
    if items.is_empty() {
        let mut s = if info.work.is_some() {
            "Nothing is planned beyond work, so before and after it are yours. A good moment for something you keep putting off, or for properly switching off.".to_string()
        } else {
            "Nothing is planned today, so it's a wide-open day. Use it for something you keep putting off, or leave it empty on purpose.".to_string()
        };
        if let Some(w) = workout.as_ref().filter(|w| !w.rest) {
            s.push_str(&format!(" The gym has {} lined up if you fancy it.", w.name));
        }
        return lead(s);
    }
    let undone: Vec<&Occurrence> = items.iter().filter(|o| !o.done).collect();
    if undone.is_empty() {
        return lead("Everything on the list is already ticked off, so the rest of the day is yours to enjoy.".into());
    }
    let mut order: Vec<&Occurrence> = undone.clone();
    order.sort_by(|a, b| match (&a.time, &b.time) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });
    let named: Vec<&Occurrence> = order.iter().take(4).cloned().collect();
    let is_named = |o: &Occurrence| named.iter().any(|n| n.task_id == o.task_id && n.date == o.date);
    let mut parts: Vec<(Vec<String>, bool)> = vec![(vec![], false); 3];
    let mut anytime: (Vec<String>, bool) = (vec![], false);
    for o in &undone {
        let slot = match bucket(o, info) {
            Some(i) => &mut parts[i],
            None => &mut anytime,
        };
        if is_named(o) {
            slot.0.push(item_phrase(o));
        } else {
            slot.1 = true;
        }
    }
    let phr = |p: &(Vec<String>, bool)| -> String {
        let mut s = join(&p.0);
        if p.1 {
            s = if s.is_empty() { "a few smaller bits".into() } else { format!("{s}, plus a few smaller bits") };
        }
        s
    };
    let empty = |i: usize| parts[i].0.is_empty() && !parts[i].1;
    let worky = info.work.is_some();
    let mut clauses: Vec<String> = vec![];
    let mut breather: Option<String> = None;
    for i in 0..3 {
        if worky {
            let (busy, free) = match i {
                0 => (format!("before work there's {}", phr(&parts[0])), "the time before work is free".to_string()),
                1 => (format!("during work hours there's {}, ideally at lunch", phr(&parts[1])), "work fills the middle of the day".to_string()),
                _ => (format!("after work there's {}", phr(&parts[2])), "after work is open".to_string()),
            };
            if empty(i) {
                clauses.push(free);
                if i != 1 && breather.is_none() {
                    breather = Some(if i == 0 { "the time before work".to_string() } else { "the time after work".to_string() });
                }
            } else {
                clauses.push(busy);
            }
        } else if empty(i) {
            clauses.push(format!("the {} is open", PARTS[i]));
            if breather.is_none() {
                breather = Some(format!("the {}", PARTS[i]));
            }
        } else {
            let verbs = [["is about", "is for", "centres on"], ["is about", "has", "holds"], ["has", "holds", "is for"]];
            clauses.push(format!("the {} {} {}", PARTS[i], verbs[i][(d.num_days_from_ce() as usize + i) % 3], phr(&parts[i])));
        }
    }
    let mut out: Vec<String> = vec![];
    if (0..3).all(empty) {
        out.push(lead(format!("Nothing is pinned to a particular time today: {} can go wherever suits you.", phr(&anytime))));
    } else {
        let mut s = cap(&format!("{}, {}, and {}.", clauses[0], clauses[1], clauses[2]));
        if !(anytime.0.is_empty() && !anytime.1) {
            s.push_str(&format!(" {} can slot in whenever there's a gap.", cap(&phr(&anytime))));
        }
        out.push(lead(s));
    }
    out.push(match breather {
        Some(b) => format!("That open stretch, {b}, is your breather, so protect it."),
        None => pick_(&["There's no big gap, so build a proper break in around the middle.", "It's evenly spread, so a short breather between stretches will help."], 3).to_string(),
    });
    let texts: Vec<(String, String)> = undone.iter().map(|o| (o.title.clone(), o.title.clone())).collect();
    if let Some((g, task)) = goals::find_link(goals, &texts) {
        let gr = goals::goal_ref(g);
        out.push(match d.num_days_from_ce() % 3 {
            0 => format!("{task} is a small step toward {gr}."),
            1 => format!("{task} feeds into {gr}, which is nice."),
            _ => format!("{task} counts toward {gr}."),
        });
    }
    if let Some(w) = workout.as_ref().filter(|w| !w.rest) {
        if out.len() < 4 {
            out.push(format!("{} is on at the gym too.", w.name));
        }
    }
    out.join(" ")
}

// ------------------------------------------------------------------ gemini

const SYSTEM: &str = "You write two short texts for the home screen of a personal planner app, about ONE specific day, in the voice of a warm, chatty friend who knows the person. \
Reply with ONLY a JSON object: {\"weather_comment\": string or null, \"overview\": string}. No markdown, no code fences, no emojis. \
weather_comment: 1 to 2 sentences interpreting the forecast through the person's weather preferences (cool is comfortable if so; warn kindly when hot or too hot; suggest sun cream or shade when UV is high and they want to avoid sunburn; enjoy rain if they like rain) and, where natural, relating it to the day's plans (an office day versus a home day, outdoor tasks, the gym). Temperatures may be said as degrees (e.g. 14 degrees), but never quote the rain probability as a percentage. Set it to null if no weather is given. Only mention an office or home context if a note or task suggests it. \
overview: 2 to 4 short sentences in plain prose organising the day: its shape (what the morning is about, the afternoon, the evening; on a work day, before work, during work and after work, with work as a fixed block that is not a task; on a public holiday or day off, say warmly that there is no work), naming the key things, pointing out a breather, tying to a goal only where it plausibly relates. NEVER state raw counts, fractions or percentages (no '5 tasks', '3 of 4', '50 percent'); small natural quantities like 'a couple of things' or 'a few smaller bits' are fine. Never a bare list of every task: name at most four things and summarise the rest. You may mention the workout briefly. No cliches such as 'you've got this'. Do not invent tasks, times or numbers.";

fn context(items: &[Occurrence], d: NaiveDate, goals: &[Goal], mem: &Memory, w: &Option<Weather>, workout: &Option<WorkoutPlan>, info: &DayInfo) -> String {
    let mut s = format!("Date: {}, {}\n", tasks::weekday_name(d), d.format("%B %-d"));
    match w {
        Some(w) => {
            s.push_str(&format!("Weather: {}, high {} C, low {} C", w.summary, w.temp_max, w.temp_min));
            if let Some(p) = w.precip_prob {
                s.push_str(&format!(", rain chance {} (do not quote)", if p >= 70 { "high" } else if p >= 40 { "moderate" } else { "low" }));
            }
            if let Some(u) = w.uv_max {
                s.push_str(&format!(", UV index {u}"));
            }
            s.push('\n');
        }
        None => s.push_str("Weather: unavailable (set weather_comment to null)\n"),
    }
    s.push_str(&context_line(info, "Today's"));
    s.push_str("The day's items in order:\n");
    if items.is_empty() {
        s.push_str("  (nothing scheduled)\n");
    }
    for o in items {
        let when = match &o.time {
            Some(t) => format!("at {}", briefing::say_time(t)),
            None => format!("no set time, {}", o.part_of_day),
        };
        s.push_str(&format!("  - {} ({when}; done: {})\n", o.title, if o.done { "yes" } else { "no" }));
    }
    match workout {
        Some(p) if !p.rest => s.push_str(&format!("Workout today: {}\n", p.name)),
        Some(_) => s.push_str("Workout today: rest day\n"),
        None => s.push_str("Workout today: none\n"),
    }
    s.push_str(&memory::context_block(mem));
    s.push_str(&goals::context_block(goals, &[], d));
    s
}

/// Pull the JSON object out of a model reply (tolerates fences or chatter around it).
fn parse_reply(raw: &str) -> Option<(Option<String>, String)> {
    let a = raw.find('{')?;
    let b = raw.rfind('}')?;
    let v: Value = serde_json::from_str(raw.get(a..=b)?).ok()?;
    let norm = |x: &str| x.split_whitespace().collect::<Vec<_>>().join(" ");
    let overview = norm(v.get("overview")?.as_str()?);
    let comment = v.get("weather_comment").and_then(Value::as_str).map(norm).filter(|s| !s.is_empty());
    Some((comment, overview))
}

fn words(s: &str) -> usize {
    s.split_whitespace().count()
}

async fn gemini_digest(st: &AppState, key: &str, ctx: &str, has_weather: bool) -> Result<(Option<String>, String), String> {
    let mut system = SYSTEM.to_string();
    for attempt in 0..2 {
        let raw = briefing::gemini_raw(st, key, &system, ctx, 0.9, 20).await?;
        let Some((comment, overview)) = parse_reply(&raw) else {
            tracing::warn!("digest reply was not parseable JSON");
            continue;
        };
        let comment = if has_weather { comment } else { None };
        if !(8..=90).contains(&words(&overview)) || (has_weather && comment.is_none()) {
            tracing::warn!("digest reply rejected (length/missing comment)");
            continue;
        }
        // the number guard applies to the overview only; temperatures in the weather comment are fine
        let bad_comment = comment.as_ref().is_some_and(|c| c.contains('%') || c.to_lowercase().contains("percent") || words(c) > 60);
        if briefing::has_stats(&overview) || bad_comment {
            if attempt == 0 {
                system.push_str("\nSTRICT REMINDER: your previous draft read out a count, fraction or percentage. Do NOT write '3 of 5', 'percent' or a percent sign anywhere; name specific things and use words like 'a couple' or 'a few'.");
            }
            continue;
        }
        return Ok((comment, overview));
    }
    Err("gemini digest unusable".into())
}

// ----------------------------------------------------------------- handler

#[derive(Serialize)]
pub struct DigestOut {
    date: String,
    weather: Option<Weather>,
    weather_comment: Option<String>,
    overview: String,
    source: &'static str,
}

pub async fn digest(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Res<Json<DigestOut>> {
    let date = match q.get("date") {
        Some(s) => parse_date(s).ok_or_else(|| AppError::bad("date must be YYYY-MM-DD"))?,
        None => today(),
    };
    let use_ai = q.get("ai").map(|v| v != "false" && v != "0").unwrap_or(true);
    let ds = fmt_date(date);
    let years = tasks::years_between(date, date);
    crate::holidays::ensure(&st, &years).await;
    let (items, workout, info, goals, mem, key, lat, lon) = {
        let conn = st.db.lock().unwrap();
        let mut days = tasks::build_days(&conn, date, 1)?;
        let day = days.remove(0);
        let s = settings::load(&conn)?;
        (day.items.clone(), day.workout.clone(), day.info(), goals::list_active(&conn)?, memory::load(&conn)?, settings::gemini_key(&conn), s.latitude, s.longitude)
    };
    let weather = weather::for_date(&st, lat, lon, &ds).await;
    let wsig = weather.as_ref().map(|w| format!("{}|{}|{}|{:?}|{:?}", w.summary, w.temp_max, w.temp_min, w.precip_prob, w.uv_max)).unwrap_or_default();
    let isig = format!("{:?}", info);
    let wname = workout.as_ref().map(|p| if p.rest { "rest".to_string() } else { p.name.clone() }).unwrap_or_default();
    let hash = fnv(&format!("{}#{}#{}#{}#{}#{}", daynote::plan_hash(&items), isig, goals::fingerprint(&goals, date), memory::fingerprint(&mem), wsig, wname));

    let cached: Option<(Option<String>, String)> = {
        let conn = st.db.lock().unwrap();
        conn.query_row("SELECT weather_comment, overview FROM digests WHERE date=?1 AND hash=?2", params![ds, hash], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .ok()
            .flatten()
    };
    if let Some((weather_comment, overview)) = cached {
        return Ok(Json(DigestOut { date: ds, weather, weather_comment, overview, source: "gemini" }));
    }
    if let (true, Some(key)) = (use_ai, key) {
        match gemini_digest(&st, &key, &context(&items, date, &goals, &mem, &weather, &workout, &info), weather.is_some()).await {
            Ok((weather_comment, overview)) => {
                let conn = st.db.lock().unwrap();
                if let Err(e) = conn.execute(
                    "INSERT INTO digests(date,hash,weather_comment,overview) VALUES(?1,?2,?3,?4) ON CONFLICT(date,hash) DO UPDATE SET weather_comment=excluded.weather_comment, overview=excluded.overview",
                    params![ds, hash, weather_comment, overview],
                ) {
                    tracing::warn!("could not cache digest: {e}");
                }
                return Ok(Json(DigestOut { date: ds, weather, weather_comment, overview, source: "gemini" }));
            }
            Err(e) => tracing::warn!("gemini digest failed, using template: {e}"),
        }
    }
    let weather_comment = weather.as_ref().map(|w| weather_comment_template(w, &mem, &items, &workout, &info, date));
    let overview = overview_template(&items, &goals, &workout, &info, date);
    Ok(Json(DigestOut { date: ds, weather, weather_comment, overview, source: "template" }))
}

