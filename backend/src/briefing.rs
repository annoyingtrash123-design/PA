use crate::daynote;
use crate::digest;
use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, Res};
use crate::goals::{self, Goal};
use crate::gym::{self, WorkoutPlan};
use crate::memory::{self, Memory};
use crate::tasks::{self, DayInfo, Occurrence};
use crate::weather::{self, Weather};
use crate::{settings, AppState};
use axum::extract::{Query, State};
use axum::Json;
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration as StdDuration;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Morning,
    Evening,
    Weekly,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Morning => "morning",
            Kind::Evening => "evening",
            Kind::Weekly => "weekly",
        }
    }
}

struct Week {
    total: usize,
    done: usize,
    /// titles done this week (deduped, one-offs before recurring routines)
    done_titles: Vec<String>,
    /// planned one-off tasks that were not done
    slid_titles: Vec<String>,
    /// completed gym sessions (template names, in order)
    sessions: Vec<String>,
    /// exercises whose top weight rose: (name, now, was)
    prs: Vec<(String, f64, f64)>,
    /// notable things next week: (weekday, title)
    next_items: Vec<(String, String)>,
    next_workouts: Vec<String>,
}

struct Ctx {
    kind: Kind,
    date: NaiveDate,
    user: String,
    assistant: String,
    lat: f64,
    lon: f64,
    key: Option<String>,
    items: Vec<Occurrence>,
    workout: Option<WorkoutPlan>,
    gym_done: Option<String>,
    tomorrow: Option<(String, Vec<Occurrence>, Option<WorkoutPlan>)>,
    week: Option<Week>,
    goals: Vec<Goal>,
    goals_achieved: Vec<Goal>,
    memory: Memory,
    yesterday: Vec<Occurrence>,
    info: DayInfo,
    tomorrow_info: DayInfo,
}

fn count_done(items: &[Occurrence]) -> usize {
    items.iter().filter(|o| o.done).count()
}

fn build_ctx(conn: &rusqlite::Connection, kind: Kind, date: NaiveDate) -> Res<Ctx> {
    let s = settings::load(conn)?;
    let mut ctx = Ctx {
        kind,
        date,
        user: s.user_name.clone(),
        assistant: s.assistant_name.clone(),
        lat: s.latitude,
        lon: s.longitude,
        key: settings::gemini_key(conn),
        items: vec![],
        workout: None,
        gym_done: None,
        tomorrow: None,
        week: None,
        goals: goals::list_active(conn)?,
        goals_achieved: vec![],
        memory: memory::load(conn)?,
        yesterday: vec![],
        info: DayInfo::default(),
        tomorrow_info: DayInfo::default(),
    };
    match kind {
        Kind::Morning | Kind::Evening => {
            let morning = kind == Kind::Morning;
            let start = if morning { date - Duration::days(1) } else { date };
            let mut days = tasks::build_days(conn, start, if morning { 3 } else { 2 })?;
            if morning {
                let y = days.remove(0);
                ctx.yesterday = y.items;
            }
            let first = days.remove(0);
            ctx.info = first.info();
            ctx.items = first.items;
            ctx.workout = first.workout;
            if kind == Kind::Evening {
                ctx.gym_done = gym::completed_session_on(conn, date)?.map(|(_, _, name)| name);
            }
            if let Some(t) = days.pop() {
                let d = parse_date(&t.date).unwrap_or(date + Duration::days(1));
                ctx.tomorrow_info = t.info();
                ctx.tomorrow = Some((tasks::weekday_name(d), t.items, t.workout));
            }
        }
        Kind::Weekly => {
            let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
            let days = tasks::build_days(conn, monday, 14)?;
            let (this, next) = days.split_at(7);
            let mut w = Week { total: 0, done: 0, done_titles: vec![], slid_titles: vec![], sessions: vec![], prs: vec![], next_items: vec![], next_workouts: vec![] };
            let mut routine_done: Vec<String> = vec![];
            for d in this {
                w.total += d.items.len();
                w.done += count_done(&d.items);
                for o in &d.items {
                    let list = if o.done {
                        if o.recurring { &mut routine_done } else { &mut w.done_titles }
                    } else if !o.recurring && parse_date(&o.date).is_some_and(|x| x < today()) {
                        &mut w.slid_titles
                    } else {
                        continue;
                    };
                    if !list.contains(&o.title) {
                        list.push(o.title.clone());
                    }
                }
            }
            w.slid_titles.retain(|t| !w.done_titles.contains(t));
            for t in routine_done {
                if !w.done_titles.contains(&t) {
                    w.done_titles.push(t);
                }
            }
            w.sessions = gym::sessions_between(conn, monday, monday + Duration::days(6))?.into_iter().map(|(_, n)| n).collect();
            w.prs = gym::weekly_prs(conn, monday, monday + Duration::days(6))?;
            for d in next {
                for o in d.items.iter().filter(|o| !o.recurring) {
                    w.next_items.push((d.weekday.clone(), item_phrase(o)));
                }
                if let Some(p) = &d.workout {
                    if !p.rest && !w.next_workouts.contains(&p.name) {
                        w.next_workouts.push(p.name.clone());
                    }
                }
            }
            ctx.goals_achieved = goals::achieved_between(conn, monday, monday + Duration::days(6))?;
            ctx.week = Some(w);
        }
    }
    Ok(ctx)
}

pub const CREATE_TABLE: &str =
    "CREATE TABLE IF NOT EXISTS briefing_cache (kind TEXT NOT NULL, date TEXT NOT NULL, hash TEXT NOT NULL, text TEXT NOT NULL, PRIMARY KEY (kind, date, hash));";

// ----------------------------------------------------------------- helpers

pub fn say_time(t: &str) -> String {
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

pub fn join(v: &[String]) -> String {
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        2 => format!("{} and {}", v[0], v[1]),
        _ => format!("{} and {}", v[..v.len() - 1].join(", "), v[v.len() - 1]),
    }
}

pub fn limited(v: Vec<String>, max: usize) -> String {
    if v.len() <= max {
        join(&v)
    } else {
        let extra = match v.len() - max {
            1 => "one more",
            2 => "a couple more",
            _ => "a few more",
        };
        format!("{}, plus {extra}", v[..max].join(", "))
    }
}

fn ordinal(n: u32) -> String {
    let suf = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suf}")
}

