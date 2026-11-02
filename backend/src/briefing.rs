use crate::db::{fmt_date, parse_date, today};
use crate::error::{AppError, Res};
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

const AFFIRMATIONS: [&str; 40] = [
    "You don't have to do it all today, just the next thing.",
    "Small steps still count as steps.",
    "You are doing better than you think you are.",
    "Rest is productive too.",
    "Be as kind to yourself as you would be to a good friend.",
    "Progress, not perfection.",
    "You have handled hard days before, and you will handle this one too.",
    "Your pace is the right pace.",
    "It's okay to start again as many times as you need to.",
    "You are allowed to take up space and take your time.",
    "Today is a fresh page, and you get to decide what goes on it.",
    "You are more capable than your to-do list gives you credit for.",
    "Showing up counts, even on the messy days.",
    "Good things are built a little at a time.",
    "You deserve the same patience you give everyone else.",
    "One calm breath at a time is a perfectly good plan.",
    "You are growing, even when it doesn't feel like it.",
    "Done is better than perfect.",
    "Your effort matters, even when nobody sees it.",
    "You are enough, exactly as you are today.",
    "Be proud of how far you've come.",
    "Doing something is always better than waiting for the perfect moment.",
    "You can do hard things, and you can also take breaks.",
    "Let today be gentle and let it be good.",
    "Your best today might look different from yesterday, and that's fine.",
    "Every little win is worth celebrating.",
    "You get to choose where your energy goes today.",
    "Consistency beats intensity, so keep showing up.",
    "It's okay to ask for help, and it's okay to say no.",
    "You are building a life you can be proud of, one day at a time.",
    "Trust yourself, you know more than you think.",
    "Slow progress is still progress.",
    "Today you get to start with kindness, for yourself first.",
    "You have everything you need to take the next step.",
    "Rest when you need to, then keep going when you're ready.",
    "Your future self is already grateful for what you do today.",
    "Breathe in, breathe out, and begin where you are.",
    "Not everything has to be figured out today.",
    "You make a difference, even in the smallest ways.",
    "Whatever happens today, you will have done your best, and that is plenty.",
];

pub fn affirmation_for(d: NaiveDate) -> &'static str {
    AFFIRMATIONS[(d.num_days_from_ce() as usize) % AFFIRMATIONS.len()]
}

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
    days: Vec<(String, usize, usize)>,
    sessions: Vec<String>,
    prs: Vec<(String, f64, f64)>,
    next_total: usize,
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
            let mut w = Week { total: 0, done: 0, days: vec![], sessions: vec![], prs: vec![], next_total: 0, next_workouts: vec![] };
            for d in this {
                let (t, dn) = (d.items.len(), count_done(&d.items));
                w.total += t;
                w.done += dn;
                w.days.push((d.weekday.clone(), t, dn));
            }
            w.sessions = gym::sessions_between(conn, monday, monday + Duration::days(6))?.into_iter().map(|(_, n)| n).collect();
            w.prs = gym::weekly_prs(conn, monday, monday + Duration::days(6))?;
            for d in next {
                w.next_total += d.items.len();
                if let Some(p) = &d.workout {
                    if !p.rest {
                        w.next_workouts.push(p.name.clone());
                    }
                }
            }
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
        let extra = v.len() - max;
        format!("{}, plus {} more", v[..max].join(", "), extra)
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

// ---------------------------------------------------------------- template

