//! Day note: a short, grounded, specific note about one day (the home screen "affirmation").
//! The core returns the prompts, a cache key and the template text; the UI calls Gemini.
use crate::digest;
use crate::env::Env;
use crate::error::Res;
use crate::goals;
use crate::memory;
use crate::model::{Goal, Memory};
use crate::tasks::{self, DayInfo, Occurrence};
use crate::util::*;
use chrono::NaiveDate;
use serde_json::{json, Value};

/// Stable FNV-1a hash of the plan (title, time, part_of_day); deliberately ignores done state.
pub fn plan_hash(items: &[Occurrence]) -> String {
    let mut s = String::new();
    for o in items {
        s.push_str(&o.title);
        s.push('\u{1f}');
        s.push_str(o.time.as_deref().unwrap_or(""));
        s.push('\u{1f}');
        s.push_str(&o.part_of_day);
        s.push('\n');
    }
    fnv(&s)
}

// ---------------------------------------------------------------- template

pub const PARTS: [&str; 3] = ["morning", "afternoon", "evening"];

/// Which part of the day an item sits in: by clock time if timed, else its part_of_day.
pub fn part_idx(o: &Occurrence) -> Option<usize> {
    if let Some(t) = &o.time {
        let h: u32 = t.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(12);
        return Some(if h < 12 {
            0
        } else if h < 17 {
            1
        } else {
            2
        });
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

fn phrase(o: &Occurrence) -> String {
    match &o.time {
        Some(t) => format!("{} at {}", o.title, say_time(t)),
        None => o.title.clone(),
    }
}

fn title_words(o: &Occurrence) -> Vec<String> {
    o.title.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// A routine or planning habit (as opposed to a meaningful appointment, meeting or errand).
pub fn is_routine(o: &Occurrence) -> bool {
    let w = title_words(o);
    let has = |k: &[&str]| w.iter().any(|x| k.contains(&x.as_str()));
    if has(&["routine"]) {
        return true;
    }
    o.recurring && (has(&["plan", "planning", "review", "journal", "reflect", "wind", "tidy"]) || o.category.as_ref().is_some_and(|c| c.name == "Self-care"))
}

/// The task to name first. Prefers meaningful timed items (appointments, meetings) over routines and
/// recurring habits; then the earliest timed one, then the first morning one, then the first undone.
pub fn key_task(items: &[Occurrence]) -> Option<&Occurrence> {
    let undone = || items.iter().filter(|o| !o.done);
    let earliest = |f: &dyn Fn(&Occurrence) -> bool| undone().filter(|o| o.time.is_some() && f(o)).min_by(|a, b| a.time.cmp(&b.time));
    earliest(&|o| !o.recurring && !is_routine(o))
        .or_else(|| earliest(&|o| !is_routine(o)))
        .or_else(|| undone().find(|o| part_idx(o) == Some(0) && !is_routine(o)))
        .or_else(|| undone().find(|o| !is_routine(o)))
        .or_else(|| earliest(&|_| true))
        .or_else(|| undone().next())
        .or_else(|| items.first())
}

fn second_task<'a>(items: &'a [Occurrence], key: &Occurrence) -> Option<&'a Occurrence> {
    items.iter().filter(|o| !o.done && !(o.task_id == key.task_id && o.date == key.date)).min_by(|a, b| match (&a.time, &b.time) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    })
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

// ------------------------------------------------------------------ prompts

pub const SYSTEM: &str = "You write a single short note shown on the home screen of a personal planner app, about ONE specific day. \
Output 1 to 2 sentences, at most 40 words, plain text. No markdown, no emojis, no quotation marks, no lists. \
Sound like a chatty, warm friend who knows the person, not a report. Be grounded and specific: name the key task, say roughly where the busy stretch is, and where there is a breather or slack. \
NEVER state raw counts, fractions or percentages (no '5 tasks', 'three things', '0 of 4', '50 percent'); small natural quantities like 'a couple of things' or 'a few' are fine, but naming the actual tasks is better. \
If one of the goals given plausibly relates to a task today, you may tie them together in a few words; never force a link, and mention a goal deadline only if it is close and relevant. \
Never use clichés or generic affirmations like 'you've got this', 'believe in yourself', 'one step at a time' or 'you are enough'. \
Do not mention the gym or workouts. Do not invent tasks, times or numbers. Output only the note.";

fn gemini_context(items: &[Occurrence], d: NaiveDate, goals: &[Goal], mem: &Memory, info: &DayInfo) -> String {
    let mut s = format!("Date: {}, {}\n", weekday_name(d), d.format("%B %-d"));
    s.push_str(&digest::context_line(info, "Today's"));
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
    s.push_str(&memory::context_block(mem));
    s.push_str(&goals::context_block(goals, &[], d));
    s
}

/// On holidays and days off, open with a warm line about it.
pub fn with_frame(t: String, info: &DayInfo) -> String {
    if info.holiday.is_some() || info.day_off {
        if let Some(f) = digest::frame_sentence(info) {
            return format!("{f} {t}");
        }
    }
    t
}

/// The day note's template text for a day (also used as the briefing's `affirmation`).
pub fn note_text(items: &[Occurrence], d: NaiveDate, goals: &[Goal], info: &DayInfo) -> String {
    with_frame(template_note(items, d, goals), info)
}

/// `GET /api/day-note?date=`
pub fn day_note(env: &mut Env, date: NaiveDate) -> Res<Value> {
    let ds = fmt_date(date);
    let mut days = tasks::build_days(env, date, 1);
    let day = days.remove(0);
    let info = day.info();
    let items = day.items;
    let goals = goals::list_active(&env.st.goals);
    let mem = env.st.memory.clone();
    let template_text = note_text(&items, date, &goals, &info);
    let all_done = !items.is_empty() && items.iter().all(|o| o.done);
    let fp = fnv(&format!("{}#{}{:?}", goals::fingerprint(&goals, date), memory::fingerprint(&mem), info));
    let hash = format!("{}{}", plan_hash(&items), &fp[..8]);
    Ok(json!({
        "date": ds,
        "cache_key": format!("daynote:{ds}:{hash}"),
        "system_prompt": SYSTEM,
        "user_prompt": gemini_context(&items, date, &goals, &mem, &info),
        "template_text": template_text,
        // nothing for the model to say about an empty or finished day: use the template as is
        "skip_ai": items.is_empty() || all_done,
        "max_words": 70,
    }))
}