pub fn pick<'a>(opts: &[&'a str], d: NaiveDate, salt: u64) -> &'a str {
    let h = (d.num_days_from_ce() as u64)
        .wrapping_add(salt.wrapping_mul(7919))
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    opts[((h >> 33) as usize) % opts.len()]
}

fn name_suffix(user: &str) -> String {
    if user.is_empty() { String::new() } else { format!(", {user}") }
}

pub fn item_phrase(o: &Occurrence) -> String {
    match &o.time {
        Some(t) => format!("{} at {}", o.title, say_time(t)),
        None => o.title.clone(),
    }
}



fn hint_sentence(p: &WorkoutPlan) -> Option<String> {
    let mut fallback = None;
    for e in &p.exercises {
        if let (Some(l), Some(sw)) = (&e.last, e.suggested_weight) {
            let last_w = l.sets.iter().map(|s| s.weight).fold(0.0, f64::max);
            if sw > last_w {
                return Some(format!("Last time you hit every rep on {}, so go for {} kilos today.", e.name, gym::fmt_w(sw)));
            }
            fallback.get_or_insert(format!("Last time you lifted {} kilos on {}, so see if you can beat those reps.", gym::fmt_w(sw), e.name));
        }
    }
    fallback
}

fn workout_sentence(p: &WorkoutPlan, d: NaiveDate) -> String {
    if p.rest {
        return pick(
            &[
                "And it's a rest day from the gym, so let your body soak up all that hard work.",
                "No gym today, it's a rest day, and rest is part of the plan.",
            ],
            d,
            5,
        )
        .to_string();
    }
    let names: Vec<String> = p.exercises.iter().map(|e| e.name.clone()).collect();
    let mut s = format!("On the gym front, today is {}.", p.name);
    if !names.is_empty() {
        s.push_str(&format!(" You'll be doing {}.", limited(names, 3)));
    }
    if let Some(h) = hint_sentence(p) {
        s.push(' ');
        s.push_str(&h);
    }
    s
}

fn next_up(items: &[Occurrence], date: NaiveDate) -> Option<Occurrence> {
    let undone: Vec<&Occurrence> = items.iter().filter(|o| !o.done).collect();
    if date == today() {
        let now = crate::db::now_hm();
        if let Some(o) = undone.iter().find(|o| o.time.as_deref().is_some_and(|t| t >= now.as_str())) {
            return Some((*o).clone());
        }
        if let Some(o) = undone.iter().find(|o| o.time.is_none()) {
            return Some((*o).clone());
        }
    }
    undone.first().map(|o| (*o).clone())
}

/// Short grounded line about the week, used as the weekly "affirmation" (no numbers).
fn week_line(w: &Week) -> String {
    let tasks: Vec<String> = w.done_titles.iter().take(2).cloned().collect();
    if tasks.is_empty() && w.sessions.is_empty() {
        "A quiet week, and next week is a fresh page.".into()
    } else if !tasks.is_empty() {
        format!("A week with {} in it, and some good momentum to carry forward.", join(&tasks))
    } else {
        format!("A week with {} at the gym, and that counts for a lot.", join(&w.sessions))
    }
}

/// The nearest active goal deadline within `days`, as a sentence fragment.
fn near_deadline(goals: &[Goal], date: NaiveDate, days: i64) -> Option<String> {
    goals
        .iter()
        .filter_map(|g| goals::days_to(g, date).filter(|n| (0..=days).contains(n)).map(|n| (n, g)))
        .min_by_key(|(n, _)| *n)
        .map(|(n, g)| format!("your goal, {}, is {}", goals::short_title(g), goals::away_phrase(n)))
}

// ---------------------------------------------------------------- template

// ------------------------------------------------------------ morning summary

const BIG_WORDS: [&str; 11] = ["report", "essay", "clean", "revise", "revision", "project", "study", "tidy", "write", "organise", "organize"];
const QUICK_WORDS: [&str; 10] = ["call", "pay", "reply", "book", "email", "text", "order", "send", "check", "buy"];

/// Rough size guess from title, notes and checklist (there is no size field).
pub fn is_big(o: &Occurrence) -> bool {
    let words: Vec<String> = o.title.to_lowercase().split(|c: char| !c.is_alphanumeric()).map(|s| s.to_string()).collect();
    let hit = |list: &[&str]| words.iter().any(|w| list.iter().any(|k| w == k || (k.len() >= 5 && w.starts_with(k))));
    if hit(&BIG_WORDS) || o.checklist.len() >= 6 || o.notes.len() > 160 {
        return true;
    }
    if hit(&QUICK_WORDS) || !o.checklist.is_empty() {
        return false;
    }
    o.title.split_whitespace().count() >= 5
}

/// Quick, errand-like things that plausibly fit a lunch break.
fn lunchable(o: &Occurrence) -> bool {
    let w: Vec<String> = o.title.to_lowercase().split(|c: char| !c.is_alphanumeric()).map(|s| s.to_string()).collect();
    o.time.is_none() && !is_big(o) && w.iter().any(|x| ["call", "pay", "book", "buy", "grocery", "shopping", "post", "pick", "order", "bill", "dentist", "doctor", "bank"].contains(&x.as_str()))
}

fn cat_name(o: &Occurrence) -> String {
    o.category.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "Other".into())
}

fn cat_lead(cat: &str, list: &str, d: NaiveDate, salt: u64) -> String {
    let v = |a: String, b: String| if (d.num_days_from_ce() as u64 + salt) % 2 == 0 { a } else { b };
    match cat {
        "Appointments" => v(format!("Appointment-wise, you've got {list}."), format!("In the appointments column there's {list}.")),
        "Work" => v(format!("On the work front you've got {list}."), format!("For work, there's {list}.")),
        "Admin & Money" => v(format!("On the life admin front you've got {list}."), format!("Admin and money-wise, there's {list}.")),
        "Chores" => v(format!("Around the house, there's {list}."), format!("On the chores front you've got {list}.")),
        "Errands" => v(format!("Out and about, you've got {list}."), format!("Errand-wise, there's {list}.")),
        "Study" => v(format!("On the study side there's {list}."), format!("For learning, you've got {list}.")),
        "Exercise" => v(format!("For movement, there's {list}."), format!("On the exercise side you've got {list}.")),
        "Social" => v(format!("Socially, there's {list}."), format!("For people time, you've got {list}.")),
        "Self-care" => v(format!("For looking after yourself, there's {list}."), format!("On the self-care side you've got {list}.")),
        _ => v(format!("And a few other bits: {list}."), format!("Then there's {list} on the side.")),
    }
}