fn template_text(c: &Ctx, weather: &Option<Weather>) -> String {
    let d = c.date;
    let n = name_suffix(&c.user);
    let a = &c.assistant;
    let aff = affirmation_for(d);
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
                parts.push(workout_sentence(p, d));
            }
            parts.push(format!("{} {aff}", pick(&["Before you head off, a little reminder:", "And here's something to carry with you today:", "One last thing:"], d, 13)));
            parts.push(pick(&["You've got this!", "I'm cheering for you all day long.", "Go have a lovely day, and I'll be right here if you need me."], d, 17).into());
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
                format!("You finished everything on your list today, all {total} of them. I'm honestly so proud of you.")
            } else if done == 0 {
                "The task list didn't quite happen today, and honestly, that's okay. Some days are just like that.".into()
            } else {
                format!("You got {done} of {total} things done today, including {}. That's real progress.", limited(done_titles, 3))
            });
            let left: Vec<String> = c.items.iter().filter(|o| !o.done).map(|o| o.title.clone()).collect();
            if !left.is_empty() {
                parts.push(format!(
                    "Still waiting on {}. Any one-off tasks will roll over to tomorrow on their own, so there's nothing to stress about.",
                    limited(left, 3)
                ));
            }
            if let Some(g) = &c.gym_done {
                parts.push(format!("And you got your {g} session in, which is fantastic."));
            } else if let Some(p) = &c.workout {
                if p.rest {
                    parts.push("Your rest day did its job too.".into());
                } else {
                    parts.push(format!("The gym didn't happen today, and that's fine. {} will be waiting for you.", p.name));
                }
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
            if let Some(w) = weather {
                parts.push(format!("It ended up as {} today.", w.summary.to_lowercase()));
            }
            parts.push(format!("{} {aff}", pick(&["Before you drift off, here's a thought:", "Let me leave you with this:"], d, 19)));
            parts.push(pick(&["Sleep well, and be gentle with yourself.", "Rest up, I'll see you tomorrow.", "Goodnight, you did enough today."], d, 23).into());
        }
        Kind::Weekly => {
            let w = c.week.as_ref().unwrap();
            parts.push(match d.num_days_from_ce() as usize % 3 {
                0 => format!("Hey{n}, it's {a}! It's time for our weekly look back, so settle in."),
                1 => format!("Hi there{n}, {a} here. Let's take a moment to look at the week you just had."),
                _ => format!("Hi again{n}! It's {a}, and I've got your week in review."),
            });
            if w.total == 0 {
                parts.push("You didn't have any tasks planned this week, so it was a lighter one on the list side.".into());
            } else {
                let pct = w.done * 100 / w.total;
                let react = if pct >= 80 {
                    "That's a seriously strong week."
                } else if pct >= 50 {
                    "That's a solid week, and you should feel good about it."
                } else {
                    "It was a lighter week, and that's totally fine. Not every week has to look the same."
                };
                parts.push(format!("This week you completed {} of {} tasks, which is {pct} percent. {react}", w.done, w.total));
                if let Some((wd, t, dn)) = w.days.iter().filter(|(_, _, dn)| *dn > 0).max_by_key(|(_, _, dn)| *dn) {
                    parts.push(format!("{wd} was your best day, with {dn} of {t} checked off."));
                }
            }
            let ns = w.sessions.len();
            if ns == 0 {
                parts.push("There were no gym sessions this week, and that's okay. A fresh start is always just one workout away.".into());
            } else {
                parts.push(format!(
                    "In the gym, you got {ns} session{} in: {}. {}",
                    if ns == 1 { "" } else { "s" },
                    join(&w.sessions),
                    if ns >= 3 { "That's real consistency." } else { "Every session counts." }
                ));
            }
            if !w.prs.is_empty() {
                let v: Vec<String> =
                    w.prs.iter().take(3).map(|(n, now, was)| format!("{n} went from {} to {} kilos", gym::fmt_w(*was), gym::fmt_w(*now))).collect();
                parts.push(format!("And for progress, {}. Look at you go.", join(&v)));
            }
            let mut ahead = format!("Looking ahead to next week, you've got {} task{} on the books", w.next_total, if w.next_total == 1 { "" } else { "s" });
            if !w.next_workouts.is_empty() {
                ahead.push_str(&format!(", and workouts coming up: {}", limited(w.next_workouts.clone(), 4)));
            }
            ahead.push('.');
            parts.push(ahead);
            parts.push(format!("{} {aff}", pick(&["Here's something to take into the new week:", "One last thought before the week begins:"], d, 29)));
            parts.push(pick(&["Be proud of yourself, I certainly am.", "Enjoy the rest of your weekend. I'm right here whenever you need me."], d, 31).into());
        }
    }
    parts.join(" ")
}

// ------------------------------------------------------------------ gemini

