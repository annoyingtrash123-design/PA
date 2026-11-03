//! Leisure list: things to watch, read, listen to, play or try when there is free time.
//! Also decides when Miranda may gently suggest one (only with genuinely free room) and how it is phrased.
use crate::daynote::is_routine;
use crate::env::Env;
use crate::error::{ApiErr, Res};
use crate::model::{Leisure, Weather};
use crate::tasks::{DayInfo, Occurrence};
use crate::util::*;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::Deserialize;
use serde_json::{json, Value};

pub const KINDS: [&str; 5] = ["watch", "read", "listen", "play", "try"];
/// Stored when the model does not know the work (the UI maps the reply "UNKNOWN" to this).
pub const UNKNOWN_BLURB: &str = "I don't know much about this one yet.";

#[derive(Deserialize)]
pub struct LeisureIn {
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub finished_on: Option<String>,
    #[serde(default)]
    pub blurb: Option<String>,
}

fn status_rank(s: &str) -> u8 {
    match s {
        "current" => 0,
        "someday" => 1,
        _ => 2,
    }
}

/// Current first, then someday, then finished (latest first); then created, id.
pub fn list_all(items: &[Leisure]) -> Vec<Leisure> {
    let mut v = items.to_vec();
    v.sort_by(|a, b| {
        status_rank(&a.status)
            .cmp(&status_rank(&b.status))
            .then(b.finished_on.cmp(&a.finished_on))
            .then(a.created.cmp(&b.created))
            .then(a.id.cmp(&b.id))
    });
    v
}

pub fn list(env: &Env) -> Value {
    json!(list_all(&env.st.leisure))
}

struct Clean {
    kind: String,
    title: String,
    note: String,
    status: String,
    finished_on: Option<String>,
    blurb: Option<String>,
}

fn clean(b: LeisureIn, today: NaiveDate) -> Res<Clean> {
    let title = b.title.trim().to_string();
    if title.is_empty() {
        return Err(ApiErr::bad("title is required"));
    }
    if title.chars().count() > 120 {
        return Err(ApiErr::bad("title must be at most 120 characters"));
    }
    if !KINDS.contains(&b.kind.as_str()) {
        return Err(ApiErr::bad("kind must be watch, read, listen, play or try"));
    }
    let status = b.status.unwrap_or_else(|| "someday".into());
    if !["someday", "current", "finished"].contains(&status.as_str()) {
        return Err(ApiErr::bad("status must be someday, current or finished"));
    }
    let note = b.note.unwrap_or_default().trim().to_string();
    if note.chars().count() > 300 {
        return Err(ApiErr::bad("note must be at most 300 characters"));
    }
    let blurb = b.blurb.map(|s| s.trim().to_string());
    if blurb.as_ref().is_some_and(|s| s.chars().count() > 700) {
        return Err(ApiErr::bad("blurb must be at most 700 characters"));
    }
    let mut finished_on = match b.finished_on.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()) {
        None => None,
        Some(s) => Some(fmt_date(parse_date(&s).ok_or_else(|| ApiErr::bad("finished_on must be YYYY-MM-DD"))?)),
    };
    if status == "finished" {
        finished_on.get_or_insert_with(|| fmt_date(today));
    } else {
        finished_on = None;
    }
    Ok(Clean { kind: b.kind, title, note, status, finished_on, blurb })
}

pub fn create(env: &mut Env, b: LeisureIn) -> Res<Leisure> {
    let c = clean(b, env.today)?;
    let l = Leisure {
        id: env.st.new_id(),
        kind: c.kind,
        title: c.title,
        note: c.note,
        status: c.status,
        created: fmt_date(env.today),
        finished_on: c.finished_on,
        last_suggested: None,
        blurb: c.blurb.unwrap_or_default(),
    };
    env.st.leisure.push(l.clone());
    env.changed = true;
    Ok(l)
}

pub fn update(env: &mut Env, id: i64, b: LeisureIn) -> Res<Leisure> {
    let c = clean(b, env.today)?;
    let today = fmt_date(env.today);
    let l = env.st.leisure.iter_mut().find(|l| l.id == id).ok_or_else(|| ApiErr::not_found("leisure item not found"))?;
    // keep the original finished date when it was already finished and none was sent
    let finished_on = if c.status == "finished" && l.status == "finished" && c.finished_on.as_deref() == Some(today.as_str()) { l.finished_on.clone().or(c.finished_on) } else { c.finished_on };
    l.kind = c.kind;
    l.title = c.title;
    l.note = c.note;
    l.status = c.status;
    l.finished_on = finished_on;
    if let Some(bl) = c.blurb {
        l.blurb = bl;
    }
    let out = l.clone();
    env.changed = true;
    Ok(out)
}

