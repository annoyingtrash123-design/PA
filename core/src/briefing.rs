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
use crate::taskbrain;
use crate::taskvoice;
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
    if let Some(t) = taskbrain::typical(o) {
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

/// One short line about a workout: what it is and what it trains.
fn gym_line(p: &WorkoutPlan, day: &str) -> String {
    if p.rest {
        return format!("It's a rest day at the gym {day}.");
    }
    match gym::trains(p) {
        Some(t) if p.is_cardio() => format!("The gym {day} is {}, for your {t}.", p.name.to_lowercase()),
        Some(t) => format!("The gym {day} is {}, which is {t}.", p.name),
        None => format!("The gym {day} is {}.", p.name),
    }
}

/// The missed (clock-time) tasks that wait for a decision, in one sentence.
fn missed_sentence(c: &Ctx) -> Option<String> {
    if c.missed.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = vec![];
    for o in c.missed.iter().take(2) {
        let wd = parse_date(&o.date).map(weekday_name).unwrap_or_else(|| "earlier".into());
        let what = match taskvoice::task_phrase(&o.title) {
            taskvoice::Phrase::Gerund(g) => g,
            taskvoice::Phrase::Noun(n) => {
                let n = n.strip_prefix("a ").or_else(|| n.strip_prefix("an ")).unwrap_or(&n).to_string();
                let lower = n.to_lowercase();
                if ["the ", "my ", "your "].iter().any(|p| lower.starts_with(p)) { n } else { format!("the {n}") }
            }
        };
        parts.push(format!("{what} from {wd}"));
    }
    let verb = if c.missed.len() == 1 { "is" } else { "are" };
    let what = if c.missed.len() == 1 { "a new time" } else { "new times" };
    Some(format!("{} {verb} waiting for {what} in Tasks.", cap(&parts.join(" and "))))
}

/// A day's open tasks as short spoken sentences: your own things (timed first), what happens at work, the routines.
fn day_sentences(items: &[Occurrence], info: &DayInfo, lead: &str) -> Vec<String> {
    let work_day = info.work.is_some();
    let open: Vec<&Occurrence> = items.iter().filter(|o| !o.done).collect();
    let at_work = |o: &Occurrence| work_day && o.time.is_none() && cat_name(o) == "Work" && !is_routine(o);
    let mut mine: Vec<&Occurrence> = open.iter().filter(|o| !is_routine(o) && !at_work(o)).cloned().collect();
    mine.sort_by(|a, b| match (&a.time, &b.time) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    });
    let work: Vec<&Occurrence> = open.iter().filter(|o| at_work(o)).cloned().collect();
    let routines: Vec<&Occurrence> = open.iter().filter(|o| is_routine(o)).cloned().collect();
    let names = |v: &[&Occurrence]| join(&v.iter().map(|o| taskvoice::inline_phrase(o)).collect::<Vec<_>>());
    let mut out = vec![];
    if !mine.is_empty() {
        out.push(format!("{lead} {}.", names(&mine)));
    }
    if let (Some(w), false) = (&info.work, work.is_empty()) {
        out.push(format!("At work, between {} and {}, there's {}.", say_time(&w.start), say_time(&w.end), names(&work)));
    }
    if !routines.is_empty() {
        out.push(format!("The usual {} there too: {}.", if routines.len() == 1 { "routine is" } else { "routines are" }, join(&routine_list(&routines))));
    }
    out
}

