//! Day note: a short, grounded, specific note about one day (the home screen "affirmation").
//! Gemini when a key is set (cached per distinct day plan), otherwise a template built from the data.
use crate::briefing;
use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, Res};
use crate::tasks::{self, Occurrence};
use crate::{settings, AppState};
use axum::extract::{Query, State};
use axum::Json;
use chrono::{Datelike, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashMap;

pub const CREATE_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS day_notes (date TEXT NOT NULL, plan_hash TEXT NOT NULL, text TEXT NOT NULL, source TEXT NOT NULL, PRIMARY KEY (date, plan_hash));";

/// Stable FNV-1a hash of the plan (title, time, part_of_day); deliberately ignores done state.
pub fn plan_hash(items: &[Occurrence]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    let mut feed = |s: &str| {
        for b in s.bytes().chain(std::iter::once(0x1f)) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    for o in items {
        feed(&o.title);
        feed(o.time.as_deref().unwrap_or(""));
        feed(&o.part_of_day);
        feed("\n");
    }
    format!("{h:016x}")
}

fn cache_get(conn: &Connection, date: &str, hash: &str) -> Option<String> {
    conn.query_row("SELECT text FROM day_notes WHERE date=?1 AND plan_hash=?2", params![date, hash], |r| r.get(0)).optional().ok().flatten()
}

fn cache_put(conn: &Connection, date: &str, hash: &str, text: &str) {
    if let Err(e) = conn.execute(
        "INSERT INTO day_notes(date,plan_hash,text,source) VALUES(?1,?2,?3,'gemini') ON CONFLICT(date,plan_hash) DO UPDATE SET text=excluded.text",
        params![date, hash, text],
    ) {
        tracing::warn!("could not cache day note: {e}");
    }
}

// ---------------------------------------------------------------- template

const PARTS: [&str; 3] = ["morning", "afternoon", "evening"];

/// Which part of the day an item sits in: by clock time if timed, else its part_of_day.
fn part_idx(o: &Occurrence) -> Option<usize> {
    if let Some(t) = &o.time {
        let h: u32 = t.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(12);
        return Some(if h < 12 { 0 } else if h < 17 { 1 } else { 2 });
    }
    PARTS.iter().position(|p| *p == o.part_of_day)
}

fn num(n: usize) -> String {
    const W: [&str; 13] = ["no", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve"];
    W.get(n).map(|s| s.to_string()).unwrap_or_else(|| n.to_string())
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn say_time(t: &str) -> String {
    let h: u32 = t.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(0);
    let m: u32 = t.get(3..5).and_then(|x| x.parse().ok()).unwrap_or(0);
    let (h12, ap) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    if m == 0 { format!("{h12} {ap}") } else { format!("{h12}:{m:02} {ap}") }
}

fn phrase(o: &Occurrence) -> String {
    match &o.time {
        Some(t) => format!("{} at {}", o.title, say_time(t)),
        None => o.title.clone(),
    }
}

/// The task to name first: earliest timed undone one, else first undone morning one, else first undone, else first.
pub fn key_task(items: &[Occurrence]) -> Option<&Occurrence> {
    let undone = || items.iter().filter(|o| !o.done);
    undone()
        .filter(|o| o.time.is_some())
        .min_by(|a, b| a.time.cmp(&b.time))
        .or_else(|| undone().find(|o| part_idx(o) == Some(0)))
        .or_else(|| undone().next())
        .or_else(|| items.first())
}

fn second_task<'a>(items: &'a [Occurrence], key: &Occurrence) -> Option<&'a Occurrence> {
    items.iter().filter(|o| !o.done && !(o.task_id == key.task_id && o.date == key.date)).min_by(|a, b| {
        // timed first (by time), then list order
        match (&a.time, &b.time) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    })
}

fn idx(d: NaiveDate, salt: u64, len: usize) -> usize {
    let h = (d.num_days_from_ce() as u64).wrapping_add(salt.wrapping_mul(7919)).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((h >> 33) as usize) % len
}

pub fn template_note(items: &[Occurrence], d: NaiveDate) -> String {
    let total = items.len();
    let done = items.iter().filter(|o| o.done).count();
    let v = |opts: Vec<String>| -> String { opts[idx(d, total as u64 + 3, opts.len())].clone() };

    if total == 0 {
        return v(vec![
            "Nothing scheduled today. Use the space for something you keep putting off, or leave it empty on purpose.".into(),
            "Nothing scheduled today. A good day to get ahead on one small thing, or to have an unhurried one.".into(),
            "Nothing scheduled today. Pick one small thing you have been avoiding and let that be the whole agenda.".into(),
        ]);
    }
    let key = key_task(items).unwrap();
    if done == total {
        let n = if total == 1 { "the one thing on the list is".to_string() } else { format!("all {} things are", num(total)) };
        return v(vec![
            format!("Everything is already done: {n} ticked off, {} included. The rest of the day is yours.", key.title),
            format!("The list is clear, with {n} done, including {}. Anything else today is a bonus.", key.title),
        ]);
    }

    let mut parts = [0usize; 3];
    let mut anytime = 0usize;
    for o in items {
        match part_idx(o) {
            Some(i) => parts[i] += 1,
            None => anytime += 1,
        }
    }
    let kp = phrase(key);
    let second = second_task(items, key);
    let then_second = second.map(|s| format!(" Then {}.", phrase(s))).unwrap_or_default();
    let n = num(total);
    // the first part of the day with nothing in it, for a breather
    let gap = PARTS.iter().enumerate().find(|(i, _)| parts[*i] == 0).map(|(_, p)| *p);
    let (busiest, bmax) = parts.iter().enumerate().max_by_key(|(i, c)| (**c, std::cmp::Reverse(*i))).map(|(i, c)| (i, *c)).unwrap();

    if total >= 6 {
        let where_ = if anytime > bmax { "most of it has no set time".to_string() } else { format!("the {} carries the most", PARTS[busiest]) };
        return v(vec![
            format!("A packed day: {n} things, and {where_}. Start with {kp}, and take a proper break between stretches rather than pushing straight through."),
            format!("{} things on the list today, and {where_}. Begin with {kp}, then pace it: a real break halfway beats a rushed finish.", cap(&n)),
        ]);
    }
    if total <= 2 {
        return if total == 1 {
            v(vec![
                format!("A light day with just one thing: {kp}. Get it out of the way early and the rest of the day is yours."),
                format!("Only one thing on the list today, {kp}. Do it when it suits you and spend the spare time on something you chose."),
            ])
        } else {
            let s = second.map(|s| s.title.clone()).unwrap_or_default();
            v(vec![
                format!("A light day with two things, {} and {s}. Plenty of slack, so leave room for a proper break.", key.title),
                format!("Just two things today: {kp}, then {s}. That leaves most of the day free, so use it deliberately."),
            ])
        };
    }
    if bmax >= 3 && bmax * 2 >= total {
        return match busiest {
            0 => {
                let tail = if parts[1] + parts[2] + anytime == 0 { "after that the day is clear" } else { "after that it eases off" };
                v(vec![
                    format!("{} things land in the morning, and {tail}. Start with {kp} and the rest of the day gets lighter.", cap(&num(bmax))),
                    format!("The morning is the busy stretch, with {} things in it; {tail}. Begin with {kp} and keep the afternoon for yourself.", num(bmax)),
                ])
            }
            1 => {
                let lead = if parts[0] == 0 {
                    "The morning is quiet, so use it to get ahead.".to_string()
                } else {
                    let m = items.iter().find(|o| !o.done && part_idx(o) == Some(0)).unwrap_or(key);
                    format!("Get {} done in the morning so the afternoon only has to be the afternoon.", m.title)
                };
                v(vec![
                    format!("The afternoon is where the day gets busy, with {} things including {kp}. {lead}", num(bmax)),
                    format!("{} things stack up in the afternoon, {kp} among them. {lead}", cap(&num(bmax))),
                ])
            }
            _ => v(vec![
                format!("The evening carries the load, with {} things including {kp}. The daytime is lighter, so move anything flexible earlier.", num(bmax)),
                format!("{} things wait until the evening, {kp} among them. Do what you can earlier in the day so tonight isn't a scramble.", cap(&num(bmax))),
            ]),
        };
    }
    // moderate, spread out
    let breather = match gap {
        Some(p) => format!("The {p} is empty, which makes it the natural place for a breather."),
        None => "Leave a gap between items for a breather.".into(),
    };
    v(vec![
        format!("{} things today, spread across the day. Start with {kp}.{then_second} {breather}", cap(&n)),
        format!("A steady day of {n} things, nothing crowded. {kp} is the anchor, so begin there. {breather}"),
    ])
}

// ------------------------------------------------------------------ gemini

const SYSTEM: &str = "You write a single short note shown on the home screen of a personal planner app, about ONE specific day. \
Output 1 to 2 sentences, at most 40 words, plain text. No markdown, no emojis, no quotation marks, no lists. \
Be grounded, specific and practical, warm but not soppy. It must reflect the real shape of the day using only the facts given: \
how many things there are, when the busy stretch is, the key task (name it), and where there is a breather or slack. Offer one practical angle, such as what to do first. \
Never use clichés or generic affirmations like 'you've got this', 'believe in yourself', 'one step at a time' or 'you are enough'. \
Do not mention the gym or workouts. Do not invent tasks, times or numbers. Output only the note.";

fn gemini_context(items: &[Occurrence], d: NaiveDate) -> String {
    let mut s = format!("Date: {}, {}\nNumber of items: {}\nItems in order:\n", tasks::weekday_name(d), d.format("%B %-d"), items.len());
    let mut parts = [0usize; 3];
    for o in items {
        let when = match &o.time {
            Some(t) => format!("at {}", say_time(t)),
            None => format!("no set time, {}", o.part_of_day),
        };
        if let Some(i) = part_idx(o) {
            parts[i] += 1;
        }
        s.push_str(&format!("  - {} ({when})\n", o.title));
    }
    s.push_str(&format!("Items per part of day: morning {}, afternoon {}, evening {}\n", parts[0], parts[1], parts[2]));
    if let Some(k) = key_task(items) {
        s.push_str(&format!("Suggested key task to name: {}\n", phrase(k)));
    }
    s
}

fn acceptable(t: &str) -> bool {
    let w = t.split_whitespace().count();
    (6..=70).contains(&w)
}

// ----------------------------------------------------------------- service

/// The day note for `date`. `use_ai=false` never calls Gemini (a cached Gemini note is still used).
pub async fn note_for(st: &AppState, date: NaiveDate, use_ai: bool) -> Res<(String, &'static str)> {
    let ds = fmt_date(date);
    let (items, key) = {
        let conn = st.db.lock().unwrap();
        let mut days = tasks::build_days(&conn, date, 1)?;
        (days.remove(0).items, settings::gemini_key(&conn))
    };
    let all_done = !items.is_empty() && items.iter().all(|o| o.done);
    if items.is_empty() || all_done {
        return Ok((template_note(&items, date), "template"));
    }
    let hash = plan_hash(&items);
    {
        let conn = st.db.lock().unwrap();
        if let Some(t) = cache_get(&conn, &ds, &hash) {
            return Ok((t, "gemini"));
        }
    }
    if let (true, Some(key)) = (use_ai, key) {
        match briefing::gemini(st, &key, SYSTEM, &gemini_context(&items, date)).await {
            Ok(t) if acceptable(&t) => {
                let conn = st.db.lock().unwrap();
                cache_put(&conn, &ds, &hash, &t);
                return Ok((t, "gemini"));
            }
            Ok(t) => tracing::warn!("gemini day note rejected ({} words), using template", t.split_whitespace().count()),
            Err(e) => tracing::warn!("gemini day note failed, using template: {e}"),
        }
    }
    Ok((template_note(&items, date), "template"))
}

#[derive(Serialize)]
pub struct DayNoteOut {
    date: String,
    text: String,
    source: &'static str,
}

pub async fn day_note(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Res<Json<DayNoteOut>> {
    let date = match q.get("date") {
        Some(s) => parse_date(s).ok_or_else(|| AppError::bad("date must be YYYY-MM-DD"))?,
        None => today(),
    };
    let use_ai = q.get("ai").map(|v| v != "false" && v != "0").unwrap_or(true);
    let (text, source) = note_for(&st, date, use_ai).await?;
    Ok(Json(DayNoteOut { date: fmt_date(date), text, source }))
}
