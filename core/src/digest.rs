//! Home-screen digest: a weather comment (forecast read through the saved weather preferences)
//! and an overview of the day in prose. The core returns prompts, cache key and template text.
use crate::daynote::{self, part_idx, PARTS};
use crate::env::Env;
use crate::error::Res;
use crate::goals;
use crate::gym::WorkoutPlan;
use crate::memory;
use crate::model::{Goal, Memory, WeatherPrefs, Weather};
use crate::tasks::{self, DayInfo, Occurrence};
use crate::util::*;
use chrono::{Datelike, NaiveDate};
use serde_json::{json, Value};

pub fn item_phrase(o: &Occurrence) -> String {
    match &o.time {
        Some(t) => format!("{} at {}", o.title, say_time(t)),
        None => o.title.clone(),
    }
}

/// A warm one-liner about the work situation of a day, only when it is notable or useful.
pub fn frame_sentence(info: &DayInfo) -> Option<String> {
    if let Some(h) = &info.holiday {
        return Some(format!("It's a public holiday, {h}, so there's no work today."));
    }
    if info.day_off {
        return Some("It's a day off, so there's no work today.".into());
    }
    info.work.as_ref().map(|w| format!("You're working from {} to {} today.", say_time(&w.start), say_time(&w.end)))
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
    /// rain, but above the "hot" threshold: muggy, not the cosy rain they enjoy
    HotRain,
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

fn feel(w: &Weather, p: &WeatherPrefs) -> Feel {
    let t = w.temp_max as f64;
    if w.summary.to_lowercase().contains("snow") {
        Feel::Snow
    } else if is_rainy(w) && t > p.hot_above {
        Feel::HotRain
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
    let v = |o: Vec<String>| -> String { o[idx(d, 101 + t as u64, o.len())].clone() };
    let first = match f {
        Feel::Snow => v(vec![format!("There's snow in the forecast, and it's only getting up to {t} degrees."), format!("It's a snowy one today, and it's topping out at {t} degrees.")]),
        Feel::Rain if p.likes_rain => v(vec![
            format!("It's a proper rainy one today, only getting up to {t} degrees, so it should be lovely and cosy."),
            format!("It's going to rain all day, with a cool {t} degrees, so it's a nice one to be inside for."),
            format!("It's wet and grey at {t} degrees today, which is the soft, rainy kind of day."),
        ]),
        Feel::Rain => v(vec![
            format!("It's a wet one today, only getting up to {t} degrees, so keep an umbrella close."),
            format!("There's rain on the way and it's only {t} degrees at best, so it's not the nicest day to be outside."),
        ]),
        Feel::HotRain if t as f64 > p.too_hot_above => v(vec![
            format!("It's going to be hot and muggy today, getting up to {t} degrees with showers around. I'd wear something light, keep some water with you, and bring an umbrella."),
            format!("It's a sticky {t} degrees with rain about, so try to keep cool and keep an umbrella handy."),
        ]),
        Feel::HotRain => v(vec![
            format!("There are showers today, but it's warm with it at {t} degrees, so it'll feel a bit muggy. I'd dress light and keep an umbrella close."),
            format!("There's rain around and it's a warm {t} degrees, so I'd wear lighter layers and bring an umbrella."),
        ]),
        Feel::TooHot => v(vec![
            format!("It's a hot one today, getting up to {t} degrees. Keep water close and take it slowly in the afternoon."),
            format!("It's properly hot today at {t} degrees, so go easy and stay cool where you can."),
        ]),
        Feel::Hot => v(vec![
            format!("It's warm and heading toward hot today, getting up to {t} degrees, so I'd wear lighter clothes and drink plenty of water."),
            format!("It's a warm {t} degrees today, so I'd dress light."),
        ]),
        Feel::Warm => v(vec![
            format!("It's a mild, slightly warm {t} degrees today."),
            format!("It's pleasantly warm at {t} degrees, so there's nothing to fuss about."),
        ]),
        Feel::Comfy => v(vec![
            format!("It's lovely and cool today, only getting up to about {t} degrees."),
            format!("It's a cool, comfortable {t} degrees today."),
            format!("It's nice and cool today, getting up to {t} degrees at most, so it's an easy one."),
        ]),
        Feel::Cold => v(vec![format!("It's a chilly one today, only {t} degrees at best, so layer up."), format!("It's properly cold today, just {t} degrees at the top, so wrap up well.")]),
    };

    let second: Option<String> = if sunny_uv {
        Some(match f {
            Feel::TooHot | Feel::Hot => "The sun is strong too, so put on sun cream before you head out and find some shade where you can.".to_string(),
            _ => v(vec!["The UV is up, so put some sun cream on if you'll be outside for a while.".into(), "Sun cream is worth it today, since the UV gets high.".into()]),
        })
    } else {
        match (&pl, f) {
            (Plans::Outdoor(t), Feel::Rain | Feel::Snow | Feel::HotRain) => Some(format!("Maybe plan {t} around the weather, or take a proper coat.")),
            (Plans::Outdoor(t), Feel::TooHot | Feel::Hot) => Some(format!("I'd do {t} early or late, away from the heat.")),
            (Plans::Outdoor(t), _) => Some(format!("It should be a nice day for {t}.")),
            (Plans::Office, Feel::Rain) if p.likes_rain => Some("It's nice to listen to from your desk, though make sure you take an umbrella for the commute.".into()),
            (Plans::Office, Feel::Rain | Feel::Snow) => Some("Make sure you take an umbrella and a coat for the commute.".into()),
            (Plans::Office, Feel::HotRain) => Some("Make sure you take an umbrella for the commute, and keep a bottle of water on your desk.".into()),
            (Plans::Office, Feel::TooHot | Feel::Hot) => Some("It sounds like an office day, so keep a bottle of water on your desk.".into()),
            (Plans::Office, _) => Some("It's easy weather for the office commute.".into()),
            (Plans::Desk, Feel::Rain) if p.likes_rain => Some("It's a cosy backdrop for the work you have on.".into()),
            (Plans::Desk, Feel::TooHot | Feel::Hot) => Some("Try to keep cool while you get through your work.".into()),
            (Plans::Off, Feel::Rain | Feel::Snow) => Some("It's a lovely excuse for a slow day off.".into()),
            (Plans::Off, Feel::TooHot | Feel::Hot) => Some("It's a good day off for staying cool and doing very little.".into()),
            (Plans::Off, _) => Some("It should be a lovely day to enjoy your day off.".into()),
            (Plans::Gym, Feel::TooHot | Feel::Hot | Feel::HotRain) => Some("Make sure you drink plenty of water before the gym.".into()),
            (Plans::Gym, Feel::Rain) => Some("At least the rain makes the gym an easy choice for the main outing.".into()),
            (_, Feel::Rain) if p.likes_rain => Some("It's cosy if you're home, and it's nice to listen to from the window.".into()),
            _ => None,
        }
    };
    match second {
        Some(s) => format!("{first} {s}"),
        None => first,
    }
}

// ---------------------------------------------------------------- overview

/// Bucket of the day: 0 = early, 1 = middle, 2 = late. On work days that is before / during / after work.
fn bucket(o: &Occurrence, info: &DayInfo) -> Option<usize> {
    if let (Some(w), Some(t)) = (&info.work, &o.time) {
        let m = minutes(t);
        return Some(if m < minutes(&w.start) {
            0
        } else if m < minutes(&w.end) {
            1
        } else {
            2
        });
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
            "Nothing is planned beyond work, so the time before and after it is yours. It's a good moment for something you keep putting off, or for properly switching off.".to_string()
        } else {
            "Nothing is planned today, so it's a wide-open day. Use it for something you keep putting off, or leave it empty on purpose.".to_string()
        };
        if let Some(w) = workout.as_ref().filter(|w| !w.rest) {
            if w.is_cardio() {
                s.push_str(" There's some cardio lined up if you fancy it.");
            } else {
                s.push_str(&format!(" The gym has {} lined up if you fancy it.", w.name));
            }
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
    // name the meaningful things first, routines last
    order.sort_by_key(|o| daynote::is_routine(o));
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
            slot.0.push(crate::taskvoice::inline_phrase(o));
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
                1 => (format!("during work hours there's {}, ideally around the middle of the day", phr(&parts[1])), "work fills the middle of the day".to_string()),
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
        out.push(lead(format!("Nothing is pinned to a particular time today: {} can go wherever it fits best.", phr(&anytime))));
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
            out.push(if w.is_cardio() { "A bit of cardio is on the cards too.".to_string() } else { format!("{} is on at the gym too.", w.name) });
        }
    }
    out.join(" ")
}

// ------------------------------------------------------------------ prompts

pub const SYSTEM: &str = concat!(spoken_rule!(), "You write two short texts for the home screen of a personal planner app, about ONE specific day, in the voice of a warm, chatty friend who knows the person. \
Reply with ONLY a JSON object: {\"weather_comment\": string or null, \"overview\": string}. No markdown, no code fences, no emojis. \
weather_comment: 1 to 2 sentences on the forecast whose tone and advice are quietly shaped by the person's weather preferences (cool days get a happy tone; hot days a gentle heads-up and practical advice; sun cream or shade when UV is high and they want to avoid sunburn; rain sounds cosy if they like it and it is cool), WITHOUT ever stating or explaining those preferences and, where natural, relating it to the day's plans (an office day versus a home day, outdoor tasks, the gym). Temperatures may be said as degrees (e.g. 14 degrees), but never quote the rain probability as a percentage. Set it to null if no weather is given. Only mention an office or home context if a note or task suggests it. \
overview: 2 to 4 short sentences in plain prose organising the day: its shape (what the morning is about, the afternoon, the evening; on a work day, before work, during work and after work, with work as a fixed block that is not a task; on a public holiday or day off, say warmly that there is no work), naming the key things, pointing out a breather, tying to a goal only where it plausibly relates. NEVER state raw counts, fractions or percentages (no '5 tasks', '3 of 4', '50 percent'); small natural quantities like 'a couple of things' or 'a few smaller bits' are fine. Never a bare list of every task: name at most four things and summarise the rest. You may mention the workout briefly. No cliches such as 'you've got this'. Do not invent tasks, times or numbers.");

pub fn weather_line(w: &Option<Weather>, quote_hint: &str) -> String {
    match w {
        Some(w) => {
            // the summary is a label, not a sentence: tell the model so it never says "it was thunderstorm"
            let mut s = format!("Weather (the summary is only a label, so say it naturally, for example 'a thunderstorm' or 'some rain', never 'it was thunderstorm'): {}, high {} C, low {} C", w.summary, w.temp_max, w.temp_min);
            if let Some(p) = w.precip_prob {
                s.push_str(&format!(", rain chance {} ({quote_hint})", if p >= 70 { "high" } else if p >= 40 { "moderate" } else { "low" }));
            }
            if let Some(u) = w.uv_max {
                s.push_str(&format!(", UV index {u}"));
            }
            s.push('\n');
            s
        }
        None => String::new(),
    }
}

fn context(items: &[Occurrence], d: NaiveDate, goals: &[Goal], mem: &Memory, w: &Option<Weather>, workout: &Option<WorkoutPlan>, info: &DayInfo) -> String {
    let mut s = format!("Date: {}, {}\n", weekday_name(d), d.format("%B %-d"));
    match w {
        Some(_) => s.push_str(&weather_line(w, "do not quote")),
        None => s.push_str("Weather: unavailable (set weather_comment to null)\n"),
    }
    s.push_str(&context_line(info, "Today's"));
    s.push_str("The day's items in order:\n");
    if items.is_empty() {
        s.push_str("  (nothing scheduled)\n");
    }
    for o in items {
        let when = match &o.time {
            Some(t) => format!("at {}", say_time(t)),
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

/// `GET /api/digest?date=`
pub fn digest(env: &mut Env, date: NaiveDate) -> Res<Value> {
    let ds = fmt_date(date);
    let mut days = tasks::build_days(env, date, 1);
    let day = days.remove(0);
    let info = day.info();
    let (items, workout) = (day.items, day.workout);
    let goals = goals::list_active(&env.st.goals);
    let mem = env.st.memory.clone();
    let weather = env.ctx.weather.get(&ds).cloned();
    let user = context(&items, date, &goals, &mem, &weather, &workout, &info);
    let hash = fnv(&format!("{SYSTEM}\n{}", user.replace("; done: yes", "; done: no")));
    let weather_comment = weather.as_ref().map(|w| weather_comment_template(w, &mem, &items, &workout, &info, date));
    let overview = overview_template(&items, &goals, &workout, &info, date);
    Ok(json!({
        "date": ds,
        "weather": weather,
        "cache_key": format!("digest:{ds}:{hash}"),
        "system_prompt": SYSTEM,
        "user_prompt": user,
        "template": {"weather_comment": weather_comment, "overview": overview},
        "weather_comment": weather_comment,
        "overview": overview,
        "source": "template",
    }))
}