pub fn delete(env: &mut Env, id: i64) -> Res<Value> {
    let n = env.st.leisure.len();
    env.st.leisure.retain(|l| l.id != id);
    if env.st.leisure.len() == n {
        return Err(ApiErr::not_found("leisure item not found"));
    }
    env.changed = true;
    Ok(json!({"ok": true}))
}

/// `GET /api/leisure/{id}/blurb-prompt`: the prompts for ONE text request that writes a spoiler-free blurb.
pub fn blurb_prompt(env: &Env, id: i64) -> Res<Value> {
    let l = env.st.leisure.iter().find(|l| l.id == id).ok_or_else(|| ApiErr::not_found("leisure item not found"))?;
    let system = format!(
        "You write a very short, spoiler-free description of one thing a person might watch, read, listen to, play or try. {spoken}\n\
         Write 2 or 3 complete spoken sentences in plain text: no markdown, no lists, no emojis, no quotation marks around the whole text. \
         First say what it is (the genre, setting and basic premise), with NO spoilers beyond the opening premise. Then say, in a natural and warm way, why this particular person might enjoy it, given the private background about them. \
         That background is silent context: never recite or list their interests and never say 'since you like'; just connect it naturally, and only if there is a genuine link. \
         Only describe a work you are confident you know. If you are not sure you know this exact work, reply with exactly the single word UNKNOWN and nothing else. Never invent plot, authors or facts. Keep it under 70 words.",
        spoken = spoken_rule!()
    );
    let kind_word = match l.kind.as_str() {
        "watch" => "something to watch (a film or show)",
        "read" => "something to read",
        "listen" => "something to listen to (music, a podcast or an audiobook)",
        "play" => "something to play (a game)",
        _ => "something to try (a place, a recipe or an activity)",
    };
    let mut user = format!("Title: {}\nWhat it is: {kind_word}\n", l.title);
    if !l.note.is_empty() {
        user.push_str(&format!("The person's own note: {}\n", l.note));
    }
    if env.st.memory.interests.trim().is_empty() {
        user.push_str("Private background about the person: nothing known yet, so describe it neutrally.\n");
    } else {
        user.push_str(&format!("Private background about the person (silent context): {}\n", env.st.memory.interests.trim()));
    }
    Ok(json!({"id": l.id, "title": l.title, "system_prompt": system, "user_prompt": user}))
}

// ------------------------------------------------------------- free room + suggestion

fn busy_items<'a>(items: &'a [Occurrence]) -> Vec<&'a Occurrence> {
    items.iter().filter(|o| !o.done && !is_routine(o)).collect()
}

/// Is the day open enough (not just the evening)?
fn open_day(items: &[Occurrence], info: &DayInfo, date: NaiveDate) -> bool {
    let b = busy_items(items);
    if b.is_empty() {
        return true;
    }
    let weekend = matches!(date.weekday(), Weekday::Sat | Weekday::Sun) && info.work.is_none();
    if info.day_off || info.holiday.is_some() || weekend {
        let timed = b.iter().filter(|o| o.time.is_some()).count();
        return b.len() - timed <= 3 && timed <= 1;
    }
    false
}

/// Room for a leisure suggestion. Morning: an empty day, a day off / holiday / quiet weekend day.
/// Evening: most evenings qualify; only a genuinely packed one does not (2+ timed items from 18:00,
/// a timed item at 20:00 or later, or a long list still waiting).
pub fn free_room(items: &[Occurrence], info: &DayInfo, date: NaiveDate, evening: bool) -> bool {
    if !evening {
        return open_day(items, info, date);
    }
    let b = busy_items(items);
    let from_18 = b.iter().filter(|o| o.time.as_deref().is_some_and(|t| t >= "18:00")).count();
    let very_late = b.iter().any(|o| o.time.as_deref().is_some_and(|t| t >= "20:00"));
    !(from_18 >= 2 || very_late || b.len() >= 7)
}

/// Is the morning too packed for an optional "on this day" aside? (3+ timed items or 6+ things waiting.)
pub fn morning_packed(items: &[Occurrence]) -> bool {
    let b = busy_items(items);
    b.iter().filter(|o| o.time.is_some()).count() >= 3 || b.len() >= 6
}