fn system_prompt(c: &Ctx) -> String {
    let words = match c.kind {
        Kind::Weekly => "between 200 and 300 words",
        _ => "between 120 and 220 words",
    };
    let focus = match c.kind {
        Kind::Morning => "This is the MORNING briefing. Greet them, mention the weather if given, walk through the day in order (naturally mentioning times for timed tasks and the part of day for untimed ones), mention today's workout and any progression hint, and end with the affirmation.",
        Kind::Evening => "This is the EVENING wind-down. Kindly say what got done (celebrate it, never guilt), mention what is left and that unfinished one-off tasks roll over to tomorrow, mention whether the gym happened, give a gentle look at tomorrow, and end with the affirmation and a calm goodnight.",
        Kind::Weekly => "This is the WEEKLY review, read on Sunday. Summarise tasks completed over the Monday to Sunday week, the gym sessions, any progress highlights, then look ahead to next week, and end with the affirmation.",
    };
    format!(
        "You are {a}, the user's warm, upbeat best friend and personal assistant. You are writing a script that will be read aloud by a text-to-speech voice, so write exactly how a chatty, caring friend would talk.\n\
         Rules: plain spoken sentences only. No markdown, no asterisks, no headings, no bullet points, no numbered lists, no emojis, no stage directions, no quotation marks around the whole text. \
         Say times naturally, like 'at seven thirty in the morning' or 'at 3 PM'. Say 'kilos' instead of 'kg'. Weather temperatures are in Celsius and should be said as degrees. \
         Only use facts from the context; never invent tasks, numbers or weather. Keep it {words}. Output only the script.\n{focus}",
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
            s.push_str(&format!("Today's items in order (done {} of {}):\n{}", count_done(&c.items), c.items.len(), list(&c.items)));
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
            s.push_str(&format!("Tasks this week (Mon-Sun): {} done out of {}\n", w.done, w.total));
            for (wd, t, d) in &w.days {
                s.push_str(&format!("  - {wd}: {d} of {t}\n"));
            }
            s.push_str(&format!("Gym sessions completed: {}{}\n", w.sessions.len(), if w.sessions.is_empty() { String::new() } else { format!(" ({})", w.sessions.join(", ")) }));
            for (n, now, was) in &w.prs {
                s.push_str(&format!("Progress: {n} top weight up from {} to {} kilos\n", gym::fmt_w(*was), gym::fmt_w(*now)));
            }
            s.push_str(&format!("Next week: {} tasks scheduled; workouts: {}\n", w.next_total, if w.next_workouts.is_empty() { "none planned".into() } else { w.next_workouts.join(", ") }));
        }
    }
    s.push_str(&format!("Affirmation to close with: {}\n", affirmation_for(c.date)));
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

async fn gemini(st: &AppState, key: &str, system: &str, user: &str) -> Result<String, String> {
    let model = std::env::var("GEMINI_MODEL").ok().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| "gemini-2.5-flash".into());
    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent");
    let body = json!({
        "systemInstruction": {"parts": [{"text": system}]},
        "contents": [{"role": "user", "parts": [{"text": user}]}],
        "generationConfig": {"temperature": 0.9, "maxOutputTokens": 4096}
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
    if text.len() < 60 {
        return Err("gemini returned too little text".into());
    }
    Ok(text)
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
    affirmation: &'static str,
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
        match gemini(&st, key, &system_prompt(&ctx), &context_text(&ctx, &weather)).await {
            Ok(t) => {
                text = Some(t);
                source = "gemini";
            }
            Err(e) => tracing::warn!("gemini failed, using template: {e}"),
        }
    }
    let text = text.unwrap_or_else(|| template_text(&ctx, &weather));

    let stats = match &ctx.week {
        Some(w) => Stats { total: w.total, done: w.done, next_up: None },
        None => Stats { total: ctx.items.len(), done: count_done(&ctx.items), next_up: next_up(&ctx.items, date) },
    };
    Ok(Json(BriefingOut { kind: kind.name(), date: fmt_date(date), text, source, weather, affirmation: affirmation_for(date), stats }))
}
