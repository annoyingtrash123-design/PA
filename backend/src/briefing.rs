use crate::daynote;
use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, Res};
use crate::goals::{self, Goal};
use crate::gym::{self, WorkoutPlan};
use crate::tasks::{self, Occurrence};
use crate::weather::{self, Weather};
use crate::{settings, AppState};
use axum::extract::{Query, State};
use axum::Json;
use chrono::{Datelike, Duration, Local, NaiveDate};
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
    };
    match kind {
        Kind::Morning | Kind::Evening => {
            let n = if kind == Kind::Evening { 2 } else { 1 };
            let mut days = tasks::build_days(conn, date, n)?;
            let first = days.remove(0);
            ctx.items = first.items;
            ctx.workout = first.workout;
            if kind == Kind::Evening {
                ctx.gym_done = gym::completed_session_on(conn, date)?.map(|(_, _, name)| name);
                if let Some(t) = days.pop() {
                    let d = parse_date(&t.date).unwrap_or(date + Duration::days(1));
                    ctx.tomorrow = Some((tasks::weekday_name(d), t.items, t.workout));
                }
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

// ----------------------------------------------------------------- helpers

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

fn join(v: &[String]) -> String {
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        2 => format!("{} and {}", v[0], v[1]),
        _ => format!("{} and {}", v[..v.len() - 1].join(", "), v[v.len() - 1]),
    }
}

fn limited(v: Vec<String>, max: usize) -> String {
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

fn pick<'a>(opts: &[&'a str], d: NaiveDate, salt: u64) -> &'a str {
    let h = (d.num_days_from_ce() as u64)
        .wrapping_add(salt.wrapping_mul(7919))
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    opts[((h >> 33) as usize) % opts.len()]
}

fn name_suffix(user: &str) -> String {
    if user.is_empty() { String::new() } else { format!(", {user}") }
}

fn item_phrase(o: &Occurrence) -> String {
    match &o.time {
        Some(t) => format!("{} at {}", o.title, say_time(t)),
        None => o.title.clone(),
    }
}

/// Timed items first as "X at 7:30 AM", then untimed grouped by part of day.
fn rundown(items: &[&Occurrence], d: NaiveDate) -> String {
    let timed: Vec<String> = items.iter().filter(|o| o.time.is_some()).map(|o| item_phrase(o)).collect();
    let mut out = vec![];
    if !timed.is_empty() {
        let lead = pick(&["On the schedule, you've got", "Timewise, you've got", "Your day has a few anchors:"], d, 11);
        out.push(format!("{lead} {}.", join(&timed)));
    }
    for (part, lead) in [
        ("morning", "For the morning, there's"),
        ("afternoon", "For the afternoon, there's"),
        ("evening", "And in the evening, there's"),
        ("anytime", "And whenever you find a gap, there's"),
    ] {
        let v: Vec<String> = items.iter().filter(|o| o.time.is_none() && o.part_of_day == part).map(|o| o.title.clone()).collect();
        if !v.is_empty() {
            out.push(format!("{lead} {}.", join(&v)));
        }
    }
    out.join(" ")
}

fn weather_line(w: &Weather, d: NaiveDate) -> String {
    let s = w.summary.to_lowercase();
    let base = match d.num_days_from_ce() % 2 {
        0 => format!("Looking outside, it's {s} today, with a high of {} degrees and a low of {}.", w.temp_max, w.temp_min),
        _ => format!("Weather-wise, expect {s}, topping out around {} degrees and dipping to {}.", w.temp_max, w.temp_min),
    };
    let tip = if s.contains("rain") || s.contains("drizzle") || s.contains("shower") || s.contains("thunder") {
        " Maybe keep an umbrella close."
    } else if s.contains("snow") {
        " Wrap up warm out there."
    } else if w.temp_max >= 26 {
        " Stay hydrated today."
    } else if w.temp_min <= 3 {
        " It's a chilly one, so bundle up."
    } else {
        ""
    };
    format!("{base}{tip}")
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
        let now = Local::now().format("%H:%M").to_string();
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

fn template_text(c: &Ctx, weather: &Option<Weather>) -> String {
    let d = c.date;
    let n = name_suffix(&c.user);
    let a = &c.assistant;
    let mut parts: Vec<String> = vec![];
    match c.kind {
        Kind::Morning => {
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
            if let Some(w) = weather {
                parts.push(weather_line(w, d));
            }
            let all: Vec<&Occurrence> = c.items.iter().collect();
            if all.is_empty() {
                parts.push("Your list is wide open today, which means you get to decide what kind of day this is. Lovely.".into());
            } else {
                parts.push(format!(
                    "{} {}",
                    pick(&["Here's how things look.", "So, what's on your plate?", "Let's run through it."], d, 7),
                    rundown(&all, d)
                ));
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
            // a goal link from today's tasks, if one is plausible (skip if the workout already made one)
            if !c.workout.as_ref().is_some_and(|p| !p.rest && goals::find_link(&c.goals, &[(p.name.clone(), format!("gym workout {}", p.name))]).is_some()) {
                let texts: Vec<(String, String)> = c.items.iter().filter(|o| !o.done).map(|o| (o.title.clone(), o.title.clone())).collect();
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
        }
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
            parts.push(pick(&["Sleep well.", "Rest up, I'll see you tomorrow.", "Goodnight."], d, 23).into());
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
        _ => "between 120 and 220 words",
    };
    let focus = match c.kind {
        Kind::Morning => "This is the MORNING briefing. Greet them, mention the weather if given, walk through the day in order (naturally mentioning times for timed tasks and the part of day for untimed ones), mention today's workout and any progression hint, tie today's tasks or workout to a relevant goal where it genuinely fits, and end with one grounded, specific closing line, such as the key task to start with. Never use generic affirmations or cliches like 'you\'ve got this'.",
        Kind::Evening => "This is the EVENING wind-down. Kindly say what got done by naming it (celebrate it, never guilt), mention what is left and that unfinished one-off tasks roll over to tomorrow, mention whether the gym happened, link what was done to a goal where it plausibly relates, give a gentle look at tomorrow, and end with one grounded, specific closing line (such as what comes first tomorrow) and a calm goodnight. No generic affirmations or cliches.",
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
        Some(w) => s.push_str(&format!("Weather: {}, high {} C, low {} C\n", w.summary, w.temp_max, w.temp_min)),
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
            o.push_str(&format!("  - {} ({when}; done: {}{steps}{roll})\n", i.title, if i.done { "yes" } else { "no" }));
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
    let ctx = {
        let conn = st.db.lock().unwrap();
        build_ctx(&conn, kind, date)?
    };
    let weather = if kind == Kind::Weekly { None } else { weather::for_date(&st, ctx.lat, ctx.lon, &fmt_date(date)).await };

    let mut source = "template";
    let mut text = None;
    // `ai=false` skips Gemini (used by the home screen to fetch weather/affirmation without spending quota)
    let use_ai = q.get("ai").map(|v| v != "false" && v != "0").unwrap_or(true);
    if let (true, Some(key)) = (use_ai, &ctx.key) {
        match gemini_guarded(&st, key, &system_prompt(&ctx), &context_text(&ctx, &weather), 0.9, 60).await {
            Ok(t) => {
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