const CAT_ORDER: [&str; 9] = ["Appointments", "Work", "Admin & Money", "Chores", "Errands", "Study", "Exercise", "Social", "Self-care"];

fn words_in(s: &str) -> usize {
    s.split_whitespace().count()
}

fn morning_template(c: &Ctx, weather: &Option<Weather>) -> String {
    let d = c.date;
    let n = name_suffix(&c.user);
    let a = &c.assistant;
    let mut parts: Vec<String> = vec![];
    // 1. hello, date, work frame, weather through the memory prefs
    parts.push(match d.num_days_from_ce() as usize % 4 {
        0 => format!("Good morning{n}! It's {a} here, and I'm so glad you're up."),
        1 => format!("Hey{n}, good morning! {a} here, ready to walk through your day with you."),
        2 => format!("Morning{n}! It's {a}. Grab something warm to drink and let's look at today together."),
        _ => format!("Rise and shine{n}! {a} here with your morning rundown."),
    });
    let wd = tasks::weekday_name(d);
    let mo = d.format("%B").to_string();
    parts.push(match d.num_days_from_ce() as usize % 3 {
        0 => format!("Today is {wd}, {mo} {}.", ordinal(d.day())),
        1 => format!("It's {wd}, the {} of {mo}.", ordinal(d.day())),
        _ => format!("Happy {wd}! It's {mo} {} today.", ordinal(d.day())),
    });
    if let Some(f) = digest::frame_sentence(&c.info) {
        parts.push(f);
    }
    if let Some(w) = weather {
        parts.push(digest::weather_comment_template(w, &c.memory, &c.items, &c.workout, &c.info, d));
    }
    // 2. yesterday
    let mut yd: Vec<String> = vec![];
    for o in c.yesterday.iter().filter(|o| o.done && !o.recurring) {
        if !yd.contains(&o.title) {
            yd.push(o.title.clone());
        }
    }
    for o in c.yesterday.iter().filter(|o| o.done && o.recurring) {
        if !yd.contains(&o.title) {
            yd.push(o.title.clone());
        }
    }
    let rolled: Vec<String> = c.items.iter().filter(|o| o.rolled_over_from.is_some() && !o.done).map(|o| o.title.clone()).collect();
    let mut back = if yd.is_empty() {
        pick(&["Yesterday was a quiet one on the list, and that's perfectly fine.", "Looking back, yesterday didn't leave much of a trail on the list, and there's no judgement here at all."], d, 51).to_string()
    } else {
        let lead = pick(&["Looking back at yesterday, you got", "Quick look back: yesterday you sorted", "Yesterday you got through"], d, 52);
        format!("{lead} {}, which is lovely.", limited(yd.clone(), 3))
    };
    if !rolled.is_empty() {
        back.push_str(&format!(" {} didn't quite happen, so it's moved across to today, with no pressure at all.", limited(rolled.clone(), 2)));
    }
    parts.push(back);
    // 3. today by category
    let todo: Vec<&Occurrence> = c.items.iter().filter(|o| !o.done).collect();
    let already: Vec<String> = c.items.iter().filter(|o| o.done).map(|o| o.title.clone()).collect();
    if c.items.is_empty() {
        parts.push("Your list is wide open today, which means you get to decide what kind of day this is. Lovely.".into());
    } else {
        parts.push(pick(&["So, what does today hold?", "Here's how today breaks down.", "Let's go through what's on your plate."], d, 7).to_string());
        let mut cats: Vec<String> = CAT_ORDER.iter().map(|s| s.to_string()).collect();
        for o in &todo {
            let cn = cat_name(o);
            if !cats.contains(&cn) && cn != "Other" {
                cats.push(cn);
            }
        }
        cats.push("Other".into());
        for (i, cn) in cats.iter().enumerate() {
            let mut group: Vec<&Occurrence> = todo.iter().filter(|o| &cat_name(o) == cn).cloned().collect();
            if group.is_empty() {
                continue;
            }
            group.sort_by(|x, y| match (&x.time, &y.time) {
                (Some(p), Some(q)) => p.cmp(q),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            });
            let list: Vec<String> = group.iter().map(|o| item_phrase(o)).collect();
            parts.push(cat_lead(cn, &limited(list, 3), d, i as u64));
        }
        if !already.is_empty() {
            parts.push(format!("{} is already ticked off, so you've got a head start.", limited(already.clone(), 2)));
        }
    }
    if let Some(p) = &c.workout {
        let mut ws = workout_sentence(p, d);
        if !p.rest {
            if let Some((g, _)) = goals::find_link(&c.goals, &[(p.name.clone(), format!("gym workout {}", p.name))]) {
                ws.push_str(&format!(" That one feeds straight into {}.", goals::goal_ref(g)));
            }
        }
        parts.push(ws);
    }
    let workout_linked = c.workout.as_ref().is_some_and(|p| !p.rest && goals::find_link(&c.goals, &[(p.name.clone(), format!("gym workout {}", p.name))]).is_some());
    if !workout_linked {
        let texts: Vec<(String, String)> = todo.iter().map(|o| (o.title.clone(), o.title.clone())).collect();
        if let Some((g, task)) = goals::find_link(&c.goals, &texts) {
            parts.push(match d.num_days_from_ce() as usize % 2 {
                0 => format!("And {task} is a nice little nudge toward {}.", goals::goal_ref(g)),
                _ => format!("Worth knowing that {task} quietly helps with {}.", goals::goal_ref(g)),
            });
        }
    }
    if let Some(dl) = near_deadline(&c.goals, d, 21) {
        parts.push(format!("Also, keep in mind that {dl}."));
    }
    // 4. suggested order
    let fixed: Vec<String> = todo.iter().filter(|o| o.time.is_some()).map(|o| item_phrase(o)).collect();
    let loose: Vec<&Occurrence> = todo.iter().filter(|o| o.time.is_none() && cat_name(o) != "Exercise").cloned().collect();
    let big: Vec<&Occurrence> = loose.iter().filter(|o| is_big(o)).cloned().collect();
    let quick: Vec<&Occurrence> = loose.iter().filter(|o| !is_big(o)).cloned().collect();
    let titles = |v: &[&Occurrence], k: usize| -> Vec<String> { v.iter().take(k).map(|o| o.title.clone()).collect() };
    let mut order: Vec<String> = vec![];
    if !fixed.is_empty() {
        order.push(format!("The fixed points are {}, so everything else bends around those.", limited(fixed, 3)));
    }
    let lunch_pick = c.info.work.as_ref().and_then(|_| quick.iter().find(|o| lunchable(o)).map(|o| o.title.clone()));
    match (big.first(), quick.len()) {
        (Some(b), q) if q >= 3 => order.push(format!(
            "For the rest, I'd clear the quick ones first, {}, to build some momentum, and then give {} your proper attention while the afternoon is still on your side.",
            limited(titles(&quick, 3), 3),
            b.title
        )),
        (Some(b), _) => order.push(format!(
            "For the rest, I'd start with {} while you're fresh, because it's the one that could drag, and let the small things{} fill the gaps afterwards.",
            b.title,
            if quick.is_empty() { String::new() } else { format!(", like {}", limited(titles(&quick, 2), 2)) }
        )),
        (None, q) if q > 0 => order.push(format!(
            "Nothing loose is heavy, so run through {} in whatever order feels good, quickest first for an easy win.",
            limited(titles(&quick, 3), 3)
        )),
        _ => {
            if order.is_empty() {
                order.push("There's nothing loose to put in order today, so the day can just unfold.".into());
            }
        }
    }
    if let Some(w) = &c.info.work {
        let r = &c.memory.rhythm;
        let mut s = format!(
            "With work from {} to {}, the window between waking up around {} and starting is good for the first small thing",
            say_time(&w.start),
            say_time(&w.end),
            say_time(&r.wake_to)
        );
        s.push_str(&match &lunch_pick {
            Some(t) => format!(", and {t} would fit neatly into your lunch hour, whenever you take it."),
            None => ", and your lunch hour, whenever you take it, is perfect for something short.".to_string(),
        });
        if !big.is_empty() {
            s.push_str(" Anything heavier is best before work or once you've finished, rather than squeezed in between.");
        }
        order.push(s);
        order.push(format!("Try to wrap up the evening comfortably before bedtime around {}.", say_time(&c.memory.rhythm.sleep_from)));
    } else if c.info.holiday.is_some() || c.info.day_off {
        order.push("With no work to fit around, take the order at your own pace and keep a long, lazy stretch free.".into());
    } else {
        order.push("Keep a decent break in the middle so the second half of the day doesn't feel like a slog.".into());
    }
    parts.extend(order);
    // 5. tomorrow
    if let Some((twd, titems, tw)) = &c.tomorrow {
        let mut notable: Vec<&Occurrence> = titems.iter().filter(|o| cat_name(o) == "Appointments" || o.time.is_some()).collect();
        if notable.is_empty() {
            notable = titems.iter().filter(|o| !o.recurring).take(2).collect();
        }
        let mut s = if notable.is_empty() {
            format!("Peeking at tomorrow, {twd}, it's looking nice and open so far.")
        } else {
            let ph: Vec<String> = notable.iter().map(|o| item_phrase(o)).collect();
            format!("Peeking at tomorrow, {twd}, you've got {}.", limited(ph, 3))
        };
        if let Some(h) = &c.tomorrow_info.holiday {
            s.push_str(&format!(" It's also a public holiday, {h}, so no work."));
        } else if c.tomorrow_info.day_off {
            s.push_str(" It's also a day off, so no work.");
        }
        if let Some(p) = tw.as_ref().filter(|p| !p.rest) {
            s.push_str(&format!(" The gym has {} lined up.", p.name));
        }
        parts.push(s);
    }
    // evergreen filler keeps the spoken length in the 2-3 minute range on lighter days
    let fillers = [
        "Whatever happens, leave yourself a proper break in the middle of the day, even if it's just a few minutes away from every screen and a decent cup of something warm.",
        "If the plan wobbles, that's completely okay. Move the flexible things later and keep the fixed ones where they are, and I'll happily help you re-sort whenever you open me.",
        "Eat something proper at lunch, drink plenty of water, and don't let the small tasks turn into a way of avoiding the bigger one.",
        "Have a rough idea of when you want to be finished tonight, so the evening stays yours rather than quietly filling up with leftovers.",
        "Remember the list is a guide, not a contract, so be kind to yourself if the day goes a little sideways.",
        "Take a moment now to picture how you want the day to feel when you finally put your feet up tonight, and let that steer what you say yes to.",
    ];
    let mut extra: Vec<&str> = vec![];
    let mut fi = d.num_days_from_ce() as usize % fillers.len();
    let total = |p: &Vec<String>, e: &Vec<&str>| p.iter().map(|x| words_in(x)).sum::<usize>() + e.iter().map(|x| words_in(x)).sum::<usize>() + 30;
    while total(&parts, &extra) < 345 && extra.len() < fillers.len() {
        extra.push(fillers[fi % fillers.len()]);
        fi += 1;
    }
    parts.extend(extra.iter().map(|s| s.to_string()));
    // 6. grounded close
    parts.push(match daynote::key_task(&c.items) {
        Some(k) if !k.done => {
            let kp = item_phrase(k);
            match d.num_days_from_ce() as usize % 3 {
                0 => format!("If you only get one thing done today, make it {kp}."),
                1 => format!("Start with {kp} and the rest of the day gets easier to read."),
                _ => format!("The one to watch today is {kp}."),
            }
        }
        Some(_) => "Everything on the list is already done, so the rest of the day is yours.".into(),
        None => "Nothing is waiting on you, so the day is yours to shape.".into(),
    });
    parts.push(pick(&["I'll be right here if the plan changes.", "Have a good one, and shout if you need me.", "Go get started, and I'll be here when you need me."], d, 17).into());
    parts.join(" ")
}


