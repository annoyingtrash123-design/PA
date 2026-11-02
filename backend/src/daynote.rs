//! Day note: a short, grounded, specific note about one day (the home screen "affirmation").
//! Gemini when a key is set (cached per distinct day plan), otherwise a template built from the data.
use crate::briefing;
use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, Res};
use crate::goals::{self, Goal};
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

pub const PARTS: [&str; 3] = ["morning", "afternoon", "evening"];

/// Which part of the day an item sits in: by clock time if timed, else its part_of_day.
pub fn part_idx(o: &Occurrence) -> Option<usize> {
    if let Some(t) = &o.time {
        let h: u32 = t.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(12);
        return Some(if h < 12 { 0 } else if h < 17 { 1 } else { 2 });
    }
    PARTS.iter().position(|p| *p == o.part_of_day)
}

/// Vague, human quantity: never a raw count.
fn qty(n: usize) -> &'static str {
    match n {
        0 => "nothing",
        1 => "one thing",
        2 => "a couple of things",
        3..=4 => "a few things",
        5..=6 => "quite a few things",
        _ => "a lot of things",
    }
}

pub fn cap(s: &str) -> String {
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

pub fn idx(d: NaiveDate, salt: u64, len: usize) -> usize {
    let h = (d.num_days_from_ce() as u64).wrapping_add(salt.wrapping_mul(7919)).wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((h >> 33) as usize) % len
}

pub fn template_note(items: &[Occurrence], d: NaiveDate, goals: &[Goal]) -> String {
    let mut t = template_core(items, d);
    // tie the day to a goal only when a pending task plausibly relates
    let texts: Vec<(String, String)> = items.iter().filter(|o| !o.done).map(|o| (o.title.clone(), o.title.clone())).collect();
    if let Some((g, task)) = goals::find_link(goals, &texts) {
        let gr = goals::goal_ref(g);
        let tail = match idx(d, 41, 3) {
            0 => format!(" {task} is a small step toward {gr}."),
            1 => format!(" Handy, since {task} feeds into {gr}."),
            _ => format!(" {task} counts toward {gr}."),
        };
        t.push_str(&tail);
    }
    t
}

fn template_core(items: &[Occurrence], d: NaiveDate) -> String {
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
        return v(vec![
            format!("Everything on the list is already ticked off, {} included. The rest of the day is yours.", key.title),
            format!("The list is clear, {} and all. Anything else today is a bonus.", key.title),
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
    // the first part of the day with nothing in it, for a breather
    let gap = PARTS.iter().enumerate().find(|(i, _)| parts[*i] == 0).map(|(_, p)| *p);
    let (busiest, bmax) = parts.iter().enumerate().max_by_key(|(i, c)| (**c, std::cmp::Reverse(*i))).map(|(i, c)| (i, *c)).unwrap();

    if total >= 6 {
        let where_ = if anytime > bmax { "most of it has no set time".to_string() } else { format!("the {} carries the most", PARTS[busiest]) };
        return v(vec![
            format!("A full day, and {where_}. The one to watch is {kp}. Take a proper break between stretches rather than pushing straight through."),
            format!("There's quite a bit on today, and {where_}. Begin with {kp}, then pace it: a real break halfway beats a rushed finish."),
        ]);
    }
    if total <= 2 {
        return if total == 1 {
            v(vec![
                format!("A light day with just one thing: {kp}. Get it out of the way early and the rest of the day is yours."),
                format!("Only {kp} on the list today. Do it when it suits you and spend the spare time on something you chose."),
            ])
        } else {
            let s = second.map(|s| s.title.clone()).unwrap_or_default();
            v(vec![
                format!("A light day, with {} and {s}. Plenty of slack, so leave room for a proper break.", key.title),
                format!("Just {kp}, then {s}. That leaves most of the day free, so use it deliberately."),
            ])
        };
    }
    if bmax >= 3 && bmax * 2 >= total {
        let q = qty(bmax);
        return match busiest {
            0 => {
                let tail = if parts[1] + parts[2] + anytime == 0 { "after that the day is clear" } else { "after that it eases off" };
                v(vec![
                    format!("{} land in the morning, and {tail}. Start with {kp} and the rest of the day gets lighter.", cap(q)),
                    format!("The morning is the busy stretch; {tail}. Begin with {kp} and keep the afternoon for yourself."),
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
                    format!("The afternoon is where the day gets busy, {kp} among it. {lead}"),
                    format!("{} stack up in the afternoon, {kp} among them. {lead}", cap(q)),
                ])
            }
            _ => v(vec![
                format!("The evening carries the load, {kp} included. The daytime is lighter, so move anything flexible earlier."),
                format!("{} wait until the evening, {kp} among them. Do what you can earlier so tonight isn't a scramble.", cap(q)),
            ]),
        };
    }
    // moderate, spread out
    let breather = match gap {
        Some(p) => format!("The {p} is empty, which makes it the natural place for a breather."),
        None => "Leave a gap between items for a breather.".into(),
    };
    v(vec![
        format!("A steady day, spread out nicely. Start with {kp}.{then_second} {breather}"),
        format!("Nothing crowded today. {kp} is the anchor, so begin there. {breather}"),
    ])
}

// ------------------------------------------------------------------ gemini

const SYSTEM: &str = "You write a single short note shown on the home screen of a personal planner app, about ONE specific day. \
Output 1 to 2 sentences, at most 40 words, plain text. No markdown, no emojis, no quotation marks, no lists. \
Sound like a chatty, warm friend who knows the person, not a report. Be grounded and specific: name the key task, say roughly where the busy stretch is, and where there is a breather or slack. \
NEVER state raw counts, fractions or percentages (no '5 tasks', 'three things', '0 of 4', '50 percent'); small natural quantities like 'a couple of things' or 'a few' are fine, but naming the actual tasks is better. \
If one of the goals given plausibly relates to a task today, you may tie them together in a few words; never force a link, and mention a goal deadline only if it is close and relevant. \
Never use clichés or generic affirmations like 'you've got this', 'believe in yourself', 'one step at a time' or 'you are enough'. \
Do not mention the gym or workouts. Do not invent tasks, times or numbers. Output only the note.";

fn gemini_context(items: &[Occurrence], d: NaiveDate, goals: &[Goal], mem: &crate::memory::Memory, info: &tasks::DayInfo) -> String {
    let mut s = format!("Date: {}, {}\n", tasks::weekday_name(d), d.format("%B %-d"));
    s.push_str(&crate::digest::context_line(info, "Today's"));
    s.push_str("Items in order:\n");
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
    let busiest = (0..3).max_by_key(|i| (parts[*i], std::cmp::Reverse(*i))).unwrap();
    s.push_str(&format!("Busiest part of the day: {} (do not quote numbers)\n", PARTS[busiest]));
    if let Some(g) = PARTS.iter().enumerate().find(|(i, _)| parts[*i] == 0) {
        s.push_str(&format!("Quiet part of the day: {}\n", g.1));
    }
    if let Some(k) = key_task(items) {
        s.push_str(&format!("Suggested key task to name: {}\n", phrase(k)));
    }
    s.push_str(&crate::memory::context_block(mem));
    s.push_str(&goals::context_block(goals, &[], d));
    s
}

/// On holidays and days off, open with a warm line about it.
fn with_frame(t: String, info: &tasks::DayInfo) -> String {
    if info.holiday.is_some() || info.day_off {
        if let Some(f) = crate::digest::frame_sentence(info) {
            return format!("{f} {t}");
        }
    }
    t
}

fn acceptable(t: &str) -> bool {
    let w = t.split_whitespace().count();
    (6..=70).contains(&w)
}

// ----------------------------------------------------------------- service

/// The day note for `date`. `use_ai=false` never calls Gemini (a cached Gemini note is still used).
pub async fn note_for(st: &AppState, date: NaiveDate, use_ai: bool) -> Res<(String, &'static str)> {
    let ds = fmt_date(date);
    crate::holidays::ensure(st, &tasks::years_between(date, date)).await;
    let (items, key, goals, mem, info) = {
        let conn = st.db.lock().unwrap();
        let mut days = tasks::build_days(&conn, date, 1)?;
        let day = days.remove(0);
        let info = day.info();
        (day.items, settings::gemini_key(&conn), goals::list_active(&conn)?, crate::memory::load(&conn)?, info)
    };
    let all_done = !items.is_empty() && items.iter().all(|o| o.done);
    if items.is_empty() || all_done {
        return Ok((with_frame(template_note(&items, date, &goals), &info), "template"));
    }
    let hash = format!("{}{:08x}", plan_hash(&items), format!("{}#{}", goals::fingerprint(&goals, date), crate::memory::fingerprint(&mem) + &format!("{:?}", info)).bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32)));
    {
        let conn = st.db.lock().unwrap();
        if let Some(t) = cache_get(&conn, &ds, &hash) {
            return Ok((t, "gemini"));
        }
    }
    if let (true, Some(key)) = (use_ai, key) {
        match briefing::gemini_guarded(st, &key, SYSTEM, &gemini_context(&items, date, &goals, &mem, &info), 0.9, 60).await {
            Ok(t) if acceptable(&t) => {
                let conn = st.db.lock().unwrap();
                cache_put(&conn, &ds, &hash, &t);
                return Ok((t, "gemini"));
            }
            Ok(t) => tracing::warn!("gemini day note rejected ({} words), using template", t.split_whitespace().count()),
            Err(e) => tracing::warn!("gemini day note failed, using template: {e}"),
        }
    }
    Ok((with_frame(template_note(&items, date, &goals), &info), "template"))
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