/// The built-in briefing body (no greeting, no weather: both are spoken first). Deliberately short and plain: it is the
/// reliable backup for when the AI text isn't available, not an imitation of it. Four short paragraphs: today, the
/// order, tomorrow, a closing line.
fn morning_template(c: &Ctx) -> String {
    let d = c.date;
    let work_day = c.info.work.is_some();
    // 1. today
    let mut p1: Vec<String> = vec![];
    if let Some(f) = digest::frame_sentence(&c.info) {
        p1.push(f);
    }
    let carried: Vec<String> = c.items.iter().filter(|o| !o.done && o.rolled_over_from.is_some()).map(taskvoice::inline_phrase).collect();
    if !carried.is_empty() {
        p1.push(format!("{} carried over from before.", cap(&limited(carried.clone(), 3))).replace(" carried over", if carried.len() == 1 { " has carried over" } else { " have carried over" }));
    }
    if let Some(m) = missed_sentence(c) {
        p1.push(m);
    }
    let today = day_sentences(&c.items, &c.info, "Today you've got");
    if today.is_empty() {
        p1.push(if c.items.is_empty() { "There's nothing on your list today.".into() } else { "Everything on today's list is already done.".to_string() });
    }
    p1.extend(today);
    match (&c.workout, &c.gym_done) {
        (_, Some(g)) => p1.push(format!("You've already done {g} at the gym.")),
        (Some(p), None) => p1.push(gym_line(p, "today")),
        _ => {}
    }

    // 2. the order
    let mut p2: Vec<String> = vec![];
    let open: Vec<&Occurrence> = c.items.iter().filter(|o| !o.done && !is_routine(o)).collect();
    let fixed: Vec<String> = open.iter().filter(|o| o.time.is_some()).map(|o| taskvoice::inline_phrase(o)).collect();
    if fixed.len() == 1 {
        p2.push(format!("The one fixed point is {}, so the rest can fit around it.", fixed[0]));
    } else if !fixed.is_empty() {
        p2.push(format!("The fixed points are {}, so the rest fits around those.", join(&fixed)));
    }
    let loose: Vec<&Occurrence> = open.iter().filter(|o| o.time.is_none() && !(work_day && cat_name(o) == "Work")).cloned().collect();
    let long = loose.iter().filter(|o| is_big(o)).max_by_key(|o| taskbrain::typical(o).map(|t| t.max).unwrap_or(45)).cloned();
    let quick: Vec<String> = loose.iter().filter(|o| !is_big(o)).map(|o| taskvoice::inline_phrase(o)).collect();
    if let Some(o) = long {
        p2.push(taskbrain::fit_sentence(o, work_day));
    }
    if quick.len() == 1 {
        p2.push(format!("{} is quick, so it'll fit in any gap.", cap(&quick[0])));
    } else if !quick.is_empty() {
        p2.push(format!("The quick ones, {}, will fit in the gaps.", limited(quick, 3)));
    }
    if p2.is_empty() {
        p2.push("Nothing needs a particular order today.".into());
    }
    if let Some(l) = c.leisure_pick.as_ref().filter(|_| c.leisure_room) {
        p2.push(leisure::morning_sentence(l, d, c.leisure_open_day));
    }

    // 3. tomorrow
    let mut p3: Vec<String> = vec![];
    if let Some((wd, items, w)) = &c.tomorrow {
        let notable: Vec<String> = items.iter().filter(|o| !is_routine(o)).take(4).map(taskvoice::inline_phrase).collect();
        p3.push(if notable.is_empty() { format!("Tomorrow, {wd}, is clear so far.") } else { format!("Tomorrow, {wd}, there's {}.", join(&notable)) });
        if let Some(p) = w {
            p3.push(gym_line(p, "tomorrow"));
        }
    }

    // 4. closing
    let p4 = closing_wish(d, &c.user);
    [p1.join(" "), p2.join(" "), p3.join(" "), p4].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
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
            // short and plain: the look back, tomorrow, goodnight (the news, when there is one, sits before the goodnight)
            let mut back: Vec<String> = vec![format!("Evening{n}.")];
            let done: Vec<String> = c.items.iter().filter(|o| o.done && !is_routine(o)).map(taskvoice::inline_phrase).collect();
            let left: Vec<&Occurrence> = c.items.iter().filter(|o| !o.done && !is_routine(o)).collect();
            back.push(if c.items.iter().all(is_routine) {
                "There was nothing much on your list today.".into()
            } else if done.is_empty() {
                "Nothing got ticked off today, and that's fine.".into()
            } else if left.is_empty() {
                format!("You got through everything today: {}.", limited(done, 4))
            } else {
                format!("Today you got through {}.", limited(done, 4))
            });
            let rolling: Vec<String> = left.iter().filter(|o| o.time.is_none() && !o.recurring).map(|o| taskvoice::inline_phrase(o)).collect();
            if !rolling.is_empty() {
                back.push(format!("{} will carry over to tomorrow on {} own.", cap(&limited(rolling.clone(), 4)), if rolling.len() == 1 { "its" } else { "their" }));
            }
            match (&c.workout, &c.gym_done, &c.cardio_done) {
                (_, Some(_), Some(cp)) => back.push(format!("You also got {cp} in.")),
                (_, Some(g), None) => back.push(format!("You also did {g} at the gym.")),
                (Some(p), None, _) if !p.rest => back.push(format!("{} is still waiting at the gym, whenever you're ready.", p.name)),
                _ => {}
            }
            if let Some(w) = weather {
                back.push(crate::weather::natural_sentence(&w.summary, true));
            }
            parts.push(back.join(" "));
            let mut tom: Vec<String> = vec![];
            if let Some((wd, items, w)) = &c.tomorrow {
                let lines = day_sentences(items, &c.tomorrow_info, &format!("Tomorrow, {wd}, you've got"));
                if lines.is_empty() {
                    tom.push(format!("Tomorrow, {wd}, is clear so far."));
                }
                tom.extend(lines);
                if let Some(f) = digest::frame_sentence(&c.tomorrow_info) {
                    tom.push(f.replace(" today.", " tomorrow.").replace("It's a", "It'll be a"));
                }
                if let Some(p) = w {
                    tom.push(gym_line(p, "tomorrow"));
                }
            }
            if let Some(l) = c.leisure_pick.as_ref().filter(|_| c.leisure_room) {
                tom.push(leisure::evening_sentence(l, d));
            }
            if !tom.is_empty() {
                parts.push("\n\n".into());
                parts.push(tom.join(" "));
            }
            if let Some(p) = &c.news {
                parts.push("\n\n".into());
                parts.push(news::template_text(&p.items, c.kind, c.date, p.position));
            }
            parts.push("\n\n".into());
            parts.push(format!("{} Sleep well.", taskvoice::signoff(d, &c.user, false)));
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

const OPENING_PARTS: &str = "PART greeting: one short, natural spoken hello, using the person's name if given, for example 'Morning, Sam!', 'Good morning, Sam. Hope you slept well.' or 'Morning! Grab a cuppa and let's look at today.' It must be a hello, NEVER a well-wish: do not open with 'I hope you have a lovely <day>' or anything like it. Well-wishes such as 'have a lovely Wednesday' belong only in the very last line of the briefing. One or two short sentences at most, no weather, no plans.\n\
PART weather_comment: 1 to 2 sentences interpreting the forecast, with tone and advice quietly shaped by the person's weather preferences (happy about cool days, a gentle heads-up and practical tips when hot, sun cream when UV is high and they avoid sunburn, cosy about cool rain if they like it) but NEVER stating or explaining those preferences, and tied to the day's plans rather than just read out. Temperatures as degrees, never a rain percentage. null if no weather is given.\n\
PART affirmation: 1 or 2 sentences, at most 40 words, grounded and SPECIFIC to today's actual plan: name a real task, appointment or workout, or the shape of the day (where the busy stretch is, where there is slack or a breather). Practical and encouraging, not soppy. Never state or narrate the person's preferences. If a goal plausibly relates to a task today you may tie them together in a few words, never forced. Never use clichés or generic affirmations like 'you\'ve got this', 'believe in yourself', 'one step at a time' or 'you are enough'. No raw counts.";

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
        Kind::Morning => "between 420 and 620 words, long enough to give every task its sentence or two (do not stop short)",
        Kind::Evening => "between 350 and 650 words, a soft floor rather than a cap",
    };
    let focus = match c.kind {
        Kind::Morning => "This is the main part of the MORNING briefing. A short hello, a weather comment and a 'for today' line are written separately and spoken just before yours, so you write the rest of the morning summary, WITHOUT any greeting and WITHOUT any weather (those are spoken first, so do not repeat them), flowing naturally with no headings: if the context says 'On this day: yes', start the briefing with ONE short spoken sentence about a well-known historical anniversary of today's calendar date that connects to the person's interests (use the interests text quietly), but ONLY if you are certain of both the date and the fact; if you are not completely certain, or the context says 'On this day: no', omit it entirely and never mention that you are omitting it. Write the briefing as FOUR short paragraphs separated by a blank line: first the look back and today broken down by theme, then the suggested order for the day, then tomorrow, then a one or two sentence closing line that ends with a varied, warm well-wish for the day (for example 'have a lovely Wednesday'). The whole morning is read aloud as THREE sections (1: greeting, weather, affirmation; 2: the first two paragraphs, the day itself; 3: tomorrow, the news if any, and the closing), each recorded separately, so the FIRST paragraph of the briefing must open with a small spoken transition ('Right, so…', 'Okay, so, um, your day.') and the THIRD paragraph (tomorrow) with another ('Oh, and…', 'Anyway…'). Then mention a public holiday or day off warmly if it is one (no work today); (1) a quick, kind look back at yesterday: name what got done and what rolled over, no counts, no guilt; (2) today walked through task by task using the TASK TALK rules below (every undone task by name, with the category as a natural lead-in such as 'on the life admin front', work, chores, errands, study, exercise and the gym, social plans, appointments with their times); routines and planning habits count as looking after yourself, not as work, and can be named together in one breath; (3) a suggested order for the day: decide whether to do the quick small tasks first to build momentum or the big one first while fresh, say which and why (use the size hints), respect fixed-time items and the work block (work hours are fixed and not a task; use the window between waking and the start of work for early things, the flexible lunch break only for short things that fit about an hour and plausibly sit near work like quick calls, bills, booking appointments or a quick shop; the lunch window is private background, so suggest such a thing only softly and optionally, in words like 'if you get a moment in the middle of the day, X would fit nicely', never stating the lunch times and never as an order like 'do X at lunch', and keep the evening finishing comfortably before bedtime); (4) a peek at tomorrow's notable things and appointments; (5) a grounded, specific closing line naming the suggested key task (a meaningful appointment or meeting, never a routine). Do not repeat the affirmation. Never use generic affirmations or cliches like 'you\'ve got this'.",
        Kind::Evening => "This is the EVENING wind-down, a relaxed, unhurried chat that winds the person down. Write it as three paragraphs separated by a blank line. First, a relaxed look back at what happened today: kindly say what got done by naming it (celebrate it, never guilt), mention what is left and that unfinished one-off tasks roll over to tomorrow, mention whether the gym happened, link what was done to a goal where it plausibly relates. Second, a preview of tomorrow, opening with something like 'So looking at what we've got on tomorrow', that walks through EVERY one of tomorrow's tasks using the TASK TALK rules below (what it involves, why it is worthwhile, when and how long, a touch of commentary), plus the gym with what it trains, and whether it is a work day, a public holiday or a day off. Routines can be named together in one breath. Recap today's weather conversationally in the past tense, in a sentence or two, if weather is given. The evening is read aloud as three separately recorded sections (the look back, the tomorrow preview, then the news if any plus the closing), so the second paragraph must open with a small spoken transition such as 'Right, so…' and the closing paragraph with 'Anyway…' or 'Oh, and…'. Third, a short closing paragraph: gently nudge toward winding down in time for sleep, then a cosy sign-off (see SIGN-OFF below) and a calm goodnight. If a news segment is requested below it is spoken between the second and third paragraph, so do not mention the news in your paragraphs. No generic affirmations or cliches.",
        Kind::Weekly => "This is the WEEKLY overview, read on Sunday. Open warmly. Pick two or three specific highlights that were actually done (task titles, gym sessions by name, and any exercise progressions) and connect them to the goals where they plausibly relate. Gently mention anything that slid, without guilt. Look ahead to the notable things next week and to any goal deadline coming up. Close with one short grounded line. If nothing much was done, be kind and low-key, give no stats, and focus on the week ahead and the goals. Do not list every task and never quote totals. No generic affirmations or cliches.",
    };
    format!(
        "You are {a}, the user's personal assistant and friend. You are writing a script that will be read aloud by a text-to-speech voice, so write exactly how a chatty, caring friend would talk.\n\
         {spoken}\nRules: plain spoken sentences only. No markdown, no asterisks, no headings, no bullet points, no numbered lists, no emojis, no stage directions, no quotation marks around the whole text. \
         Say times naturally, like 'at seven thirty in the morning' or 'at 3 PM'. Say 'kilos' instead of 'kg'. Weather temperatures are in Celsius and should be said as degrees. \
         Only use facts from the context; never invent tasks, numbers or weather. Keep it {words}. Output only the script, as plain text with a blank line between paragraphs.\n{TONE}\n{kb}\n{focus}{rules}{nonews}",
        kb = knowledge::RULES,
        a = c.assistant,
        spoken = concat!(spoken_rule!(), style_guide!()),
        rules = match c.kind {
            Kind::Morning => format!("\n{}", taskvoice::PROMPT_RULES),
            Kind::Evening => format!("\n{}{}", taskvoice::PROMPT_RULES, taskvoice::EVENING_SIGNOFF_RULE),
            Kind::Weekly => String::new(),
        },
        nonews = if c.kind == Kind::Weekly { "" } else { "\nThe news is written separately and slotted in for you, so do not mention the news at all." }
    )
}