fn template_text(c: &Ctx, weather: &Option<Weather>) -> String {
    let d = c.date;
    let n = name_suffix(&c.user);
    let a = &c.assistant;
    let mut parts: Vec<String> = vec![];
    match c.kind {
        Kind::Morning => parts.push(morning_template(c, weather)),
        Kind::Evening => {
            parts.push(match d.num_days_from_ce() as usize % 3 {
                0 => format!("Hey{n}, it's {a}. Let's wind the day down together."),
                1 => format!("Good evening{n}! {a} here. You made it through another day, so let's take a quiet look back."),
                _ => format!("Hi there{n}! It's {a}. Kick your shoes off, it's time to unwind."),
            });
            let total = c.items.len();
            let done = count_done(&c.items);
            let done_titles: Vec<String> = c.items.iter().filter(|o| o.done).map(|o| o.title.clone()).collect();
            parts.push(if total == 0 {
                "There was nothing on your list today, so there's no pressure at all.".into()
            } else if done == total {
                format!("You wrapped up everything on your list today, {} included. I'm honestly so proud of you.", limited(done_titles.clone(), 3))
            } else if done == 0 {
                match d.num_days_from_ce() as usize % 2 {
                    0 => "The task list didn't quite happen today, and honestly, that's okay. Some days are just like that.".into(),
                    _ => "Today was more of a slow one on the task front, and that's completely fine. Tomorrow is a fresh start.".into(),
                }
            } else {
                match d.num_days_from_ce() as usize % 2 {
                    0 => format!("You got {} sorted today. That's real progress.", limited(done_titles.clone(), 3)),
                    _ => format!("Nice work today, with {} now off your plate.", limited(done_titles.clone(), 3)),
                }
            });
            let left: Vec<String> = c.items.iter().filter(|o| !o.done).map(|o| o.title.clone()).collect();
            if !left.is_empty() {
                parts.push(format!(
                    "{} still waiting, but any one-off tasks will roll over to tomorrow on their own, so there's nothing to stress about.",
                    limited(left, 3)
                ));
            }
            let mut link_texts: Vec<(String, String)> = Vec::new();
            if let Some(g) = &c.gym_done {
                parts.push(format!("And you got your {g} session in, which is fantastic."));
                link_texts.push((format!("that {g} session"), format!("gym session {g}")));
            } else if let Some(p) = &c.workout {
                if p.rest {
                    parts.push("Your rest day did its job too.".into());
                } else {
                    parts.push(format!("The gym didn't happen today, and that's fine. {} will be waiting for you.", p.name));
                }
            }
            link_texts.extend(done_titles.iter().map(|t| (t.clone(), t.clone())));
            if let Some((g, what)) = goals::find_link(&c.goals, &link_texts) {
                let gr = goals::goal_ref(g);
                parts.push(match d.num_days_from_ce() as usize % 3 {
                    0 => format!("{what} counts toward {gr}, by the way."),
                    1 => format!("Quietly, {what} moved {gr} forward."),
                    _ => format!("That's one more step toward {gr}, thanks to {what}."),
                });
            }
            if let Some((wd, items, w)) = &c.tomorrow {
                let mut s = if items.is_empty() {
                    format!("Tomorrow is {wd}, and it's looking nice and open.")
                } else {
                    let ph: Vec<String> = items.iter().map(item_phrase).collect();
                    format!("Looking ahead to tomorrow, {wd}, you've got {}.", limited(ph, 3))
                };
                if let Some(p) = w {
                    if p.rest {
                        s.push_str(" And it's a rest day at the gym.");
                    } else {
                        s.push_str(&format!(" The gym has {} lined up.", p.name));
                    }
                }
                parts.push(s);
            }
            if let Some(h) = &c.tomorrow_info.holiday {
                parts.push(format!("Tomorrow is a public holiday, {h}, so no work, which is something to look forward to."));
            } else if c.tomorrow_info.day_off {
                parts.push("Tomorrow is a day off, so no work to set an alarm for.".into());
            }
            if let Some(dl) = near_deadline(&c.goals, d, 14) {
                parts.push(format!("One thing on the horizon: {dl}."));
            }
            if let Some(w) = weather {
                parts.push(format!("It ended up as {} today.", w.summary.to_lowercase()));
            }
            parts.push(match &c.tomorrow {
                Some((wd, items, _)) => match daynote::key_task(items) {
                    Some(k) => format!("The first thing to think about for {wd} is {}, so nothing else needs your attention tonight.", item_phrase(k)),
                    None => format!("{wd} has nothing scheduled, so there's nothing to prepare tonight."),
                },
                None => "Nothing else needs your attention tonight.".into(),
            });
            parts.push(format!("{} {}", pick(&["Start winding down soon so you're asleep in good time.", "Dim the lights and ease off the screens soon, so sleep comes easily.", "Maybe get ready for bed a little early, so you're properly rested."], d, 29), pick(&["Sleep well.", "Rest up, I'll see you tomorrow.", "Goodnight."], d, 23)));
        }
        Kind::Weekly => {
            let w = c.week.as_ref().unwrap();
            parts.push(match d.num_days_from_ce() as usize % 3 {
                0 => format!("Hey{n}, it's {a}! Time for our weekly catch-up, so settle in."),
                1 => format!("Hi there{n}, {a} here. Let's take a moment to look back at your week."),
                _ => format!("Hi again{n}! It's {a}, and I've got your week in review."),
            });
            let quiet = w.done_titles.is_empty() && w.sessions.is_empty();
            // goal connection: first goal that relates to something done or trained
            let mut texts: Vec<(String, String)> = Vec::new();
            for (nm, _, _) in w.prs.iter() {
                texts.push((format!("the progress on {nm}"), format!("{nm} gym lift")));
            }
            for sn in w.sessions.iter() {
                texts.push((format!("the {sn} session"), format!("gym session {sn}")));
            }
            texts.extend(w.done_titles.iter().map(|t| (t.clone(), t.clone())));
            let linked = goals::find_link(&c.goals, &texts);
            if quiet {
                parts.push(
                    pick(
                        &[
                            "It was a quiet one this week, and that's completely okay. Some weeks are for getting through, and you got through it.",
                            "Not much got ticked off this week, and I'm not going to make anything of that. Weeks like this happen to everyone.",
                        ],
                        d,
                        37,
                    )
                    .into(),
                );
            } else {
                // 2-3 highlights, goal-linked ones first
                let mut titles: Vec<String> = w.done_titles.clone();
                if let Some((_, t)) = &linked {
                    if let Some(i) = titles.iter().position(|x| x == t) {
                        let x = titles.remove(i);
                        titles.insert(0, x);
                    }
                }
                let mut hl: Vec<String> = vec![];
                if !titles.is_empty() {
                    let take: Vec<String> = titles.iter().take(if w.sessions.is_empty() { 3 } else { 2 }).cloned().collect();
                    hl.push(format!("got {} done", join(&take)));
                }
                if !w.sessions.is_empty() {
                    let mut names: Vec<String> = vec![];
                    for x in &w.sessions {
                        if !names.contains(x) {
                            names.push(x.clone());
                        }
                    }
                    hl.push(format!("made it to the gym for {}", join(&names)));
                }
                if let Some((nm, now, was)) = w.prs.first() {
                    hl.push(format!("moved {nm} up from {} to {} kilos", gym::fmt_w(*was), gym::fmt_w(*now)));
                }
                hl.truncate(3);
                let lead = pick(&["The bits that stood out: you", "Looking back, you", "A few things I loved: you"], d, 41);
                parts.push(format!("{lead} {}. That's a good week's work.", join(&hl)));
                if let Some((g, what)) = &linked {
                    let gr = goals::goal_ref(g);
                    parts.push(match d.num_days_from_ce() as usize % 2 {
                        0 => format!("And it all counts, because {what} ties right into {gr}."),
                        _ => format!("That's real movement on {gr}, with {what} leading the way."),
                    });
                }
            }
            for g in &c.goals_achieved {
                parts.push(format!("And you actually hit a goal this week: {}. That deserves a proper moment.", goals::short_title(g)));
            }
            if !w.slid_titles.is_empty() {
                let v: Vec<String> = w.slid_titles.iter().take(2).cloned().collect();
                parts.push(format!(
                    "{} slid a little, and that's absolutely fine. Those can simply carry into next week, no guilt.",
                    join(&v)
                ));
            }
            // next week
            let mut ahead: Vec<String> = vec![];
            if !w.next_items.is_empty() {
                let v: Vec<String> = w.next_items.iter().take(3).map(|(wd, t)| format!("{t} on {wd}")).collect();
                ahead.push(format!("Looking ahead to next week, you've got {}.", limited(v, 3)));
            } else {
                ahead.push("Next week is looking pleasantly open so far.".into());
            }
            if let Some(first) = w.next_workouts.first() {
                ahead.push(format!("In the gym, {first} is first up."));
            }
            parts.push(ahead.join(" "));
            if let Some(dl) = near_deadline(&c.goals, d, 60) {
                parts.push(format!("Worth a thought: {dl}, so keep it gently on your radar."));
            } else if linked.is_none() {
                if let Some(g) = c.goals.first() {
                    parts.push(format!("And I'm keeping {}, in the back of my mind for you.", goals::goal_ref(g)));
                }
            }
            parts.push(match w.next_items.first() {
                Some((wd, t)) if !quiet => format!("{} The first thing to watch for is {t} on {wd}.", pick(&["Enjoy what's left of your weekend.", "Rest up before the week begins."], d, 31)),
                _ => pick(&["Enjoy the rest of your weekend. I'm right here whenever you need me.", "Take it easy, and I'll see you at the start of the week."], d, 31).into(),
            });
        }
    }
    parts.join(" ")
}