/// Deterministic "on this day" flag: 2 mornings a week, 3 in some weeks, spread by a hash of the week.
pub fn on_this_day(date: NaiveDate, items: &[Occurrence]) -> bool {
    if morning_packed(items) {
        return false;
    }
    let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
    let a = idx(monday, 93, 4);
    let mut days = vec![a, a + 3];
    if idx(monday, 94, 2) == 0 {
        days.push((a + 5) % 7);
    }
    days.contains(&(date.weekday().num_days_from_monday() as usize))
}

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() >= 5).map(|w| w.trim_end_matches('s').to_string()).collect()
}

const STOP: [&str; 10] = ["especially", "particularly", "about", "things", "learn", "keen", "more", "love", "century", "centuries"];

/// How many meaningful words the interests share with the item (title, note, blurb).
pub fn interest_score(interests: &str, l: &Leisure) -> usize {
    let it: Vec<String> = tokens(interests).into_iter().filter(|w| !STOP.contains(&w.as_str())).collect();
    let text = tokens(&format!("{} {} {}", l.title, l.note, l.blurb));
    it.iter().filter(|w| text.contains(w)).count()
}

fn bad_weather(w: &Option<Weather>) -> bool {
    w.as_ref().is_some_and(|w| {
        let s = w.summary.to_lowercase();
        ["rain", "drizzle", "shower", "storm", "snow", "thunder"].iter().any(|k| s.contains(k)) || w.temp_max < 15 || w.precip_prob.is_some_and(|p| p >= 60)
    })
}

/// The one item (if any) Miranda may suggest for `date`. Pure: the same inputs give the same pick.
/// `current` first (rotating by date), else a `someday` item not suggested within the last 5 days; interests raise
/// priority; rainy or cold days avoid "try" items; evenings never suggest "try" items.
pub fn suggest(all: &[Leisure], interests: &str, date: NaiveDate, weather: &Option<Weather>, room: bool, evening: bool) -> Option<Leisure> {
    if !room {
        return None;
    }
    let ds = fmt_date(date);
    let live: Vec<&Leisure> = all.iter().filter(|l| l.status != "finished").collect();
    // already suggested for this date: stay stable on repeated calls
    if let Some(l) = live.iter().find(|l| l.last_suggested.as_deref() == Some(ds.as_str())) {
        return (!(evening && l.kind == "try")).then(|| (*l).clone());
    }
    let recent = |l: &Leisure| l.last_suggested.as_deref().and_then(parse_date).is_some_and(|s| (date - s).num_days().abs() < 5);
    let pool = |status: &str| -> Vec<&Leisure> {
        let mut v: Vec<&Leisure> = live.iter().copied().filter(|l| l.status == status).collect();
        if status == "someday" {
            v.retain(|l| !recent(l));
        }
        if evening {
            v.retain(|l| l.kind != "try");
        } else if bad_weather(weather) {
            let calm: Vec<&Leisure> = v.iter().copied().filter(|l| l.kind != "try").collect();
            if !calm.is_empty() {
                v = calm;
            }
        }
        v.sort_by_key(|l| l.id);
        v
    };
    for status in ["current", "someday"] {
        let v = pool(status);
        if v.is_empty() {
            continue;
        }
        let best = v.iter().map(|l| interest_score(interests, l)).max().unwrap_or(0);
        let top: Vec<&Leisure> = v.into_iter().filter(|l| interest_score(interests, l) == best).collect();
        return Some(top[idx(date, 91, top.len())].clone());
    }
    None
}

/// Record that `id` was suggested on `date` (called when the briefing prep for today is computed).
pub fn stamp(env: &mut Env, id: i64, date: NaiveDate) {
    let ds = fmt_date(date);
    if let Some(l) = env.st.leisure.iter_mut().find(|l| l.id == id) {
        if l.last_suggested.as_deref() != Some(ds.as_str()) {
            l.last_suggested = Some(ds);
            env.changed = true;
        }
    }
}

// ------------------------------------------------------------- phrasing

fn usable_blurb(l: &Leisure) -> Option<String> {
    let b = l.blurb.trim();
    if b.is_empty() || b == UNKNOWN_BLURB {
        return None;
    }
    let first = match b.find(". ") {
        Some(i) => &b[..=i],
        None => b,
    };
    let first = first.trim();
    Some(if first.ends_with(['.', '!', '?']) { first.to_string() } else { format!("{first}.") })
}