/// The small separate request for the three short opening parts of the morning (hello, weather line, "for today").
fn opening_prompt(c: &Ctx) -> String {
    format!(
        "You are {a}, the user's personal assistant and friend, who has looked after them for a long time and knows them well. You write the three short opening parts of the MORNING briefing, which a text-to-speech voice reads aloud in this order, just before the main part: greeting, weather_comment, affirmation.\n\
         {spoken}\nRules: plain spoken sentences only. No markdown, no emojis, no lists. Only use facts from the context; never invent tasks, numbers or weather. Warm, relaxed and natural, never like a report, with at most one light filler across all three parts. {names}\n\
         OPENING_PARTS\n\
         Reply with ONLY a JSON object {{\"greeting\": string, \"weather_comment\": string or null, \"affirmation\": string}}, no markdown, no code fences. \"weather_comment\" is null if no weather is given. No part may contain a rain percentage or any count.",
        a = c.assistant,
        spoken = spoken_rule!(),
        names = "Say tasks the way a person would, never as typed commands ('a doctor's appointment', not 'go to the doctor')."
    )
    .replace("OPENING_PARTS", OPENING_PARTS)
}

/// The context block for the writer (the news stories go to their own request).
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
            let size = match taskbrain::typical(i) {
                Some(t) => format!("; {}", taskbrain::context_hint(&t)),
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
        "opening": (kind == Kind::Morning).then(|| json!({"system_prompt": opening_prompt(&ctx), "user_prompt": user.clone()})),
        "max_words": kind.max_words() + if ctx.news.is_some() { 350 } else { 0 },
    }))
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