// ------------------------------------------------------------------ gemini

const TONE: &str = "TONE: you are a warm, chatty best friend who is also a really good personal assistant and genuinely knows this person. Sound natural and human, with contractions and a bit of personality, never like a report. \
NEVER read out raw counts, fractions, percentages or tallies: no '0 of 24 tasks', no 'six tasks', no '50 percent', no lists of numbers. Small natural quantities like 'a couple of things' or 'a few' are fine, but prefer naming the specific things (task titles, workout names, exercises) that were done or are coming up. \
Be kind and low-key about quiet days or weeks: no guilt, no stats, no pressure. \
GOALS: when active goals are given, connect specific completed or planned tasks and gym sessions to a goal, but ONLY where they plausibly relate (never force a link, and it is fine to mention no goal at all). Mention an approaching goal deadline naturally when it is relevant and fairly close, saying it like a person would ('that's a few weeks away now'), not as a date dump.";

fn system_prompt(c: &Ctx) -> String {
    let words = match c.kind {
        Kind::Weekly => "between 200 and 300 words",
        Kind::Morning => "between 320 and 450 words (a 2 to 3 minute read-aloud; do not stop short)",
        Kind::Evening => "between 150 and 250 words",
    };
    let focus = match c.kind {
        Kind::Morning => "This is the MORNING briefing, the main daily summary, read aloud in about 2 to 3 minutes. Flow naturally through these parts, as one spoken piece with no headings: (1) a warm hello and the weather, commented on through the person's weather preferences (e.g. cool is comfortable, warn kindly when hot, sun cream when UV is high and they avoid sunburn, enjoy rain if they like it) and tied to the day's plans rather than just read out; mention a public holiday or day off warmly if it is one (no work today); (2) a quick, kind look back at yesterday: name what got done and what rolled over, no counts, no guilt; (3) today broken down by category or theme using the category given for each item (e.g. 'on the life admin front you've got X and Y', work, chores, errands, study, exercise and the gym with goal tie-ins, social plans, appointments with their times), naming specific things; (4) a suggested order for the day: decide whether to do the quick small tasks first to build momentum or the big one first while fresh, say which and why (use the size hints), respect fixed-time items and the work block (work hours are fixed and not a task; use the window between waking and the start of work for early things, the flexible lunch hour only for short things that fit about an hour and plausibly sit near work like quick calls, bills, booking appointments or a quick shop, and keep the evening finishing comfortably before bedtime); (5) a peek at tomorrow's notable things and appointments; (6) a grounded, specific closing line such as the key task to start with. Never use generic affirmations or cliches like 'you\'ve got this'.",
        Kind::Evening => "This is the EVENING wind-down. Kindly say what got done by naming it (celebrate it, never guilt), mention what is left and that unfinished one-off tasks roll over to tomorrow, mention whether the gym happened, link what was done to a goal where it plausibly relates, give a gentle look at tomorrow, mention whether tomorrow is a work day, a public holiday or a day off, gently nudge toward winding down in time for sleep, and end with one grounded, specific closing line (such as what comes first tomorrow) and a calm goodnight. No generic affirmations or cliches.",
        Kind::Weekly => "This is the WEEKLY overview, read on Sunday. Open warmly. Pick two or three specific highlights that were actually done (task titles, gym sessions by name, and any exercise progressions) and connect them to the goals where they plausibly relate. Gently mention anything that slid, without guilt. Look ahead to the notable things next week and to any goal deadline coming up. Close with one short grounded line. If nothing much was done, be kind and low-key, give no stats, and focus on the week ahead and the goals. Do not list every task and never quote totals. No generic affirmations or cliches.",
    };
    format!(
        "You are {a}, the user's personal assistant and friend. You are writing a script that will be read aloud by a text-to-speech voice, so write exactly how a chatty, caring friend would talk.\n\
         Rules: plain spoken sentences only. No markdown, no asterisks, no headings, no bullet points, no numbered lists, no emojis, no stage directions, no quotation marks around the whole text. \
         Say times naturally, like 'at seven thirty in the morning' or 'at 3 PM'. Say 'kilos' instead of 'kg'. Weather temperatures are in Celsius and should be said as degrees. \
         Only use facts from the context; never invent tasks, numbers or weather. Keep it {words}. Output only the script.\n{TONE}\n{focus}",
        a = c.assistant
    )
}