/// What to do, as a verb phrase ("watch another episode of X", "start reading X").
pub fn action(l: &Leisure) -> String {
    let t = &l.title;
    match (l.kind.as_str(), l.status == "current") {
        ("watch", true) => format!("watch another episode of {t}"),
        ("watch", false) => format!("start watching {t}"),
        ("read", true) => format!("read a few more chapters of {t}"),
        ("read", false) => format!("start reading {t}"),
        ("listen", true) => format!("listen to a bit more of {t}"),
        ("listen", false) => format!("start listening to {t}"),
        ("play", true) => format!("play another round of {t}"),
        ("play", false) => format!("give {t} a go"),
        (_, true) => format!("get around to {t}"),
        _ => format!("try {t}"),
    }
}

/// One gentle, optional sentence (plus the item's blurb for something new) for the morning briefing.
pub fn morning_sentence(l: &Leisure, date: NaiveDate, open: bool) -> String {
    let a = action(l);
    let s = if open {
        match idx(date, 61, 2) {
            0 => format!("Since today is looking so open, it might be a nice chance to {a}."),
            _ => format!("If you feel like some downtime today, it might be a lovely time to {a}."),
        }
    } else {
        match idx(date, 61, 2) {
            0 => format!("If the afternoon stays quiet, it might be a nice chance to {a}."),
            _ => format!("If you find a free hour later, it could be a good moment to {a}."),
        }
    };
    match (l.status == "current", usable_blurb(l)) {
        (false, Some(b)) => format!("{s} {b}"),
        _ => s,
    }
}

pub fn evening_sentence(l: &Leisure, date: NaiveDate) -> String {
    let a = action(l);
    let s = match (l.kind.as_str(), idx(date, 62, 2)) {
        ("play", _) => format!("If you've got some time tonight, you could settle in and {a}."),
        (_, 0) => format!("If you've got some time tonight, you could curl up and {a}."),
        _ => format!("If you've got a quiet evening, you could curl up and {a}."),
    };
    match (l.status == "current", usable_blurb(l)) {
        (false, Some(b)) => format!("{s} {b}"),
        _ => s,
    }
}

// ------------------------------------------------------------- AI context

fn kind_ing(k: &str) -> &'static str {
    match k {
        "watch" => "to watch",
        "read" => "to read",
        "listen" => "to listen to",
        "play" => "to play",
        _ => "to try",
    }
}

/// Leisure lines for a morning/evening Gemini context. Empty when there is no list.
pub fn context_block(list: &[Leisure], pick: &Option<Leisure>, room: bool) -> String {
    if list.is_empty() {
        return String::new();
    }
    let mut s = String::from("Leisure list (current first; things for free time, NOT goals):\n");
    for l in list.iter().take(12) {
        let st = if l.status == "current" { "currently on the go" } else { "someday" };
        let note = if l.note.is_empty() { String::new() } else { format!("; note: {}", l.note) };
        let blurb = usable_blurb(l).map(|b| format!("; about it: {b}")).unwrap_or_default();
        s.push_str(&format!("  - {} ({}, {st}{note}{blurb})\n", l.title, kind_ing(&l.kind)));
    }
    s.push_str(&format!("Free room for leisure today: {}\n", if room { "yes, there is genuinely free time" } else { "no, the day is not free enough" }));
    match pick {
        Some(l) => s.push_str(&format!("Computed leisure suggestion (use it only if there is free room): {}\n", action(l))),
        None => s.push_str("Computed leisure suggestion: none today\n"),
    }
    s.push_str("LEISURE RULES: suggest AT MOST ONE leisure item, only when the free room line says yes, softly and optionally in one sentence (for example 'if the afternoon stays quiet, it might be a nice chance to ...'), never as a goal, task or progress to track. In the morning that means only genuinely free days; in the evening most evenings qualify, so do suggest it unless the computed suggestion says none. Only mention items on the list; never introduce new titles of your own. When you do, use the computed suggestion; for a new item you may add one short spoken sentence from its description. Never mention finished items. If the computed suggestion says none, mention no leisure at all.\n");
    s
}

pub fn finished_context(finished: &[Leisure]) -> String {
    if finished.is_empty() {
        return String::new();
    }
    let t: Vec<&str> = finished.iter().map(|l| l.title.as_str()).collect();
    format!("Finished for fun this week (optional light highlight, at most one short mention): {}\n", t.join("; "))
}

pub fn week_range(date: NaiveDate) -> (NaiveDate, NaiveDate) {
    let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
    (monday, monday + Duration::days(6))
}

pub fn finished_between(all: &[Leisure], from: NaiveDate, to: NaiveDate) -> Vec<Leisure> {
    let (f, t) = (fmt_date(from), fmt_date(to));
    list_all(all).into_iter().filter(|l| l.status == "finished" && l.finished_on.as_deref().is_some_and(|d| d >= f.as_str() && d <= t.as_str())).collect()
}
