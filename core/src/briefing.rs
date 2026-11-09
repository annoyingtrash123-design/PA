//! Morning / evening / weekly briefings: context builders, template generators and Gemini prompts.
//! The core never calls the network: it returns the prompts, a cache key and the template text.
use crate::daynote::{self, is_routine};
use crate::digest::{self, item_phrase};
use crate::env::Env;
use crate::error::{ApiErr, Res};
use crate::goals;
use crate::gym::{self, WorkoutPlan};
use crate::knowledge;
use crate::leisure;
use crate::memory;
use crate::news;
use crate::model::{Goal, Leisure, Memory, Weather};
use crate::taskvoice::{self, Look};
use crate::tasks::{self, DayInfo, Occurrence};
use crate::util::*;
use chrono::{Datelike, Duration, NaiveDate};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Morning,
    Evening,
    Weekly,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Morning => "morning",
            Kind::Evening => "evening",
            Kind::Weekly => "weekly",
        }
    }
    fn max_words(self) -> usize {
        match self {
            Kind::Morning => 760,
            Kind::Evening => 650,
            Kind::Weekly => 300,
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
    /// completed cardio sessions as natural phrases ("a swim", "40 minutes on the rowing machine")
    cardio: Vec<String>,
    /// exercises that improved: (name, now, was, assisted)
    prs: Vec<(String, f64, f64, bool)>,
    /// notable things next week: (weekday, title)
    next_items: Vec<(String, String)>,
    next_workouts: Vec<String>,
}

struct Ctx {
    kind: Kind,
    date: NaiveDate,
    today: NaiveDate,
    now: String,
    user: String,
    assistant: String,
    items: Vec<Occurrence>,
    workout: Option<WorkoutPlan>,
    gym_done: Option<String>,
    /// set when the session done today was a cardio one: a natural phrase
    cardio_done: Option<String>,
    tomorrow: Option<(String, Vec<Occurrence>, Option<WorkoutPlan>)>,
    /// the days after tomorrow (weekday, items), for a glance further ahead
    ahead: Vec<(String, Vec<Occurrence>)>,
    week: Option<Week>,
    goals: Vec<Goal>,
    goals_achieved: Vec<Goal>,
    /// open leisure items, current first (morning/evening)
    leisure: Vec<Leisure>,
    /// the one item that may be suggested today, and whether the day has free room
    leisure_pick: Option<Leisure>,
    leisure_room: bool,
    leisure_open_day: bool,
    /// morning only: ask the model for an optional 'on this day' sentence
    on_this_day: bool,
    /// finished this week (weekly only)
    leisure_finished: Vec<Leisure>,
    memory: Memory,
    yesterday: Vec<Occurrence>,
    info: DayInfo,
    tomorrow_info: DayInfo,
    /// timed tasks / appointments whose day passed (morning only), oldest first
    missed: Vec<Occurrence>,
    /// morning / evening only: the news stories to cover (None = no news)
    news: Option<news::Plan>,
}

fn count_done(items: &[Occurrence]) -> usize {
    items.iter().filter(|o| o.done).count()
}

fn build_ctx(env: &mut Env, kind: Kind, date: NaiveDate) -> Ctx {
    let mut ctx = Ctx {
        kind,
        date,
        today: env.today,
        now: env.now(),
        user: env.st.settings.user_name.clone(),
        assistant: env.st.settings.assistant_name.clone(),
        items: vec![],
        workout: None,
        gym_done: None,
        cardio_done: None,
        tomorrow: None,
        ahead: vec![],
        week: None,
        goals: goals::list_active(&env.st.goals),
        goals_achieved: vec![],
        leisure: vec![],
        leisure_pick: None,
        leisure_room: false,
        leisure_open_day: false,
        on_this_day: false,
        leisure_finished: vec![],
        memory: env.st.memory.clone(),
        yesterday: vec![],
        info: DayInfo::default(),
        tomorrow_info: DayInfo::default(),
        missed: vec![],
        news: None,
    };
    match kind {
        Kind::Morning | Kind::Evening => {
            let morning = kind == Kind::Morning;
            let start = if morning { date - Duration::days(1) } else { date };
            let mut days = tasks::build_days(env, start, if morning { 6 } else { 2 });
            if morning {
                let y = days.remove(0);
                ctx.yesterday = y.items;
            }
            if morning {
                ctx.missed = tasks::missed_list(env.st);
            }
            let first = days.remove(0);
            ctx.info = first.info();
            ctx.items = first.items;
            ctx.workout = first.workout;
            if kind == Kind::Evening {
                ctx.gym_done = gym::completed_session_on(env.st, date).map(|(_, _, name)| name);
                ctx.cardio_done = gym::cardio_done_on(env.st, date).map(|c| gym::cardio_phrase(&c));
            }
            let evening = kind == Kind::Evening;
            ctx.on_this_day = morning && leisure::on_this_day(date, &ctx.items);
            ctx.leisure = leisure::list_all(&env.st.leisure).into_iter().filter(|l| l.status != "finished").collect();
            ctx.leisure_room = leisure::free_room(&ctx.items, &ctx.info, date, evening);
            ctx.leisure_open_day = leisure::free_room(&ctx.items, &ctx.info, date, false);
            let wx = env.ctx.weather.get(&fmt_date(date)).cloned();
            ctx.leisure_pick = leisure::suggest(&env.st.leisure, &env.st.memory.interests, date, &wx, ctx.leisure_room, evening);
            if !days.is_empty() {
                let t = days.remove(0);
                let d = parse_date(&t.date).unwrap_or(date + Duration::days(1));
                ctx.tomorrow_info = t.info();
                ctx.tomorrow = Some((weekday_name(d), t.items, t.workout));
            }
            for a in days {
                let d = parse_date(&a.date).unwrap_or(date);
                ctx.ahead.push((weekday_name(d), a.items));
            }
        }
        Kind::Weekly => {
            let monday = date - Duration::days(date.weekday().num_days_from_monday() as i64);
            let days = tasks::build_days(env, monday, 14);
            let (this, next) = days.split_at(7);
            let mut w = Week { total: 0, done: 0, done_titles: vec![], slid_titles: vec![], sessions: vec![], cardio: vec![], prs: vec![], next_items: vec![], next_workouts: vec![] };
            let mut routine_done: Vec<String> = vec![];
            for d in this {
                w.total += d.items.len();
                w.done += count_done(&d.items);
                for o in &d.items {
                    let list = if o.done {
                        if o.recurring {
                            &mut routine_done
                        } else {
                            &mut w.done_titles
                        }
                    } else if !o.recurring && parse_date(&o.date).is_some_and(|x| x < env.today) {
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
            for (_, n, c) in gym::sessions_between(env.st, monday, monday + Duration::days(6)) {
                match c {
                    Some(c) if !c.activity.trim().is_empty() => w.cardio.push(gym::cardio_phrase(&c)),
                    Some(_) => {}
                    None => w.sessions.push(n),
                }
            }
            w.prs = gym::weekly_prs(env.st, monday, monday + Duration::days(6));
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
            ctx.goals_achieved = goals::achieved_between(&env.st.goals, monday, monday + Duration::days(6));
            ctx.leisure_finished = leisure::finished_between(&env.st.leisure, monday, monday + Duration::days(6));
            ctx.week = Some(w);
        }
    }
    if kind != Kind::Weekly && date == env.today {
        let busy = ctx.items.len();
        ctx.news = news::plan(env.st, &env.ctx.news, &env.ctx.news_generated_at, kind, date, busy, None, false);
    }
    ctx
}

// ----------------------------------------------------------------- helpers

fn hint_sentence(p: &WorkoutPlan) -> Option<String> {
    let mut fallback = None;
    for e in &p.exercises {
        if let (Some(l), Some(sw)) = (&e.last, e.suggested_weight) {
            let last_w = l.sets.iter().map(|s| s.weight).fold(0.0, f64::max);
            if e.assisted && sw < last_w {
                return Some(format!("Last time you hit every rep on {}, so try {} kilos of counterweight today.", e.name, gym::fmt_w(sw)));
            }
            if !e.assisted && sw > last_w {
                return Some(format!("Last time you hit every rep on {}, so go for {} kilos today.", e.name, gym::fmt_w(sw)));
            }
            fallback.get_or_insert(format!("Last time you lifted {} kilos on {}, so see if you can beat those reps.", gym::fmt_w(sw), e.name));
        }
    }
    fallback
}

fn join_or(v: &[String]) -> String {
    match v {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} or {b}"),
        _ => format!("{} or {}", v[..v.len() - 1].join(", "), v[v.len() - 1]),
    }
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
    if p.is_cardio() {
        let opts: Vec<String> = p.options.iter().map(|o| o.to_lowercase()).collect();
        let mins = match (p.minutes_min, p.minutes_max) {
            (Some(a), Some(b)) if a != b => format!(" for {a} to {b} minutes"),
            (Some(a), _) => format!(" for {a} minutes"),
            _ => String::new(),
        };
        return if opts.is_empty() { format!("Today is a cardio day{mins}, which is good for your heart, lungs and endurance.") } else { format!("Today is a cardio day, which is good for your heart, lungs and endurance, so pick {}{mins}.", join_or(&opts)) };
    }
    let names: Vec<String> = p.exercises.iter().map(|e| e.name.clone()).collect();
    let mut s = match gym::trains(p) {
        Some(t) => format!("On the gym front, today is {}, which works your {t}.", p.name),
        None => format!("On the gym front, today is {}.", p.name),
    };
    if !names.is_empty() {
        if names.len() > 3 {
            s.push_str(&format!(" You'll be doing {} to start, with a few more after that.", join(&names[..3])));
        } else {
            s.push_str(&format!(" You'll be doing {}.", join(&names)));
        }
    }
    if let Some(h) = hint_sentence(p) {
        s.push(' ');
        s.push_str(&h);
    }
    s
}

/// Tomorrow's workout in one or two sentences: what it trains and the key lift.
fn gym_preview(p: &WorkoutPlan) -> String {
    if p.rest {
        return "And it's a rest day at the gym.".into();
    }
    let mut s = match gym::trains(p) {
        Some(t) if p.is_cardio() => format!("The gym has {} lined up, which is good for your {t}.", p.name),
        Some(t) => format!("The gym has {} lined up, which works your {t}.", p.name),
        None => format!("The gym has {} lined up.", p.name),
    };
    if p.is_cardio() {
        if !p.options.is_empty() {
            let opts: Vec<String> = p.options.iter().map(|o| o.to_lowercase()).collect();
            s.push_str(&format!(" You could pick {}.", join_or(&opts)));
        }
    } else if let Some(e) = p.exercises.first() {
        s.push_str(&format!(" The key lift is {}, so warm up properly before the heavy sets.", e.name));
        if let Some(h) = hint_sentence(p) {
            s.push(' ');
            s.push_str(&h.replace("today", "tomorrow"));
        }
    }
    s
}

fn next_up(items: &[Occurrence], date: NaiveDate, today: NaiveDate, now: &str) -> Option<Occurrence> {
    let undone: Vec<&Occurrence> = items.iter().filter(|o| !o.done).collect();
    if date == today {
        if let Some(o) = undone.iter().find(|o| o.time.as_deref().is_some_and(|t| t >= now)) {
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
    if tasks.is_empty() && w.sessions.is_empty() && w.cardio.is_empty() {
        "It was a quiet week, and next week is a fresh page.".into()
    } else if !tasks.is_empty() {
        format!("It's been a week with {} in it, and there's some good momentum to carry forward.", join(&tasks))
    } else if !w.sessions.is_empty() {
        format!("It's been a week with {} at the gym, and that counts for a lot.", join(&w.sessions))
    } else {
        format!("It's been a week with {}, and that counts for a lot.", join(&w.cardio))
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

// ------------------------------------------------------------ morning summary

const BIG_WORDS: [&str; 11] = ["report", "essay", "clean", "revise", "revision", "project", "study", "tidy", "write", "organise", "organize"];
const QUICK_WORDS: [&str; 10] = ["call", "pay", "reply", "book", "email", "text", "order", "send", "check", "buy"];

/// Rough size guess from title, notes and checklist (there is no size field).
pub fn is_big(o: &Occurrence) -> bool {
    if let Some(t) = crate::durations::typical(o) {
        return t.max >= 40;
    }
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
    if cat_name(o) == "Work" {
        return false;
    }
    let w: Vec<String> = o.title.to_lowercase().split(|c: char| !c.is_alphanumeric()).map(|s| s.to_string()).collect();
    let far = crate::durations::typical(o).is_some_and(|t| (t.outing && t.max > 30) || t.max > 45);
    o.time.is_none() && !far && !is_big(o) && o.part_of_day != "evening" && w.iter().any(|x| ["call", "pay", "book", "buy", "grocery", "shopping", "post", "pick", "order", "bill", "dentist", "doctor", "bank"].contains(&x.as_str()))
}

fn cat_name(o: &Occurrence) -> String {
    o.category.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "Other".into())
}

/// The theme an item is grouped under: its category, except that planning and routine habits
/// (e.g. a recurring "Plan tomorrow" filed under Work) count as looking after yourself.
fn group_name(o: &Occurrence) -> String {
    if is_routine(o) {
        "Self-care".into()
    } else {
        cat_name(o)
    }
}

/// Routines named in one breath, timed ones first, as they read mid-sentence ("your morning routine at 7:30 AM and planning tomorrow").
fn routine_list<T: std::borrow::Borrow<Occurrence>>(routines: &[T]) -> Vec<String> {
    let mut r: Vec<&Occurrence> = routines.iter().map(|o| o.borrow()).collect();
    r.sort_by_key(|o| o.time.is_none());
    r.iter()
        .map(|o| {
            let p = taskvoice::inline_phrase(o);
            let noun = matches!(taskvoice::task_phrase(&o.title), taskvoice::Phrase::Noun(_));
            if noun && p.starts_with(|c: char| c.is_lowercase()) && p.contains("routine") { format!("your {p}") } else { p }
        })
        .collect()
}

fn cat_lead(cat: &str, routine: bool, list: &str, d: NaiveDate, salt: u64) -> String {
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
        "Self-care" if routine => v(format!("For looking after yourself and keeping your rhythm, there's {list}."), format!("On the routine side, for looking after yourself, you've got {list}.")),
        "Self-care" => v(format!("For looking after yourself, there's {list}."), format!("On the self-care side you've got {list}.")),
        _ => v(format!("And on the side, you've got {list}."), format!("Then there's {list} on the side.")),
    }
}


#[derive(Clone, Copy, PartialEq)]
enum PK {
    /// always included
    Core,
    /// included, but dropped (highest rank first) when the text runs over the limit
    Drop(u8),
    /// real extra content, added (lowest rank first) when the text runs short
    Extra(u8),
}

struct Piece {
    text: String,
    kind: PK,
}

fn assemble(pieces: Vec<Piece>, min: usize, max: usize) -> String {
    let mut incl: Vec<bool> = pieces.iter().map(|p| !matches!(p.kind, PK::Extra(_))).collect();
    let total = |incl: &[bool]| -> usize { pieces.iter().zip(incl).filter(|(_, i)| **i).map(|(p, _)| words_in(&p.text)).sum() };
    while total(&incl) > max {
        let cand = pieces.iter().enumerate().filter(|(i, p)| incl[*i] && matches!(p.kind, PK::Drop(_))).max_by_key(|(i, p)| (if let PK::Drop(r) = p.kind { r } else { 0 }, *i));
        match cand {
            Some((i, _)) => incl[i] = false,
            None => break,
        }
    }
    while total(&incl) < min {
        let now = total(&incl);
        let mut cands: Vec<(u8, usize)> = pieces.iter().enumerate().filter(|(i, p)| !incl[*i] && matches!(p.kind, PK::Extra(_))).map(|(i, p)| (if let PK::Extra(r) = p.kind { r } else { 0 }, i)).collect();
        cands.sort();
        match cands.into_iter().find(|(_, i)| now + words_in(&pieces[*i].text) <= max) {
            Some((_, i)) => incl[i] = true,
            None => break,
        }
    }
    // a piece that is just a paragraph break joins its neighbours with a blank line
    pieces.iter().zip(&incl).filter(|(_, i)| **i).map(|(p, _)| p.text.as_str()).collect::<Vec<_>>().join(" ").replace(" \n\n ", "\n\n")
}

fn must(v: &mut Vec<Piece>, s: String) {
    v.push(Piece { text: s, kind: PK::Core });
}

fn brk(v: &mut Vec<Piece>) {
    v.push(Piece { text: "\n\n".into(), kind: PK::Core });
}

fn add(v: &mut Vec<Piece>, s: String, k: PK) {
    v.push(Piece { text: s, kind: k });
}

/// The four spoken parts of the morning briefing.
pub struct MorningParts {
    pub greeting: String,
    pub weather_comment: Option<String>,
    pub affirmation: String,
    pub briefing: String,
}

impl MorningParts {
    /// All parts as one text, blank-line separated (the no-AI `template_text`).
    pub fn joined(&self) -> String {
        self.joined_with_news(None)
    }

    /// Same, with the news segment inserted at its position (0 = before the briefing, else inside it).
    pub fn joined_with_news(&self, news: Option<(&str, usize)>) -> String {
        let mut v: Vec<String> = vec![self.greeting.clone()];
        if let Some(w) = self.weather_comment.as_deref().filter(|w| !w.is_empty()) {
            v.push(w.to_string());
        }
        v.push(self.affirmation.clone());
        match news {
            Some((n, pos)) => v.push(news::compose(&self.briefing, n, pos)),
            None => v.push(self.briefing.clone()),
        }
        v.retain(|p| !p.is_empty());
        v.join("\n\n")
    }
}

fn morning_parts(c: &Ctx, weather: &Option<Weather>) -> MorningParts {
    let weather_comment = weather.as_ref().map(|w| digest::weather_comment_template(w, &c.memory, &c.items, &c.workout, &c.info, c.date));
    MorningParts {
        greeting: morning_greeting(c),
        weather_comment,
        // the holiday / day-off frame is already the first line of the briefing body
        affirmation: daynote::template_note(&c.items, c.date, &[]),
        briefing: morning_template(c),
    }
}

/// One short, natural spoken hello. Never opens with a well-wish ("I hope you have a lovely ..."): those belong in the closing.
fn morning_greeting(c: &Ctx) -> String {
    let d = c.date;
    let n = name_suffix(&c.user);
    let wd = weekday_name(d);
    let mo = d.format("%B").to_string();
    match d.num_days_from_ce() as usize % 6 {
        0 => format!("Morning{n}!"),
        1 => format!("Good morning{n}. Hope you slept well."),
        2 => format!("Morning{n}! Grab a cuppa and let's look at today."),
        3 => format!("Morning{n}! It's {wd}, the {} of {mo}.", ordinal(d.day())),
        4 => format!("Hey{n}, good morning! Let's see what the day's got for us."),
        _ => format!("Rise and shine{n}! It's {wd}, {mo} {}.", ordinal(d.day())),
    }
}

/// A small spoken transition that opens a section, with a natural pause or filler (deterministic by date).
fn transition(d: NaiveDate, salt: u64, pool: &[&'static str]) -> String {
    pick(pool, d, salt).to_string()
}

/// A closing well-wish for the day (the place for "have a lovely Wednesday").
fn closing_wish(d: NaiveDate, name: &str) -> String {
    let wd = weekday_name(d);
    let n = name_suffix(name);
    match idx(d, 19, 5) {
        0 => format!("Have a lovely {wd}{n}."),
        1 => format!("Enjoy your {wd}{n}, and be kind to yourself today."),
        2 => format!("I hope your {wd} is a good one{n}."),
        3 => format!("Have a good one{n}, and enjoy the day."),
        _ => format!("Make it a lovely {wd}{n}."),
    }
}

/// The briefing body: no greeting and no weather (both are spoken first).
fn morning_template(c: &Ctx) -> String {
    let d = c.date;
    let mut v: Vec<Piece> = vec![];
    // the second spoken section opens with a small transition
    must(&mut v, transition(d, 61, &["Right, so… let's get into your day.", "Okay, so, um, let's look at your day.", "Hmm, right, let's see what today holds.", "Alright… let's get on to your day.", "Right, so, let's have a look at the day."]));
    if let Some(f) = digest::frame_sentence(&c.info) {
        must(&mut v, f);
    }

    // 2. yesterday
    let mut yd: Vec<String> = vec![];
    for o in c.yesterday.iter().filter(|o| o.done && !o.recurring) {
        let p = taskvoice::inline_phrase(o);
        if !yd.contains(&p) {
            yd.push(p);
        }
    }
    for o in c.yesterday.iter().filter(|o| o.done && o.recurring) {
        let p = taskvoice::inline_phrase(o);
        if !yd.contains(&p) {
            yd.push(p);
        }
    }
    let rolled: Vec<String> = c.items.iter().filter(|o| o.rolled_over_from.is_some() && !o.done).map(|o| taskvoice::inline_phrase(o)).collect();
    let mut back = if yd.is_empty() {
        pick(&["Yesterday was a quiet one on the list, and that's perfectly fine.", "Looking back, yesterday didn't leave much of a trail on the list, and there's no judgement here at all."], d, 51).to_string()
    } else {
        let lead = pick(&["Looking back at yesterday, you got through", "Quick look back: yesterday you worked through", "Yesterday you got through"], d, 52);
        format!("{lead} {}, which is lovely.", join(&yd))
    };
    if !rolled.is_empty() {
        back.push_str(&format!(" {} didn't quite happen, so {} moved across to today, with no pressure at all.", cap(&join(&rolled)), if rolled.len() == 1 { "it's" } else { "they've" }));
    }
    must(&mut v, back);
    must(&mut v, care_line(Kind::Morning, c.info.work.is_some(), d));
    if !c.missed.is_empty() {
        let mut parts: Vec<String> = vec![];
        for o in c.missed.iter().take(2) {
            let wd = parse_date(&o.date).map(weekday_name).unwrap_or_else(|| "earlier".into());
            parts.push(format!("{} from {wd}", o.title));
        }
        let verb = if c.missed.len() == 1 { "is" } else { "are" };
        let what = if c.missed.len() == 1 { "a new time" } else { "new times" };
        must(&mut v, format!("The {} {verb} waiting for {what} in Tasks.", parts.join(" and the ")));
    }

    // 3. today by category
    let todo: Vec<&Occurrence> = c.items.iter().filter(|o| !o.done).collect();
    let already: Vec<String> = c.items.iter().filter(|o| o.done).map(|o| taskvoice::inline_phrase(o)).collect();
    let mut goal_said = false;
    if c.items.is_empty() {
        must(&mut v, "Your list is wide open today, which means you get to decide what kind of day this is, and that sounds lovely.".into());
    } else {
        let lk = Look { d, goals: &c.goals, work: c.info.work.is_some() };
        let nm = if d.num_days_from_ce() % 2 == 0 { name_suffix(&c.user) } else { String::new() };
        let regular: Vec<&Occurrence> = todo.iter().filter(|o| !is_routine(o)).cloned().collect();
        let mut routines: Vec<&Occurrence> = todo.iter().filter(|o| is_routine(o)).cloned().collect();
        routines.sort_by(|x, y| x.time.cmp(&y.time));
        must(&mut v, match d.num_days_from_ce() as usize % 5 {
            0 => format!("So, um, looking at what we've got on today{nm}, {}.", if regular.is_empty() { "it's mostly your usual rhythm".to_string() } else { format!("we've got {}", taskvoice::how_many(regular.len())) }),
            1 => format!("Anyway, here's how your day is shaping up{nm}."),
            2 => format!("So{nm}, here's what you've got on today."),
            3 => format!("Let's see… what have we got on today{nm}."),
            _ => format!("Let's have a wander through what we've got on today{nm}."),
        });
        if !regular.is_empty() {
            let (text, said) = taskvoice::walk_through(&regular, &lk, true);
            goal_said = said;
            must(&mut v, text);
        }
        if !routines.is_empty() {
            let list = routine_list(&routines);
            must(&mut v, cat_lead("Self-care", true, &join(&list), d, 0));
        }
        if !already.is_empty() {
            add(&mut v, format!("{} is already ticked off, so you've got a head start.", cap(&join(&already))), PK::Drop(2));
        }
    }
    if let Some(p) = &c.workout {
        must(&mut v, workout_sentence(p, d));
        // at most one goal mention in the whole summary
        if !goal_said && !p.rest {
            if let Some((g, _)) = goals::find_link(&c.goals, &[(p.name.clone(), format!("gym workout {}", p.name))]) {
                goal_said = true;
                add(&mut v, format!("That one feeds straight into {}.", goals::goal_ref(g)), PK::Drop(1));
            }
        }
    }
    if !goal_said {
        if let Some(dl) = near_deadline(&c.goals, d, 21) {
            add(&mut v, format!("Also, keep in mind that {dl}."), PK::Drop(3));
        }
    }

    brk(&mut v);

    // 4. suggested order
    let timed: Vec<&Occurrence> = todo.iter().filter(|o| o.time.is_some()).cloned().collect();
    let fixed: Vec<String> = timed.iter().filter(|o| !is_routine(o)).map(|o| taskvoice::inline_phrase(o)).collect();
    let timed_routines: Vec<&Occurrence> = timed.iter().filter(|o| is_routine(o)).cloned().collect();
    // on a work day, untimed Work tasks belong inside the work block, not in the free time around it
    let at_work: Vec<&Occurrence> = if c.info.work.is_some() { todo.iter().filter(|o| o.time.is_none() && cat_name(o) == "Work" && !is_routine(o)).cloned().collect() } else { vec![] };
    let loose: Vec<&Occurrence> = todo.iter().filter(|o| o.time.is_none() && cat_name(o) != "Exercise" && !is_routine(o) && !(c.info.work.is_some() && cat_name(o) == "Work")).cloned().collect();
    let big: Vec<&Occurrence> = loose.iter().filter(|o| is_big(o)).cloned().collect();
    let quick: Vec<&Occurrence> = loose.iter().filter(|o| !is_big(o)).cloned().collect();
    let titles = |v: &[&Occurrence], k: usize| -> Vec<String> { v.iter().take(k).map(|o| taskvoice::inline_phrase(o)).collect() };
    if !fixed.is_empty() {
        must(&mut v, if fixed.len() == 1 {
            format!("The one fixed point is {}, so everything else can bend around it.", fixed[0])
        } else {
            format!("The fixed points are {}, so everything else bends around those.", join(&fixed))
        });
    } else if let Some(r) = timed_routines.first() {
        must(&mut v, format!("Beyond your {}, nothing is pinned to the clock today, so the rest can flex around how you feel.", r.title.to_lowercase()));
    }
    if let (Some(w), false) = (&c.info.work, at_work.is_empty()) {
        let list = limited(at_work.iter().map(|o| taskvoice::inline_phrase(o)).collect(), 4);
        must(&mut v, format!(
            "{} {list}.",
            pick(
                &[
                    "Once you're at work, the work things slot into your day there, so between {s} and {e} you've got",
                    "Your work list sits inside the workday itself, so from {s} to {e} that's",
                ],
                d,
                63
            )
            .replace("{s}", &say_time(&w.start))
            .replace("{e}", &say_time(&w.end))
        ));
    }
    let lunch_pick = c.info.work.as_ref().and_then(|_| quick.iter().find(|o| lunchable(o)).map(|o| taskvoice::inline_phrase(o)));
    match (big.first(), quick.len()) {
        (Some(b), q) if q >= 3 => must(&mut v, format!(
            "{}, I'd clear the quick ones first, {}, to build some momentum, and then save your proper attention for {} while the afternoon is still on your side.",
            pick(&["Anyway, for the rest", "Honestly, for the rest", "Anyway… for the rest"], d, 62),
            limited(titles(&quick, 3), 3),
            taskvoice::inline_phrase(b)
        )),
        (Some(b), _) => must(&mut v, format!(
            "Anyway, for the rest, I'd start with {} while you're fresh, because it's the one that could drag, and let the small things{} fill the gaps afterwards.",
            taskvoice::inline_phrase(b),
            if quick.is_empty() { String::new() } else { format!(", like {}", limited(titles(&quick, 2), 2)) }
        )),
        (None, 1) => must(&mut v, format!("The only loose thing is {}, and it's a quick one, so slot it in wherever it suits.", titles(&quick, 1)[0])),
        (None, q) if q > 0 => must(&mut v, format!(
            "Nothing loose is heavy, so run through {} in whatever order feels good, quickest first for an easy win.",
            limited(titles(&quick, 3), 3)
        )),
        _ => {
            if todo.iter().all(|o| o.time.is_none() || is_routine(o)) && loose.is_empty() && todo.is_empty() {
                must(&mut v, "There's nothing loose to put in order today, so the day can just unfold.".into());
            }
        }
    }
    let evening_routines: Vec<String> = todo.iter().filter(|o| is_routine(o) && o.time.is_none() && o.part_of_day == "evening").map(|o| taskvoice::inline_phrase(o)).collect();
    if !evening_routines.is_empty() {
        let verb = if evening_routines.len() > 1 { "are" } else { "is" };
        must(&mut v, format!("To close the day, {} {verb} a calm, low-effort way to finish.", join(&evening_routines)));
    }
    if let Some(w) = &c.info.work {
        let r = &c.memory.rhythm;
        let mut s = format!(
            "Other than that, with work from {} to {}, the window between waking up around {} and starting is good for the first small thing",
            say_time(&w.start),
            say_time(&w.end),
            say_time(&r.wake_to)
        );
        s.push_str(&match &lunch_pick {
            Some(t) => format!(", and if you get a moment in the middle of the day, {t} would fit in nicely."),
            None => ", and if you get a moment in the middle of the day, that's a good time for something short.".to_string(),
        });
        if !big.is_empty() {
            s.push_str(" Anything heavier is best before work or once you've finished, rather than squeezed in between.");
        }
        must(&mut v, s);
        add(&mut v, format!("Try to wrap up the evening comfortably before bedtime around {}.", say_time(&c.memory.rhythm.sleep_from)), PK::Drop(4));
    } else if c.info.holiday.is_some() || c.info.day_off {
        must(&mut v, "With no work to fit around, take the order at your own pace and keep a long, lazy stretch free.".into());
    } else {
        must(&mut v, "With no work block today, you can put the loose things wherever they fit best and keep a decent break in the middle.".into());
    }

    brk(&mut v);

    // 5. tomorrow (the third spoken section starts here)
    if let Some((twd, titems, tw)) = &c.tomorrow {
        let mut notable: Vec<&Occurrence> = titems.iter().filter(|o| cat_name(o) == "Appointments" || (o.time.is_some() && !is_routine(o))).collect();
        if notable.is_empty() {
            notable = titems.iter().filter(|o| !o.recurring).take(2).collect();
        }
        let mut s = if notable.is_empty() {
            format!("{} {twd}, it's looking nice and open so far.", pick(&["Oh, and a little peek at tomorrow,", "Hmm, the other thing is tomorrow,", "Oh, and… a quick peek at tomorrow,"], d, 63))
        } else {
            let ph: Vec<String> = notable.iter().map(|o| item_phrase(o)).collect();
            format!("{} {twd}, you've got {}.", pick(&["Oh, and a little peek at tomorrow,", "Hmm, the other thing is tomorrow,", "Oh, and… a quick peek at tomorrow,"], d, 63), join(&ph))
        };
        if let Some(h) = &c.tomorrow_info.holiday {
            s.push_str(&format!(" It's also a public holiday, {h}, so there's no work."));
        } else if c.tomorrow_info.day_off {
            s.push_str(" It's also a day off, so there's no work.");
        } else if let Some(w) = &c.tomorrow_info.work {
            s.push_str(&format!(" You start work at {} again.", say_time(&w.start)));
        }
        if let Some(p) = tw.as_ref().filter(|p| !p.rest) {
            s.push_str(&format!(" The gym has {} lined up.", p.name));
        }
        must(&mut v, s);
    }

    // extras: real content used only when the morning would otherwise run short
    if let Some(o) = todo.iter().find(|o| !o.checklist.is_empty()) {
        let steps: Vec<String> = o.checklist.iter().filter(|s| !s.done).map(|s| taskvoice::step_phrase(&s.text)).collect();
        if !steps.is_empty() {
            add(&mut v, format!("Your {} is really just {}, which is easy to tick off bit by bit.", o.title.to_lowercase(), join(&steps)), PK::Extra(1));
        }
    }
    if !c.items.is_empty() {
        add(&mut v, format!("Put together, {}", lower_first(&digest::overview_template(&c.items, &[], &None, &c.info, d))), PK::Extra(2));
    }
    if let Some(o) = todo.iter().find(|o| !o.notes.trim().is_empty() && words_in(&o.notes) <= 25) {
        add(&mut v, format!("You left yourself a note on {}, and it says {}", o.title, o.notes.trim()), PK::Extra(3));
    }
    let mut further: Vec<String> = vec![];
    for (wd, items) in c.ahead.iter().take(2) {
        let notable: Vec<String> = items.iter().filter(|o| !o.recurring || o.time.is_some() && !is_routine(o)).filter(|o| !is_routine(o)).take(2).map(item_phrase).collect();
        if !notable.is_empty() {
            further.push(format!("{wd} has {}", join(&notable)));
        }
    }
    if !further.is_empty() {
        add(&mut v, format!("Further ahead, {}.", join(&further)), PK::Extra(4));
    }
    if let Some(p) = c.workout.as_ref().filter(|p| !p.rest) {
        if let Some(e) = p.exercises.first() {
            add(&mut v, format!("{} opens with {}, which is three or so working sets, so warm up properly before the heavy ones.", p.name, e.name), PK::Extra(5));
        }
    }
    if c.items.iter().filter(|o| o.rolled_over_from.is_some() && !o.done).count() > 0 {
        add(&mut v, "If something keeps rolling over day after day, that's a sign to either shrink it or give it a time, so let me know if you want to reshuffle.".into(), PK::Extra(6));
    }

    // gentle, optional leisure idea (only with genuinely free room)
    if let Some(l) = &c.leisure_pick {
        must(&mut v, leisure::morning_sentence(l, d, c.leisure_open_day));
    }

    brk(&mut v);

    // 6. grounded close
    must(&mut v, match daynote::key_task(&c.items) {
        Some(k) if !k.done => {
            let kp = taskvoice::inline_phrase(k);
            match d.num_days_from_ce() as usize % 3 {
                0 => format!("If you only get one thing done today, let it be {kp}."),
                1 => format!("Start with {kp} and the rest of the day gets easier to read."),
                _ => format!("The one to watch today is {kp}."),
            }
        }
        Some(_) => "Everything on the list is already done, so the rest of the day is yours.".into(),
        None => "Nothing is waiting on you, so the day is yours to shape.".into(),
    });
    must(&mut v, pick(&["I'll be right here if the plan changes.", "Shout if you need me.", "Go get started, and I'll be here when you need me."], d, 17).into());
    must(&mut v, closing_wish(d, &c.user));
    assemble(v, 340, 640)
}

/// A small, optional everyday-care line (a complete spoken sentence), varied by date and fitted to the time of day and
/// whether it is a work day. Never the same line two days running.
fn care_line(kind: Kind, work_day: bool, d: NaiveDate) -> String {
    let pool: Vec<&str> = match (kind, work_day) {
        (Kind::Morning, true) => vec![
            "Maybe put the kettle on and have a cup of tea before the day gets going.",
            "A proper breakfast is worth it this morning, even if it's just some Weet-Bix or a bit of toast.",
            "If you can, pop outside for a few minutes before work, just to wake up properly.",
            "Have a glass of water first thing, too, it makes a surprising difference.",
            "There's time for a cup of tea before work, so I'd take it.",
        ],
        (Kind::Morning, false) => vec![
            "There's no rush this morning, so a slow cup of tea and some toast sounds just right.",
            "Maybe put the kettle on and have a proper breakfast, like Weet-Bix or a bit of toast, before you do anything else.",
            "A few minutes outside in the fresh air would be a nice way to start the day.",
            "Have a glass of water first, and then let the morning unfold however it likes.",
        ],
        (_, true) => vec![
            "After a day of sitting, a little stretch would probably feel lovely.",
            "A cup of tea might be nice as you wind down.",
            "An early night wouldn't hurt, either.",
            "A glass of water before bed, and a few slow stretches, and you're all set.",
        ],
        (_, false) => vec![
            "A cup of tea would go nicely with a quiet evening like this.",
            "If you've been sitting for a while, a little stretch before bed would feel good.",
            "An early night sounds like a lovely idea.",
            "Maybe step outside for a minute for some fresh air before you settle in.",
        ],
    };
    // different on consecutive days: step through the pool by the day number
    let n = pool.len();
    pool[(d.num_days_from_ce() as usize).wrapping_add(if kind == Kind::Morning { 0 } else { 2 }) % n].to_string()
}

/// Today's pick of care touches for the model (varied by date).
fn care_ideas(c: &Ctx) -> String {
    let w = c.info.work.is_some();
    let a = care_line(c.kind, w, c.date);
    let b = care_line(c.kind, w, c.date + Duration::days(1));
    let c2 = care_line(c.kind, w, c.date + Duration::days(2));
    format!("{a} / {b} / {c2}")
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn template_text(c: &Ctx, weather: &Option<Weather>) -> String {
    let d = c.date;
    let n = name_suffix(&c.user);
    let a = &c.assistant;
    let mut parts: Vec<String> = vec![];
    match c.kind {
        Kind::Morning => {
            let mp = morning_parts(c, weather);
            parts.push(match &c.news {
                Some(p) => mp.joined_with_news(Some((&news::template_text(&p.items, c.kind, c.date, p.position), p.position))),
                None => mp.joined(),
            })
        }
        Kind::Evening => {
            parts.push(match d.num_days_from_ce() as usize % 3 {
                0 => format!("Hey{n}, it's {a}… let's wind the day down together."),
                1 => format!("Good evening{n}! It's {a} here, and you made it through another day, so, um, let's take a quiet look back."),
                _ => format!("Hi there{n}! It's {a}. Kick your shoes off, it's time to, you know, unwind."),
            });
            let total = c.items.len();
            let done = count_done(&c.items);
            let done_titles: Vec<String> = c.items.iter().filter(|o| o.done).map(|o| taskvoice::inline_phrase(o)).collect();
            parts.push(if total == 0 {
                "There was nothing on your list today, so there's no pressure at all.".into()
            } else if done == total {
                format!("You wrapped up everything on your list today, {} included. I'm honestly so proud of you.", join(&done_titles))
            } else if done == 0 {
                match d.num_days_from_ce() as usize % 2 {
                    0 => "The task list didn't quite happen today, and honestly, that's okay. Some days are just like that.".into(),
                    _ => "Today was more of a slow one on the task front, and that's completely fine. Tomorrow is a fresh start.".into(),
                }
            } else {
                match d.num_days_from_ce() as usize % 2 {
                    0 => format!("You got through {} today. That's real progress.", join(&done_titles)),
                    _ => format!("You did nice work today, with {} now off your plate.", join(&done_titles)),
                }
            });
            let left: Vec<String> = c.items.iter().filter(|o| !o.done && !is_routine(o)).map(taskvoice::inline_phrase).collect();
            if !left.is_empty() {
                parts.push(format!("{} {} still waiting, but any one-off tasks will roll over to tomorrow on their own, so there's nothing to stress about.", cap(&join(&left)), if left.len() == 1 { "is" } else { "are" }));
            }
            let mut link_texts: Vec<(String, String)> = Vec::new();
            if let Some(g) = &c.gym_done {
                if let Some(cp) = &c.cardio_done {
                    parts.push(format!("And you fitted in {cp} today, which is fantastic."));
                } else {
                    parts.push(format!("And you got your {g} session in, which is fantastic."));
                }
                link_texts.push((format!("that {g} session"), format!("gym session {g}")));
            } else if let Some(p) = &c.workout {
                if p.rest {
                    parts.push("Your rest day did its job too.".into());
                } else {
                    parts.push(format!("The gym didn't happen today, and that's fine. {} will be waiting for you.", p.name));
                }
            }
            link_texts.extend(c.items.iter().filter(|o| o.done).map(|o| (o.title.clone(), o.title.clone())));
            let mut goal_said = false;
            if let Some((g, what)) = goals::find_link(&c.goals, &link_texts) {
                goal_said = true;
                let what = c.items.iter().find(|o| o.title == what).map(|o| taskvoice::inline_phrase(o)).unwrap_or(what);
                let gr = goals::goal_ref(g);
                parts.push(match d.num_days_from_ce() as usize % 3 {
                    0 => format!("{} counts toward {gr}, by the way.", cap(&what)),
                    1 => format!("Quietly, {what} moved {gr} forward."),
                    _ => format!("That's one more step toward {gr}, thanks to {what}."),
                });
            }
            if let Some(w) = weather {
                parts.push(crate::weather::natural_sentence(&w.summary, true));
            }
            parts.push(care_line(Kind::Evening, c.tomorrow_info.work.is_some() || c.info.work.is_some(), d));
            parts.push("\n\n".into());
            let name_in_intro = d.num_days_from_ce() % 2 == 0;
            if let Some((wd, items, w)) = &c.tomorrow {
                let lk = Look { d, goals: if goal_said { &[] } else { &c.goals }, work: c.tomorrow_info.work.is_some() };
                let regular: Vec<&Occurrence> = items.iter().filter(|o| !is_routine(o)).collect();
                let mut routines: Vec<&Occurrence> = items.iter().filter(|o| is_routine(o)).collect();
                routines.sort_by(|x, y| x.time.cmp(&y.time));
                let nm = if name_in_intro { n.clone() } else { String::new() };
                let mut s = if items.is_empty() {
                    format!("Tomorrow is {wd}, and it's looking nice and open.")
                } else if regular.is_empty() {
                    format!("So looking at what we've got on tomorrow{nm}, {wd}, it's mostly your usual rhythm.")
                } else {
                    {
                        let (text, said) = taskvoice::walk_through(&regular, &lk, !goal_said);
                        goal_said |= said;
                        format!("So looking at what we've got on tomorrow{nm}, {wd}, we've got {}. {}", taskvoice::how_many(regular.len()), text)
                    }
                };
                if !routines.is_empty() {
                    let list = routine_list(&routines);
                    s.push_str(&format!(" {}", cat_lead("Self-care", true, &join(&list), d, 0)));
                }
                if let Some(p) = w {
                    s.push(' ');
                    s.push_str(&gym_preview(p));
                }
                parts.push(s);
            }
            if let Some(h) = &c.tomorrow_info.holiday {
                parts.push(format!("Tomorrow is a public holiday, {h}, so no work, which is something to look forward to."));
            } else if c.tomorrow_info.day_off {
                parts.push("Tomorrow is a day off, so there's no work to set an alarm for.".into());
            }
            if !goal_said {
                if let Some(dl) = near_deadline(&c.goals, d, 14) {
                    parts.push(format!("One thing on the horizon is that {dl}."));
                }
            }
            if let Some(l) = &c.leisure_pick {
                parts.push(leisure::evening_sentence(l, d));
            }
            let first = c.tomorrow.as_ref().and_then(|(wd, items, _)| daynote::key_task(items).filter(|k| !is_routine(k)).map(|k| (wd.clone(), item_phrase(k))));
            let sign = taskvoice::signoff(d, &c.user, !name_in_intro && !c.user.is_empty());
            parts.push(match (&c.tomorrow, first) {
                (Some((wd, _, _)), Some((_, k))) if d.num_days_from_ce() % 3 == 0 => format!("{k} is the first thing to think about for {wd}. {sign}"),
                (Some((wd, items, _)), None) if items.is_empty() => format!("{wd} has nothing scheduled. {sign}"),
                _ => sign,
            });
            parts.push("\n\n".into());
            if let Some(p) = &c.news {
                parts.push(news::template_text(&p.items, c.kind, d, p.position));
                parts.push("\n\n".into());
            }
            parts.push(format!(
                "{} {} {}",
                pick(&["Oh, and…", "Hmm, one last thing…", "Right, so, um, I'll leave you with this."], d, 28),
                pick(&["Start winding down soon so you're asleep in good time.", "Dim the lights and ease off the screens soon, so sleep comes easily.", "Maybe get ready for bed a little early, so you're properly rested."], d, 29),
                pick(&["Sleep well.", "Rest up, I'll see you tomorrow.", "Goodnight."], d, 23)
            ));
        }
        Kind::Weekly => {
            let w = c.week.as_ref().unwrap();
            parts.push(match d.num_days_from_ce() as usize % 3 {
                0 => format!("Hey{n}, it's {a}! It's time for our weekly catch-up, so settle in."),
                1 => format!("Hi there{n}, it's {a} here. Let's take a moment to look back at your week."),
                _ => format!("Hi again{n}! It's {a}, and I've got your week in review."),
            });
            let quiet = w.done_titles.is_empty() && w.sessions.is_empty() && w.cardio.is_empty();
            // goal connection: first goal that relates to something done or trained
            let mut texts: Vec<(String, String)> = Vec::new();
            for (nm, _, _, _) in w.prs.iter() {
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
                    let take: Vec<String> = titles.iter().take(if w.sessions.is_empty() && w.cardio.is_empty() { 3 } else { 2 }).cloned().collect();
                    hl.push(format!("got {} done", join(&take)));
                }
                if !w.cardio.is_empty() {
                    hl.push(format!("fitted in cardio too, with {}", join(&w.cardio)));
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
                if let Some((nm, now, was, assisted)) = w.prs.first() {
                    if *assisted {
                        hl.push(format!("brought the counterweight on {nm} down from {} to {} kilos", gym::fmt_w(*was), gym::fmt_w(*now)));
                    } else {
                        hl.push(format!("moved {nm} up from {} to {} kilos", gym::fmt_w(*was), gym::fmt_w(*now)));
                    }
                }
                hl.truncate(3);
                let lead = pick(&["What stood out to me is that you", "Looking back, you", "One thing I loved is that you"], d, 41);
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
                parts.push(format!("And you actually hit a goal this week, which was {}. That's worth a proper moment of celebration.", goals::short_title(g)));
            }
            if let Some(l) = c.leisure_finished.first() {
                parts.push(format!("On the lighter side, you also finished {} this week, which is a lovely thing to have done just for fun.", l.title));
            }
            if !w.slid_titles.is_empty() {
                let v: Vec<String> = w.slid_titles.iter().take(2).cloned().collect();
                parts.push(format!("{} slid a little, and that's absolutely fine. Those can simply carry into next week, no guilt.", join(&v)));
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
                parts.push(format!("It's worth a thought that {dl}, so keep it gently on your radar."));
            } else if linked.is_none() {
                if let Some(g) = c.goals.first() {
                    parts.push(format!("And I'm keeping {} in the back of my mind for you.", goals::goal_ref(g)));
                }
            }
            parts.push(match w.next_items.first() {
                Some((wd, t)) if !quiet => format!("{} The first thing to watch for is {t} on {wd}.", pick(&["Enjoy what's left of your weekend.", "Rest up before the week begins."], d, 31)),
                _ => pick(&["Enjoy the rest of your weekend. I'm right here whenever you need me.", "Take it easy, and I'll see you at the start of the week."], d, 31).into(),
            });
        }
    }
    parts.join(" ").replace(" \n\n ", "\n\n")
}

// ------------------------------------------------------------------ prompts

const TONE: &str = "TONE: you are a warm, chatty best friend who is also a really good personal assistant and genuinely knows this person. Sound natural and human, with contractions and a bit of personality, never like a report. \
KNOWING THEM AND YOURSELF: you have looked after this person for a long time. Speak with the easy, confident familiarity of someone who knows their routines, their week, their background, their goals and their tastes, and act on that knowledge quietly in what you suggest and how you put it, without reciting it back to them. You also know yourself: your own habits, little stories and opinions (the ones given to you) are simply yours, so tell them naturally and never hedge about being an assistant, an AI or a program, never say you can't remember or don't know them, and never ask them to tell you about themselves. \
Be relaxed and conversational, like a friend chatting over breakfast: gently meandering is fine ('Anyway,', 'Other than that,', 'Oh, and'), it does not have to be a tight checklist, and it should always feel warm and unhurried. \
CARE TOUCHES: where they fit the time of day and the plans, weave in small, optional everyday care touches naturally, such as putting the kettle on for a cup of tea, a proper breakfast (Weet-Bix or toast), a glass of water, a few minutes outside, a stretch after sitting for a while, or an early night. At most two or three per summary, different ones on different days, never bossy, never a list, never the same ones every day, and only the ones that genuinely suit this day (a stretch or an early night belong to the evening; breakfast and tea to the morning). \
The listener is well-read and knows history and politics well: never over-explain or define basics or give potted histories; be straightforward and substantive, with a sharp bit of context only when it adds something. \
NEVER read out raw counts, fractions, percentages or tallies: no '0 of 24 tasks', no 'six tasks', no '50 percent', no lists of numbers. Small natural quantities like 'a couple of things' or 'a few' are fine, but prefer naming the specific things (task titles, workout names, exercises) that were done or are coming up. \
Be kind and low-key about quiet days or weeks: no guilt, no stats, no pressure. \
GOALS: when active goals are given, connect specific completed or planned tasks and gym sessions to a goal, but ONLY where they plausibly relate (never force a link, and it is fine to mention no goal at all). Mention an approaching goal deadline naturally when it is relevant and fairly close, saying it like a person would ('that's a few weeks away now'), not as a date dump.";

fn system_prompt(c: &Ctx) -> String {
    let words = match c.kind {
        Kind::Weekly => "between 200 and 300 words",
        Kind::Morning if c.news.is_some() => "between 420 and 620 words for the briefing part alone, long enough to give every task its sentence or two (the four parts together, with the separate news field, come to roughly 750 to 950 words, a 5 to 6 minute read-aloud; do not stop short)",
        Kind::Morning => "between 420 and 620 words for the briefing part alone, long enough to give every task its sentence or two (the four parts together are a 4 to 5 minute read-aloud; do not stop short)",
        Kind::Evening if c.news.is_some() => "at least 350 words for the wind-down part alone (a soft floor, it can run longer if there is more to say, up to about 650; with the separate news field the whole thing comes to at least 550 words, a 4 to 5 minute read-aloud)",
        Kind::Evening => "between 350 and 650 words, a soft floor rather than a cap",
    };
    let focus = match c.kind {
        Kind::Morning => "This is the MORNING briefing, which you write as FOUR separate spoken parts that are read aloud in this order: greeting, weather_comment, affirmation, briefing. Together they run several relaxed minutes.\n\
PART greeting: one short, natural spoken hello, using the person's name if given, for example 'Morning, Sam!', 'Good morning, Sam. Hope you slept well.' or 'Morning! Grab a cuppa and let's look at today.' It must be a hello, NEVER a well-wish: do not open with 'I hope you have a lovely <day>' or anything like it. Well-wishes such as 'have a lovely Wednesday' belong only in the very last line of the briefing. One or two short sentences at most, no weather, no plans.\n\
PART weather_comment: 1 to 2 sentences interpreting the forecast, with tone and advice quietly shaped by the person's weather preferences (happy about cool days, a gentle heads-up and practical tips when hot, sun cream when UV is high and they avoid sunburn, cosy about cool rain if they like it) but NEVER stating or explaining those preferences, and tied to the day's plans rather than just read out. Temperatures as degrees, never a rain percentage. null if no weather is given.\n\
PART affirmation: 1 or 2 sentences, at most 40 words, grounded and SPECIFIC to today's actual plan: name a real task, appointment or workout, or the shape of the day (where the busy stretch is, where there is slack or a breather). Practical and encouraging, not soppy. Never state or narrate the person's preferences. If a goal plausibly relates to a task today you may tie them together in a few words, never forced. Never use clichés or generic affirmations like 'you\'ve got this', 'believe in yourself', 'one step at a time' or 'you are enough'. No raw counts.\n\
PART briefing: the rest of the morning summary, WITHOUT any greeting and WITHOUT any weather (those are spoken first, so do not repeat them), flowing naturally with no headings: if the context says 'On this day: yes', start the briefing with ONE short spoken sentence about a well-known historical anniversary of today's calendar date that connects to the person's interests (use the interests text quietly), but ONLY if you are certain of both the date and the fact; if you are not completely certain, or the context says 'On this day: no', omit it entirely and never mention that you are omitting it. Write the briefing as FOUR short paragraphs separated by a blank line: first the look back and today broken down by theme, then the suggested order for the day, then tomorrow, then a one or two sentence closing line that ends with a varied, warm well-wish for the day (for example 'have a lovely Wednesday'). The whole morning is read aloud as THREE sections (1: greeting, weather, affirmation; 2: the first two paragraphs, the day itself; 3: tomorrow, the news if any, and the closing), each recorded separately, so the FIRST paragraph of the briefing must open with a small spoken transition ('Right, so…', 'Okay, so, um, your day.') and the THIRD paragraph (tomorrow) with another ('Oh, and…', 'Anyway…'). Then mention a public holiday or day off warmly if it is one (no work today); (1) a quick, kind look back at yesterday: name what got done and what rolled over, no counts, no guilt; (2) today walked through task by task using the TASK TALK rules below (every undone task by name, with the category as a natural lead-in such as 'on the life admin front', work, chores, errands, study, exercise and the gym, social plans, appointments with their times); routines and planning habits count as looking after yourself, not as work, and can be named together in one breath; (3) a suggested order for the day: decide whether to do the quick small tasks first to build momentum or the big one first while fresh, say which and why (use the size hints), respect fixed-time items and the work block (work hours are fixed and not a task; use the window between waking and the start of work for early things, the flexible lunch break only for short things that fit about an hour and plausibly sit near work like quick calls, bills, booking appointments or a quick shop; the lunch window is private background, so suggest such a thing only softly and optionally, in words like 'if you get a moment in the middle of the day, X would fit nicely', never stating the lunch times and never as an order like 'do X at lunch', and keep the evening finishing comfortably before bedtime); (4) a peek at tomorrow's notable things and appointments; (5) a grounded, specific closing line naming the suggested key task (a meaningful appointment or meeting, never a routine). Do not repeat the affirmation. Never use generic affirmations or cliches like 'you\'ve got this'.",
        Kind::Evening => "This is the EVENING wind-down, a relaxed, unhurried chat that winds the person down. Write it as three paragraphs separated by a blank line. First, a relaxed look back at what happened today: kindly say what got done by naming it (celebrate it, never guilt), mention what is left and that unfinished one-off tasks roll over to tomorrow, mention whether the gym happened, link what was done to a goal where it plausibly relates. Second, a preview of tomorrow, opening with something like 'So looking at what we've got on tomorrow', that walks through EVERY one of tomorrow's tasks using the TASK TALK rules below (what it involves, why it is worthwhile, when and how long, a touch of commentary), plus the gym with what it trains, and whether it is a work day, a public holiday or a day off. Routines can be named together in one breath. Recap today's weather conversationally in the past tense, in a sentence or two, if weather is given. The evening is read aloud as three separately recorded sections (the look back, the tomorrow preview, then the news if any plus the closing), so the second paragraph must open with a small spoken transition such as 'Right, so…' and the closing paragraph with 'Anyway…' or 'Oh, and…'. Third, a short closing paragraph: gently nudge toward winding down in time for sleep, then a cosy sign-off (see SIGN-OFF below) and a calm goodnight. If a news segment is requested below it is spoken between the second and third paragraph, so do not mention the news in your paragraphs. No generic affirmations or cliches.",
        Kind::Weekly => "This is the WEEKLY overview, read on Sunday. Open warmly. Pick two or three specific highlights that were actually done (task titles, gym sessions by name, and any exercise progressions) and connect them to the goals where they plausibly relate. Gently mention anything that slid, without guilt. Look ahead to the notable things next week and to any goal deadline coming up. Close with one short grounded line. If nothing much was done, be kind and low-key, give no stats, and focus on the week ahead and the goals. Do not list every task and never quote totals. No generic affirmations or cliches.",
    };
    format!(
        "You are {a}, the user's personal assistant and friend. You are writing a script that will be read aloud by a text-to-speech voice, so write exactly how a chatty, caring friend would talk.\n\
         {spoken}\nRules: plain spoken sentences only. No markdown, no asterisks, no headings, no bullet points, no numbered lists, no emojis, no stage directions, no quotation marks around the whole text. \
         Say times naturally, like 'at seven thirty in the morning' or 'at 3 PM'. Say 'kilos' instead of 'kg'. Weather temperatures are in Celsius and should be said as degrees. \
         Only use facts from the context; never invent tasks, numbers or weather. Keep it {words}. Output only the script.\n{TONE}\n{kb}\n{focus}{rules}{news}",
        kb = knowledge::RULES,
        a = c.assistant,
        spoken = concat!(spoken_rule!(), style_guide!()),
        rules = match c.kind {
            Kind::Morning => format!("\n{}", taskvoice::PROMPT_RULES),
            Kind::Evening => format!("\n{}{}", taskvoice::PROMPT_RULES, taskvoice::EVENING_SIGNOFF_RULE),
            Kind::Weekly => String::new(),
        },
        news = c.news.as_ref().map(|p| format!("\n{}", news::instructions(c.kind, p.position, p.items.len()))).unwrap_or_default()
    )
}

fn context_text(c: &Ctx, weather: &Option<Weather>) -> String {
    let mut s = String::new();
    s.push_str(&format!("Briefing type: {}\n", c.kind.name()));
    s.push_str(&format!("Date: {}, {}\n", weekday_name(c.date), c.date.format("%B %-d, %Y")));
    s.push_str(&format!("User's name: {}\n", if c.user.is_empty() { "(not set, don't use a name)" } else { &c.user }));
    s.push_str(&format!("Your name: {}\n", c.assistant));
    match weather {
        Some(_) => s.push_str(&digest::weather_line(weather, "never quote a percentage")),
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
            let routine = if is_routine(i) { "; a routine/planning habit" } else { "" };
            let notes = match i.notes.split_whitespace().take(40).collect::<Vec<_>>().join(" ") {
                n if n.is_empty() => String::new(),
                n => format!("; notes: {n}"),
            };
            let cat = format!("; category: {}", group_name(i));
            let size = match crate::durations::typical(i) {
                Some(t) => format!("; {}", crate::durations::context_hint(&t)),
                None if is_big(i) => "; looks bigger".to_string(),
                None => "; looks quick".to_string(),
            };
            o.push_str(&format!("  - {} ({when}; done: {}{cat}{routine}{size}{notes}{steps}{roll})\n", i.title, if i.done { "yes" } else { "no" }));
        }
        o
    };
    let plan = |p: &WorkoutPlan| -> String {
        if p.rest {
            return "rest day".into();
        }
        if p.is_cardio() {
            return format!("{}: cardio (trains heart, lungs and endurance), choose one of {} ({}-{} minutes)", p.name, p.options.join(", "), p.minutes_min.unwrap_or(20), p.minutes_max.unwrap_or(40));
        }
        let ex: Vec<String> = p
            .exercises
            .iter()
            .map(|e| match &e.hint {
                Some(h) => format!("{} {}x{}-{} ({})", e.name, e.sets, e.rep_min, e.rep_max, h),
                None => format!("{} {}x{}-{}", e.name, e.sets, e.rep_min, e.rep_max),
            })
            .collect();
        match gym::trains(p) {
            Some(t) => format!("{} (trains: {t}): {}", p.name, ex.join("; ")),
            None => format!("{}: {}", p.name, ex.join("; ")),
        }
    };
    s.push_str(&digest::context_line(&c.info, "Today's"));
    if c.tomorrow.is_some() {
        s.push_str(&digest::context_line(&c.tomorrow_info, "Tomorrow's"));
    }
    s.push_str(&memory::context_block(&c.memory));
    if c.kind == Kind::Morning {
        s.push_str(&format!("On this day: {}\n", if c.on_this_day { "yes" } else { "no" }));
    }
    if c.kind == Kind::Morning {
        s.push_str(&format!("Yesterday's items (done flags; mention what got done by name, kindly):\n{}", list(&c.yesterday)));
    }
    if !c.missed.is_empty() {
        let m: Vec<String> = c.missed.iter().take(3).map(|o| format!("{} (from {}{})", o.title, parse_date(&o.date).map(weekday_name).unwrap_or_default(), o.time.as_deref().map(|t| format!(" at {}", say_time(t))).unwrap_or_default())).collect();
        s.push_str(&format!("Missed and still waiting for a new time in the Tasks tab: {}. Mention this once, in one short gentle sentence such as 'The dentist slot from Monday is waiting for a new time in Tasks.' No counts, no guilt.\n", m.join("; ")));
    }
    match c.kind {
        Kind::Morning | Kind::Evening => {
            s.push_str(&format!("Today's items in order (each flagged done or not; do not quote totals):\n{}", list(&c.items)));
            if let Some(k) = daynote::key_task(&c.items).filter(|k| !k.done) {
                s.push_str(&format!("Suggested key task to name as the one to watch: {}\n", item_phrase(k)));
            }
            match (&c.workout, &c.gym_done) {
                (_, Some(g)) => match &c.cardio_done {
                    Some(cp) => s.push_str(&format!("Workout: completed cardio today ({cp})\n")),
                    None => s.push_str(&format!("Workout: completed {g} today\n")),
                },
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
            s.push_str(&format!("Cardio completed this week (mention naturally, e.g. 'a swim'): {}\n", vl(&w.cardio)));
            for (n, now, was, assisted) in &w.prs {
                if *assisted {
                    s.push_str(&format!("Progression: {n} counterweight (lower is stronger) went down from {} to {} kilos\n", gym::fmt_w(*was), gym::fmt_w(*now)));
                } else {
                    s.push_str(&format!("Progression: {n} top weight went up from {} to {} kilos\n", gym::fmt_w(*was), gym::fmt_w(*now)));
                }
            }
            let next: Vec<String> = w.next_items.iter().map(|(wd, t)| format!("{t} on {wd}")).collect();
            s.push_str(&format!("Next week's notable things: {}\n", vl(&next)));
            s.push_str(&format!("Next week's workouts: {}\n", if w.next_workouts.is_empty() { "none planned".into() } else { w.next_workouts.join(", ") }));
        }
    }
    s.push_str(&goals::context_block(&c.goals, &c.goals_achieved, c.date));
    if c.kind != Kind::Weekly {
        s.push_str(&format!("Care touch ideas that suit today, pick at most two or three, or fewer, only if they fit naturally: {}\n", care_ideas(c)));
    }
    if let Some(p) = &c.news {
        s.push_str(&news::user_block(&p.items));
    }
    if c.kind != Kind::Weekly {
        s.push_str(&leisure::context_block(&c.leisure, &c.leisure_pick, c.leisure_room));
    } else {
        s.push_str(&leisure::finished_context(&c.leisure_finished));
    }
    s.push_str(&knowledge::context_block(&knowledge_picks(c, weather)));
    s
}

/// Today's few optional memories, facts, tips and thoughts for the model, fitted to the day.
fn knowledge_picks(c: &Ctx, weather: &Option<Weather>) -> Vec<&'static knowledge::Entry> {
    let mut cats: Vec<String> = c.items.iter().filter(|o| !o.done).map(cat_name).collect();
    if c.kind == Kind::Evening {
        if let Some((_, items, _)) = &c.tomorrow {
            cats.extend(items.iter().map(cat_name));
        }
    }
    if c.workout.as_ref().is_some_and(|p| !p.rest) || c.gym_done.is_some() {
        cats.push("Exercise".into());
    }
    let busy = c.items.iter().filter(|o| !o.done && !is_routine(o)).count() >= 4;
    let leisure = c.leisure_pick.as_ref().map(|l| format!("{} {}", l.title, l.note)).unwrap_or_default();
    let tags = knowledge::tags(
        c.kind.name(),
        weather.as_ref().map(|w| w.temp_max),
        weather.as_ref().map(|w| w.summary.as_str()).unwrap_or(""),
        weather.as_ref().and_then(|w| w.precip_prob).is_some_and(|p| p >= 50),
        &cats,
        c.info.work.is_some(),
        !c.goals.is_empty(),
        busy,
        &leisure,
    );
    knowledge::picks(knowledge::kinds_for(c.kind == Kind::Evening, c.kind == Kind::Weekly), &tags, c.date)
}

/// The text without anything inside quotation marks (headlines are quoted word for word and may hold numbers).
fn strip_quoted(t: &str) -> String {
    let chars: Vec<char> = t.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let close = match c {
            '"' => Some('"'),
            '“' => Some('”'),
            // a single quote opens a quotation only at a word start (not an apostrophe)
            '\'' | '‘' if i == 0 || !chars[i - 1].is_alphanumeric() => Some(if c == '‘' { '’' } else { '\'' }),
            _ => None,
        };
        if let Some(cl) = close {
            // the closing quote must not be an apostrophe inside a word
            let end = (i + 1..chars.len()).find(|&j| chars[j] == cl && (cl == '"' || cl == '”' || j + 1 == chars.len() || !chars[j + 1].is_alphanumeric()));
            if let Some(j) = end {
                out.push(' ');
                i = j + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// True when the text reads out a count, fraction or percentage ("3 of 5", "7 out of 10", "percent", "%").
/// Text inside quotation marks (verbatim headlines) is ignored.
pub fn has_stats(t: &str) -> bool {
    let t = &strip_quoted(t);
    if t.contains('%') || t.to_lowercase().contains("percent") {
        return true;
    }
    let w: Vec<String> = t.split_whitespace().map(|x| x.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase()).collect();
    let num = |x: &str| !x.is_empty() && x.chars().all(|c| c.is_ascii_digit());
    (0..w.len()).any(|i| {
        num(&w[i])
            && ((w.get(i + 1).is_some_and(|x| x == "of") && w.get(i + 2).is_some_and(|x| num(x)))
                || (w.get(i + 1).is_some_and(|x| x == "out") && w.get(i + 2).is_some_and(|x| x == "of") && w.get(i + 3).is_some_and(|x| num(x))))
    })
}

#[derive(Serialize)]
struct Stats {
    total: usize,
    done: usize,
    next_up: Option<Occurrence>,
}

/// The morning/evening/weekly template text for a date (exposed for tests).
pub fn template_for(env: &mut Env, kind: Kind, date: NaiveDate) -> String {
    let c = build_ctx(env, kind, date);
    let weather = if kind == Kind::Weekly { None } else { env.ctx.weather.get(&fmt_date(date)).cloned() };
    template_text(&c, &weather)
}

/// The prompt with every done state removed (done flags, the suggested key task, workout-completed lines),
/// so a cache key built from it only changes when the day's plan changes.
fn plan_only(user: &str) -> String {
    user.replace("; done: yes", "; done: no")
        .lines()
        .filter(|l| !l.starts_with("Suggested key task") && !l.starts_with("Workout"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A short fingerprint of what the text is about: the plan (titles, times, parts of the day, workout name),
/// plus done states for the evening; for the weekly overview what was done and the gym sessions.
/// Never includes weather. The UI regenerates a written text only when the person asks and this changed.
fn plan_fingerprint(ctx: &Ctx) -> String {
    let mut s = String::new();
    match &ctx.week {
        Some(w) => {
            s.push_str(&format!("done:{}|sessions:{}", w.done_titles.join(";"), w.sessions.join(";")));
        }
        None => {
            for o in &ctx.items {
                s.push_str(&format!("{}|{}|{}|{}", o.task_id, o.title, o.time.clone().unwrap_or_default(), o.part_of_day));
                if ctx.kind == Kind::Evening {
                    s.push_str(if o.done { "|y" } else { "|n" });
                }
                s.push('\n');
            }
            if let Some(wp) = &ctx.workout {
                s.push_str(&format!("workout:{}", if wp.rest { "rest" } else { wp.name.as_str() }));
            }
        }
    }
    fnv(&s)
}

/// `GET /api/briefing?kind=&date=`
pub fn briefing(env: &mut Env, kind: Kind, date: NaiveDate) -> Res<Value> {
    let mut ctx = build_ctx(env, kind, date);
    // the suggestion counts as "made" once today's summary is prepared: remember it for the 5-day rotation
    if kind != Kind::Weekly && date == env.today {
        if let Some(l) = &ctx.leisure_pick {
            leisure::stamp(env, l.id, date);
        }
    }
    let ds = fmt_date(date);
    let weather = if kind == Kind::Weekly { None } else { env.ctx.weather.get(&ds).cloned() };
    let system = system_prompt(&ctx);
    let user = context_text(&ctx, &weather);
    // The morning briefing is a plan for the day: ticking things off must not change its cache key.
    let hash = if kind == Kind::Morning { fnv(&format!("{system}\n{}", plan_only(&user))) } else { fnv(&format!("{system}\n{user}")) };
    let text = template_text(&ctx, &weather);
    // the same text without the news (the app inserts the news itself, at the same position)
    let template_body = if ctx.news.is_some() {
        let mut plain = ctx;
        let saved = plain.news.take();
        let t = template_text(&plain, &weather);
        plain.news = saved;
        ctx = plain;
        t
    } else {
        text.clone()
    };
    let template_parts = (kind == Kind::Morning).then(|| {
        let p = morning_parts(&ctx, &weather);
        json!({"greeting": p.greeting, "weather_comment": p.weather_comment, "affirmation": p.affirmation, "briefing": p.briefing})
    });
    let affirmation = match &ctx.week {
        Some(w) => week_line(w),
        None => daynote::note_text(&ctx.items, date, &ctx.goals, &ctx.info),
    };
    let stats = match &ctx.week {
        Some(w) => Stats { total: w.total, done: w.done, next_up: None },
        None => Stats { total: ctx.items.len(), done: count_done(&ctx.items), next_up: next_up(&ctx.items, date, ctx.today, &ctx.now) },
    };
    Ok(json!({
        "kind": kind.name(),
        "date": ds,
        "weather": weather,
        "stats": stats,
        "affirmation": affirmation,
        "template_text": text,
        "template_body": template_body,
        "template_parts": template_parts,
        "news": ctx.news.as_ref().map(|p| news::prep_json(p, kind, date, &env.ctx.news_generated_at, &ctx.assistant, "")),
        "cache_key": format!("briefing:{}:{ds}:{hash}", kind.name()),
        "plan_fingerprint": plan_fingerprint(&ctx),
        "system_prompt": system,
        "user_prompt": user,
        "review_prompt": review_prompt(&ctx.assistant, kind),
        "max_words": kind.max_words() + if ctx.news.is_some() { 350 } else { 0 },
    }))
}

/// The editor's brief for the second pass: one coherent piece, no repetition inside it or with the recent summaries.
pub fn review_prompt(assistant: &str, kind: Kind) -> String {
    let what = match kind {
        Kind::Morning => "morning briefing",
        Kind::Evening => "evening wind-down",
        Kind::Weekly => "weekly overview",
    };
    format!(
        "You are the careful editor of {assistant}'s spoken {what}. You are given the brief the writer followed, the context it was written from, the summaries {assistant} has already spoken recently, and the writer's DRAFT as a JSON object. Return the polished final version as a JSON object with exactly the same keys. \
EDIT FOR: (1) coherence: the parts and the news read as one natural, connected piece, in a sensible order, with smooth hand-offs, no contradictions (for example calling a task 'a bigger one' and then saying to clear it first as a quick one, or suggesting the same slot for two things), and each paragraph that opens a spoken section keeps its small spoken transition; (2) repetition inside the draft: no phrase, sentence opener, filler, care touch, joke, memory, idea or well-wish used twice, and varied sentence shapes; (3) overlap with the recent summaries: do not reuse their greeting, openings, sign-off, well-wishes, care touches, memories, facts, tips, jokes or distinctive turns of phrase; the same tasks may come up again because they are the plan, but say them freshly; if a memory, fact or tip in the draft already appeared recently, replace it with nothing rather than repeat it; (4) the voice: warm, relaxed, natural spoken prose as the brief describes, with only light fillers. \
KEEP: every task, time, appointment, workout, exercise, weight and fact in the draft; every news headline word for word inside its quotation marks, with the story told in full; the same paragraph structure and blank lines; roughly the same length (never noticeably shorter); all the brief's rules (no raw counts, fractions or percentages, no markdown, no lists). Never add facts that are not in the draft or the context. \
Reply with ONLY the JSON object, no markdown, no code fences, no notes about what you changed."
    )
}

pub fn parse_kind(s: Option<&str>) -> Res<Kind> {
    match s.unwrap_or("morning") {
        "morning" => Ok(Kind::Morning),
        "evening" => Ok(Kind::Evening),
        "weekly" => Ok(Kind::Weekly),
        _ => Err(ApiErr::bad("kind must be morning, evening or weekly")),
    }
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
        // quoted headlines may hold numbers and percentages
        assert!(!has_stats("The Guardian's headline is: \"1 in 5 workers say 40% of jobs are at risk\". It is a lot."));
        assert!(!has_stats("From the ABC, the headline is: “Prices up 5 percent”. It's a lot."));
        assert!(!has_stats("The ABC says 'Rates hold at 4.1%' and that's that."));
        assert!(has_stats("The ABC says \"hello\" and you did 3 of 5 things."));
        assert!(!has_stats("It's Sam's day, and Sam's list isn't long."));
    }
}