fn context_text(c: &Ctx, weather: &Option<Weather>) -> String {
    let mut s = String::new();
    s.push_str(&format!("Briefing type: {}\n", c.kind.name()));
    s.push_str(&format!("Date: {}, {}\n", tasks::weekday_name(c.date), c.date.format("%B %-d, %Y")));
    s.push_str(&format!("User's name: {}\n", if c.user.is_empty() { "(not set, don't use a name)" } else { &c.user }));
    s.push_str(&format!("Your name: {}\n", c.assistant));
    match weather {
        Some(w) => {
            s.push_str(&format!("Weather: {}, high {} C, low {} C", w.summary, w.temp_max, w.temp_min));
            if let Some(p) = w.precip_prob {
                s.push_str(&format!(", rain chance {} (never quote a percentage)", if p >= 70 { "high" } else if p >= 40 { "moderate" } else { "low" }));
            }
            if let Some(u) = w.uv_max {
                s.push_str(&format!(", UV index {u}"));
            }
            s.push('\n');
        }
        None => s.push_str("Weather: unavailable (do not mention it)\n"),
    }
    let list = |items: &[Occurrence]| -> String {
        if items.is_empty() {
            return "  (nothing scheduled)\n".into();
        }
        let mut o = String::new();
        for i in items {
            let when = match &i.time {
                Some(t) => format!("at {}", say_time(t)),
                None => format!("untimed, {}", i.part_of_day),
            };
            let steps = if i.checklist.is_empty() { String::new() } else { format!("; steps: {}", i.checklist.iter().map(|x| x.text.as_str()).collect::<Vec<_>>().join(", ")) };
            let roll = if i.rolled_over_from.is_some() { "; rolled over from an earlier day" } else { "" };
            let cat = i.category.as_ref().map(|c| format!("; category: {}", c.name)).unwrap_or_default();
            let size = if is_big(i) { "; looks bigger" } else { "; looks quick" };
            o.push_str(&format!("  - {} ({when}; done: {}{cat}{size}{steps}{roll})\n", i.title, if i.done { "yes" } else { "no" }));
        }
        o
    };
    let plan = |p: &WorkoutPlan| -> String {
        if p.rest {
            return "rest day".into();
        }
        let ex: Vec<String> = p
            .exercises
            .iter()
            .map(|e| match &e.hint {
                Some(h) => format!("{} {}x{}-{} ({})", e.name, e.sets, e.rep_min, e.rep_max, h),
                None => format!("{} {}x{}-{}", e.name, e.sets, e.rep_min, e.rep_max),
            })
            .collect();
        format!("{}: {}", p.name, ex.join("; "))
    };
    s.push_str(&digest::context_line(&c.info, "Today's"));
    if c.tomorrow.is_some() {
        s.push_str(&digest::context_line(&c.tomorrow_info, "Tomorrow's"));
    }
    s.push_str(&memory::context_block(&c.memory));
    if c.kind == Kind::Morning {
        s.push_str(&format!("Yesterday's items (done flags; mention what got done by name, kindly):\n{}", list(&c.yesterday)));
    }
    match c.kind {
        Kind::Morning | Kind::Evening => {
            s.push_str(&format!("Today's items in order (each flagged done or not; do not quote totals):\n{}", list(&c.items)));
            match (&c.workout, &c.gym_done) {
                (_, Some(g)) => s.push_str(&format!("Workout: completed {g} today\n")),
                (Some(p), None) => s.push_str(&format!("Workout today: {}\n", plan(p))),
                _ => s.push_str("Workout today: none planned\n"),
            }
            if let Some((wd, items, w)) = &c.tomorrow {
                s.push_str(&format!("Tomorrow is {wd}:\n{}", list(items)));
                if let Some(p) = w {
                    s.push_str(&format!("Tomorrow's workout: {}\n", plan(p)));
                }
            }
        }
        Kind::Weekly => {
            let w = c.week.as_ref().unwrap();
            let vl = |v: &[String]| if v.is_empty() { "nothing notable".to_string() } else { v.join("; ") };
            s.push_str(&format!("Things done this week (Mon-Sun): {}\n", vl(&w.done_titles)));
            s.push_str(&format!("Planned but slid, not done (mention gently, no guilt): {}\n", vl(&w.slid_titles)));
            s.push_str(&format!("Gym sessions completed this week: {}\n", vl(&w.sessions)));
            for (n, now, was) in &w.prs {
                s.push_str(&format!("Progression: {n} top weight went up from {} to {} kilos\n", gym::fmt_w(*was), gym::fmt_w(*now)));
            }
            let next: Vec<String> = w.next_items.iter().map(|(wd, t)| format!("{t} on {wd}")).collect();
            s.push_str(&format!("Next week's notable things: {}\n", vl(&next)));
            s.push_str(&format!("Next week's workouts: {}\n", if w.next_workouts.is_empty() { "none planned".into() } else { w.next_workouts.join(", ") }));
        }
    }
    s.push_str(&goals::context_block(&c.goals, &c.goals_achieved, c.date));
    s
}

fn is_emoji(ch: char) -> bool {
    let u = ch as u32;
    u >= 0x1F000 || (0x2600..=0x27BF).contains(&u) || (0x2B00..=0x2BFF).contains(&u) || u == 0xFE0F || u == 0x200D
}

fn clean_script(raw: &str) -> String {
    let lines: Vec<String> = raw
        .lines()
        .map(|l| {
            let l = l.trim().trim_start_matches(['#', '-', '*', '•']).trim();
            l.chars().filter(|c| !matches!(c, '*' | '`' | '#') && !is_emoji(*c)).collect::<String>().trim().to_string()
        })
        .filter(|l| !l.is_empty())
        .collect();
    lines.join(" ")
}

/// Raw Gemini call with a configurable temperature and minimum reply length.
pub async fn gemini_raw(st: &AppState, key: &str, system: &str, user: &str, temperature: f64, min_len: usize) -> Result<String, String> {
    let model = std::env::var("GEMINI_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| "gemini-2.5-flash".into());
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let body = json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": [{"role": "user", "parts": [{"text": user}]}],
        "generationConfig": {"temperature": temperature, "maxOutputTokens": 4096}
    });
    let resp = st
        .http
        .post(url)
        .header("x-goog-api-key", key)
        .timeout(StdDuration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let v: Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("gemini returned {status}: {}", v["error"]["message"].as_str().unwrap_or("unknown error")));
    }
    let text: String = v["candidates"][0]["content"]["parts"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    let text = clean_script(&text);
    if text.len() < min_len {
        return Err("gemini returned too little text".into());
    }
    Ok(text)
}

/// True when the text reads out a count, fraction or percentage ("3 of 5", "7 out of 10", "percent", "%").
pub fn has_stats(t: &str) -> bool {
    if t.contains('%') || t.to_lowercase().contains("percent") {
        return true;
    }
    let w: Vec<String> = t.split_whitespace().map(|x| x.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).collect();
    let num = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
    (0..w.len()).any(|i| {
        num(&w[i]) && ((w.get(i + 1).is_some_and(|x| x == "of") && w.get(i + 2).is_some_and(|x| num(x)))
            || (w.get(i + 1).is_some_and(|x| x == "out") && w.get(i + 2).is_some_and(|x| x == "of") && w.get(i + 3).is_some_and(|x| num(x))))
    })
}

/// Gemini call with the no-stats guard: retries once with a stricter reminder, then errors (caller uses the template).
pub async fn gemini_guarded(st: &AppState, key: &str, system: &str, user: &str, temperature: f64, min_len: usize) -> Result<String, String> {
    let t = gemini_raw(st, key, system, user, temperature, min_len).await?;
    if !has_stats(&t) {
        return Ok(t);
    }
    tracing::warn!("gemini output contained counts/percentages, retrying once");
    let strict = format!(
        "{system}\nSTRICT REMINDER: your previous draft read out a count, fraction or percentage. Do NOT write anything like '3 of 5', '2 out of 7', 'percent' or a percent sign. Instead name the specific things (task titles, workouts) and use words like 'a couple' or 'a few'."
    );
    let t2 = gemini_raw(st, key, &strict, user, temperature, min_len).await?;
    if has_stats(&t2) { Err("gemini still produced counts or percentages".into()) } else { Ok(t2) }
}

// ----------------------------------------------------------------- handler

#[derive(Serialize)]
struct Stats {
    total: usize,
    done: usize,
    next_up: Option<Occurrence>,
}

#[derive(Serialize)]
pub struct BriefingOut {
    kind: &'static str,
    date: String,
    text: String,
    source: &'static str,
    weather: Option<Weather>,
    affirmation: String,
    stats: Stats,
}

pub async fn briefing(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Res<Json<BriefingOut>> {
    let kind = match q.get("kind").map(String::as_str).unwrap_or("morning") {
        "morning" => Kind::Morning,
        "evening" => Kind::Evening,
        "weekly" => Kind::Weekly,
        _ => return Err(AppError::bad("kind must be morning, evening or weekly")),
    };
    let date = match q.get("date") {
        Some(s) => parse_date(s).ok_or_else(|| AppError::bad("date must be YYYY-MM-DD"))?,
        None => today(),
    };
    crate::holidays::ensure(&st, &tasks::years_between(date - Duration::days(1), date + Duration::days(8))).await;
    let ctx = {
        let conn = st.db.lock().unwrap();
        build_ctx(&conn, kind, date)?
    };
    let weather = if kind == Kind::Weekly { None } else { weather::for_date(&st, ctx.lat, ctx.lon, &fmt_date(date)).await };

    let mut source = "template";
    let mut text = None;
    // `ai=false` skips Gemini (used by the home screen to fetch weather/affirmation without spending quota)
    let use_ai = q.get("ai").map(|v| v != "false" && v != "0").unwrap_or(true);
    let system = system_prompt(&ctx);
    let context = context_text(&ctx, &weather);
    let ds = fmt_date(date);
    let hash = digest::fnv(&format!("{system}\n{context}"));
    let cached: Option<String> = {
        let conn = st.db.lock().unwrap();
        conn.query_row("SELECT text FROM briefing_cache WHERE kind=?1 AND date=?2 AND hash=?3", rusqlite::params![kind.name(), ds, hash], |r| r.get(0)).ok()
    };
    if let Some(t) = cached {
        text = Some(t);
        source = "gemini";
    } else if let (true, Some(key)) = (use_ai, &ctx.key) {
        let min_len = if kind == Kind::Morning { 600 } else { 60 };
        match gemini_guarded(&st, key, &system, &context, 0.9, min_len).await {
            Ok(t) => {
                {
                    let conn = st.db.lock().unwrap();
                    if let Err(e) = conn.execute(
                        "INSERT INTO briefing_cache(kind,date,hash,text) VALUES(?1,?2,?3,?4) ON CONFLICT(kind,date,hash) DO UPDATE SET text=excluded.text",
                        rusqlite::params![kind.name(), ds, hash, t],
                    ) {
                        tracing::warn!("could not cache briefing: {e}");
                    }
                }
                text = Some(t);
                source = "gemini";
            }
            Err(e) => tracing::warn!("gemini failed, using template: {e}"),
        }
    }
    let text = text.unwrap_or_else(|| template_text(&ctx, &weather));

    let affirmation = match &ctx.week {
        Some(w) => week_line(w),
        None => daynote::note_for(&st, date, use_ai).await?.0,
    };
    let stats = match &ctx.week {
        Some(w) => Stats { total: w.total, done: w.done, next_up: None },
        None => Stats { total: ctx.items.len(), done: count_done(&ctx.items), next_up: next_up(&ctx.items, date) },
    };
    Ok(Json(BriefingOut { kind: kind.name(), date: fmt_date(date), text, source, weather, affirmation, stats }))
}

#[cfg(test)]
mod tests {
    use super::has_stats;
    #[test]
    fn stats_guard() {
        assert!(has_stats("You did 3 of 5 things."));
        assert!(has_stats("That is 7 out of 10, nice."));
        assert!(has_stats("About 50 percent done"));
        assert!(has_stats("80% there"));
        assert!(!has_stats("You lifted 62.5 kilos and did a couple of things."));
        assert!(!has_stats("Bench went from 60 to 62.5 kilos, one of the best."));
    }
}
