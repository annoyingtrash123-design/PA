//! How the morning and evening summaries talk about tasks: the no-AI template wording (a sentence or two per
//! task, from the signals a task carries) and the matching rules for the Gemini prompts.
use crate::briefing::is_big;
use crate::goals;
use crate::model::Goal;
use crate::tasks::Occurrence;
use crate::util::*;
use chrono::{Datelike, NaiveDate};

/// Prompt rules for the per-task treatment, gym, weather, leisure and sign-off (morning and evening only).
pub const PROMPT_RULES: &str = "TASK TALK (morning: today's tasks; evening: tomorrow's tasks): speak warmly and naturally, mixing a collaborative 'we' ('so looking at what we've got on today, <name>…') with plain 'you' ('you've got the dentist at three', 'you might want to…') the way a real friend or assistant would; don't overuse either, and use the person's name now and then, never in every sentence. \
Go through EVERY undone task for that day by name. Never say 'plus a few more', 'a few other things' or leave a task out; if there are many, give the bigger or timed ones a sentence or two each and group the very small ones into one breath, but still name each of them. \
For EACH task say, in one or two natural sentences: (a) what it practically involves, with a concrete first step (use its notes and steps if they are given, otherwise a sensible common-sense first step, without inventing facts about the person's life); (b) why it is worthwhile, linking it to one of the goals or the person's interests ONLY where that genuinely fits (never forced; linking is fine for tasks but never for news); (c) when and for how long to fit it, using its time, part of day, size hint and the work block ('set aside fifteen to twenty minutes in the morning if you can, otherwise save it for an evening read'); (d) where it fits, with a touch of genuine, thoughtful commentary such as a pro and a con or what to watch for. \
Illustration with made-up tasks (match the style, never reuse these tasks): 'So looking at what we've got on today, we've got a few things to do. First, renewing the car registration, which is mostly finding the reminder letter and paying online. It's worth doing now because a late fee is a silly way to lose money, so set aside ten minutes before work. Then there's sorting the garden shed, and I'd start by clearing just the floor. It'll feel good to get it done, though it can swallow an afternoon, so give it a proper hour on a day with no plans.' \
GYM: when there is a workout, say what it trains in plain words (Push is chest, shoulders and triceps; Pull is back, biceps and rear delts; Legs and Abs is legs, glutes and core; Cardio is heart, lungs and endurance; work it out from the workout's name, notes and exercises) and mention the key lift or the progression hint. \
WEATHER: when you recap or mention it, be conversational and grammatical, never reading the label out as a sentence (not 'it was thunderstorm today' but 'pretty wild weather today, wasn't it? Lots of rain and a thunderstorm.'). \
LEISURE: when you suggest something for free time you may point to the person's list generally ('your watch list is there if you fancy it') or to one specific item. ";

/// Evening-only rule: the cosy sign-off.
pub const EVENING_SIGNOFF_RULE: &str = "SIGN-OFF: end with a cosy, specific sign-off that says nothing else needs attention tonight and invites the person to get comfy, for example 'Nothing else needs your attention tonight, so get comfy and enjoy a quiet one.' ";

/// Who and what the template is talking about.
pub struct Look<'a> {
    pub d: NaiveDate,
    pub goals: &'a [Goal],
    /// the day in question is a work day (there is a work block to fit around)
    pub work: bool,
}

fn cat_name(o: &Occurrence) -> String {
    o.category.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "Other".into())
}

fn cat_opener(cat: &str) -> Option<&'static str> {
    Some(match cat {
        "Appointments" => "On the appointments side",
        "Work" => "On the work side",
        "Admin & Money" => "On the admin side",
        "Chores" => "Around the house",
        "Errands" => "Out and about",
        "Study" => "On the study side",
        "Exercise" => "For movement",
        "Social" => "On the social side",
        "Self-care" => "For looking after yourself",
        _ => return None,
    })
}

// ------------------------------------------------------------------ naturally phrased titles

/// Imperative verb at the start of a task title and the gerund it becomes inside a sentence.
const VERBS: &[(&str, &str)] = &[
    ("try", "trying"), ("read", "reading"), ("make", "making"), ("organise", "organising"), ("organize", "organizing"), ("call", "calling"),
    ("book", "booking"), ("pay", "paying"), ("check", "checking"), ("write", "writing"), ("clean", "cleaning"), ("buy", "buying"),
    ("email", "emailing"), ("finish", "finishing"), ("start", "starting"), ("plan", "planning"), ("sort", "sorting"), ("send", "sending"),
    ("fix", "fixing"), ("order", "ordering"), ("renew", "renewing"), ("cancel", "cancelling"), ("submit", "submitting"), ("review", "reviewing"),
    ("update", "updating"), ("tidy", "tidying"), ("wash", "washing"), ("pick", "picking"), ("drop", "dropping"), ("return", "returning"),
    ("research", "researching"), ("learn", "learning"), ("practise", "practising"), ("schedule", "scheduling"), ("prepare", "preparing"),
    ("pack", "packing"), ("clear", "clearing"), ("declutter", "decluttering"), ("print", "printing"), ("reply", "replying"), ("find", "finding"),
    ("get", "getting"), ("take", "taking"), ("do", "doing"), ("go", "going"), ("visit", "visiting"), ("text", "texting"), ("message", "messaging"),
    ("ring", "ringing"), ("arrange", "arranging"), ("confirm", "confirming"), ("download", "downloading"), ("install", "installing"),
    ("complete", "completing"), ("cook", "cooking"), ("bake", "baking"), ("walk", "walking"), ("watch", "watching"), ("create", "creating"),
    ("draft", "drafting"), ("look", "looking"), ("set", "setting"), ("sign", "signing"), ("apply", "applying"), ("research", "researching"),
];
/// Verbs that take "some" before a bare plural ("making some book club inquiries").
const SOME_VERBS: &[&str] = &["make", "buy", "send", "write", "order", "print", "book"];
const DETERMINERS: &[&str] = &["a", "an", "the", "my", "your", "our", "his", "her", "their", "some", "any", "this", "that", "these", "those", "more", "new", "another", "all", "no", "about"];

/// A task title as it reads inside a sentence.
#[derive(Clone, Debug, PartialEq)]
pub enum Phrase {
    /// "reading about the mining boom": from an imperative title
    Gerund(String),
    /// the title as written
    Noun(String),
}

impl Phrase {
    pub fn text(&self) -> &str {
        match self {
            Phrase::Gerund(s) | Phrase::Noun(s) => s,
        }
    }
}

/// Turn "Read about the mining boom" into "reading about the mining boom"; other titles stay as written.
pub fn task_phrase(title: &str) -> Phrase {
    let t = title.trim();
    let mut it = t.split_whitespace();
    let Some(first) = it.next() else { return Phrase::Noun(t.to_string()) };
    let rest: Vec<&str> = it.collect();
    let key = first.to_lowercase();
    let Some((_, ger)) = VERBS.iter().find(|(v, _)| *v == key) else { return Phrase::Noun(t.to_string()) };
    // "Book club", "Order of service" and the like are nouns
    if let Some(n) = rest.first().map(|w| w.to_lowercase()) {
        if (key == "book" && matches!(n.as_str(), "club" | "shelf" | "launch")) || (key == "order" && n == "of") || (key == "sign" && n == "off") {
            return Phrase::Noun(t.to_string());
        }
    }
    if first.chars().any(|c| !c.is_alphabetic()) {
        return Phrase::Noun(t.to_string());
    }
    let mut out = (*ger).to_string();
    if !rest.is_empty() {
        out.push(' ');
        let first_obj = rest[0].to_lowercase();
        let last = rest[rest.len() - 1].to_lowercase();
        let plural = last.len() > 3 && last.ends_with('s') && !last.ends_with("ss") && !last.ends_with("us") && !last.ends_with("is");
        if SOME_VERBS.contains(&key.as_str()) && plural && !DETERMINERS.contains(&first_obj.as_str()) && !first_obj.chars().any(|c| c.is_ascii_digit()) {
            out.push_str("some ");
        }
        out.push_str(&rest.join(" "));
    }
    Phrase::Gerund(out)
}

/// The task as it reads mid-sentence, with its time ("calling mum at 3 PM").
pub fn inline_phrase(o: &Occurrence) -> String {
    let p = task_phrase(&o.title).text().to_string();
    match &o.time {
        Some(t) => format!("{p} at {}", say_time(t)),
        None => p,
    }
}

/// What a sentence can say about a task: gerunds stand alone, plain titles get "the ... one".
fn subject(o: &Occurrence, i: usize) -> (String, bool) {
    match task_phrase(&o.title) {
        Phrase::Gerund(_) => (inline_phrase(o), true),
        Phrase::Noun(t) => {
            let l = t.to_lowercase();
            let bare = o.time.is_some() || ["the ", "a ", "an ", "my ", "your ", "our ", "this ", "that "].iter().any(|p| l.starts_with(p)) || t.chars().next().is_some_and(|c| c.is_ascii_digit());
            if bare {
                (inline_phrase(o), false)
            } else {
                (if i % 2 == 0 { format!("the {t} one") } else { format!("the {t} task") }, false)
            }
        }
    }
}

// ------------------------------------------------------------------ pools without repeats

/// What this summary has already said, so no stock line is used twice.
#[derive(Default)]
pub struct Used {
    lines: Vec<&'static str>,
    openers: Vec<String>,
}

/// The first template in `pool`, starting at `start`, that has not been used yet.
fn fresh(pool: &[&'static str], start: usize, used: &mut Used) -> Option<&'static str> {
    (0..pool.len()).map(|k| pool[(start + k) % pool.len()]).find(|c| !used.lines.contains(c)).inspect(|c| used.lines.push(c))
}

fn why_pool(cat: &str) -> Vec<&'static str> {
    let mut v: Vec<&'static str> = match cat {
        "Work" => vec!["it clears the decks for the rest of the week", "it keeps work from piling up behind you", "it's one less thing waiting for you at your desk"],
        "Admin & Money" => vec!["it's the sort of thing that's nicer off your mind", "future you will be glad it's sorted", "it's boring, but it stops being a worry once it's done"],
        "Chores" => vec!["it keeps the place easy to live in", "the house will feel better for it", "it keeps the little jobs from piling up"],
        "Errands" => vec!["doing it now saves a scramble later", "it's easier to fit in when nothing is pressing", "it means one less trip to think about"],
        "Study" => vec!["little and often is how study sticks", "a bit of focus here goes a long way", "it's the kind of thing that adds up quietly"],
        "Social" => vec!["time with people is worth protecting", "a little connection does everyone good", "it's the sort of thing you won't regret"],
        "Exercise" => vec!["it keeps your energy up", "your body will thank you for it", "movement tends to clear the head too"],
        "Self-care" => vec!["it keeps your rhythm going", "it's good for you in a quiet way", "small kindnesses to yourself add up"],
        "Appointments" => vec!["it's one of the fixed points", "it's worth getting there without a rush", "it's the one the day pivots around"],
        _ => vec![],
    };
    v.extend(["it's one less thing hanging over you", "it'll feel good to have it ticked off", "getting it out of the way means one fewer thing nagging at you", "it's a nice one to have behind you"]);
    v
}

fn timing_pool(o: &Occurrence, big: bool, lk: &Look) -> Vec<&'static str> {
    if o.time.is_some() {
        return vec![
            "it's pinned to the clock, so we'll plan the rest of the day around it",
            "it's a fixed point, so give yourself a little buffer beforehand",
            "the time is set, so build a bit of breathing room around it",
        ];
    }
    match (o.part_of_day.as_str(), big) {
        ("morning", true) => vec!["it's a bigger one, so set aside a proper half hour in the morning while you're fresh", "it needs a proper half hour, and the morning is when you're sharpest", "it's meatier than the rest, so give it half an hour first thing"],
        ("morning", false) => vec!["it's a quick one, so ten minutes in the morning will do it", "ten minutes in the morning should cover it", "it won't take long, so squeeze it in early"],
        ("afternoon", true) => vec!["it's a bigger one, so set aside a proper half hour in the afternoon", "it needs a decent half hour, so the afternoon suits it well", "it's a proper chunk of effort, so give it the afternoon's best half hour"],
        ("afternoon", false) => vec!["it's a quick one, so ten minutes in the afternoon will do it", "ten minutes after lunch should cover it", "it won't take long, so slot it into the afternoon"],
        ("evening", true) => vec!["it's a bigger one, so set aside a proper half hour in the evening, once everything else is done", "it needs half an hour, so keep it for the evening when things are calmer", "it's a substantial one, so save it for the evening and take your time"],
        ("evening", false) => vec!["it's a quick one, so ten minutes in the evening is plenty", "ten minutes this evening will do it", "it won't take long, so tuck it in after dinner"],
        (_, true) if lk.work => vec!["it's a bigger one, so set aside a proper half hour before work if you can, otherwise save it for the evening", "it's a bigger one, so set aside a proper half hour, before work if you can, or save it for an evening when you're winding down", "it needs a proper half hour, so aim for before work or after you've finished"],
        (_, true) => vec!["it's a bigger one, so set aside a proper half hour somewhere comfortable in the day", "it's a bigger one, so give it a proper half hour while you're fresh", "it needs half an hour of real attention, so pick a calm patch"],
        (_, false) if lk.work => vec!["it's a quick one, maybe ten minutes, so the middle of the day works if you have a moment", "it only needs ten minutes or so, so squeeze it in before work or in a spare moment", "it's a small one, so a gap before or after work will do"],
        (_, false) => vec!["it's a quick one, maybe ten minutes, so do it whenever suits", "it only needs ten minutes or so, so slot it in wherever there's a gap", "it won't take long, so do it when the mood strikes"],
    }
}

const CATCH: [&str; 3] = ["The catch is that it can drag, so I wouldn't start it when you're tired.", "It could swallow more time than you expect, so don't begin it when you're running low.", "Watch out, because these have a way of growing, so keep an eye on the clock."];
const UPSIDE: [&str; 3] = ["The upside is that once it's done it's properly out of the way.", "The nice part is how much lighter the day feels afterwards.", "Once it's finished, everything else will feel easier by comparison."];

/// The notes text, cut to a short spoken fragment without its closing punctuation.
fn note_fragment(o: &Occurrence) -> Option<String> {
    let first = o.notes.lines().map(str::trim).find(|l| !l.is_empty())?;
    let words: Vec<&str> = first.split_whitespace().take(28).collect();
    let t = words.join(" ");
    let t = t.trim_end_matches(['.', '!', '?', ',', ';', ':']).to_string();
    (!t.is_empty()).then_some(t)
}

const GOAL_TIES: [&str; 3] = ["it ties in nicely with", "it quietly helps with", "it's a nudge toward"];

/// One or two sentences (three for a bigger untimed one) about a single task. `goal` is the one goal mention allowed
/// in the whole summary; only the task it belongs to gets it.
pub fn blurb(o: &Occurrence, i: usize, lk: &Look, used: &mut Used, goal: Option<&Goal>) -> String {
    let cat = cat_name(o);
    let big = is_big(o);
    let (subj, gerund) = subject(o, i);
    let salt = i + lk.d.num_days_from_ce() as usize;
    // an opener ("On the study side") is used once per category; later tasks get plain connectors
    let opener = cat_opener(&cat).filter(|op| !used.openers.iter().any(|u| u == op));
    let lead: String = match (opener, gerund) {
        (Some(op), true) => {
            used.openers.push(op.to_string());
            match fresh(&["{op}, there's {s}", "{op}, you'll be {s}", "{op}, we've got {s} lined up"], salt, used).unwrap_or("{op}, there's {s}") {
                t => t.replace("{op}", op).replace("{s}", &subj),
            }
        }
        (Some(op), false) => {
            used.openers.push(op.to_string());
            fresh(&["{op}, there's {s}", "{op}, you've got {s}", "{op}, we've got {s}"], salt, used).unwrap_or("{op}, there's {s}").replace("{op}", op).replace("{s}", &subj)
        }
        (None, true) if used.lines.is_empty() => fresh(&["First up, there's {s}", "To start, you'll be {s}", "First up is {s}"], salt, used).unwrap_or("First up is {s}").replace("{s}", &subj),
        (None, true) => fresh(&["Then there's {s}", "Next, you'll be {s}", "Also on the list is {s}", "Then comes {s}", "There's also {s}"], salt, used).unwrap_or("Also on the list is {s}").replace("{s}", &subj),
        (None, false) if used.lines.is_empty() => fresh(&["First up, there's {s}", "To start, we've got {s}"], salt, used).unwrap_or("First up, there's {s}").replace("{s}", &subj),
        (None, false) => fresh(&["Then there's {s}", "There's also {s}", "Next, you've got {s}", "After that we've got {s}"], salt, used).unwrap_or("Then there's {s}").replace("{s}", &subj),
    };
    let mut s1 = lead;
    if let Some(n) = note_fragment(o) {
        s1.push_str(&format!(", and your note on it says {n}"));
    } else if let Some(step) = o.checklist.iter().find(|c| !c.done) {
        s1.push_str(&format!(", and the checklist starts with {}", step.text.trim().trim_end_matches('.').to_lowercase()));
    }
    s1.push('.');
    let reason: Option<String> = match goal {
        Some(g) => Some(format!("{} {}", GOAL_TIES[i % 3], goals::goal_ref(g))),
        None => fresh(&why_pool(&cat), salt, used).map(String::from),
    };
    let when = fresh(&timing_pool(o, big, lk), salt, used).map(String::from);
    let mut s2 = match (reason, when) {
        (Some(r), Some(w)) => match salt % 3 {
            0 => format!("{}, and {w}.", cap(&r)),
            1 => format!("{}, and {r}.", cap(&w)),
            _ => format!("{}. {}.", cap(&w), cap(&r)),
        },
        (Some(x), None) | (None, Some(x)) => format!("{}.", cap(&x)),
        (None, None) => String::new(),
    };
    if big && o.time.is_none() {
        let pool: &[&'static str] = if salt % 2 == 0 { &CATCH } else { &UPSIDE };
        if let Some(c) = fresh(pool, salt, used) {
            if !s2.is_empty() {
                s2.push(' ');
            }
            s2.push_str(c);
        }
    }
    if s2.is_empty() { s1 } else { format!("{s1} {s2}") }
}

/// Order for the walk-through: fixed times first, then bigger things, then the quick ones.
fn ordered<'a>(items: &[&'a Occurrence]) -> Vec<&'a Occurrence> {
    let mut v: Vec<&Occurrence> = items.to_vec();
    v.sort_by(|a, b| match (&a.time, &b.time) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => is_big(b).cmp(&is_big(a)),
    });
    v
}

/// Every task, by name: the first few with a full blurb each, any extra (from eight tasks on) named in one sentence.
/// At most one task is tied to a goal (when `allow_goal`); the bool says whether a goal was mentioned.
pub fn walk_through(items: &[&Occurrence], lk: &Look, allow_goal: bool) -> (String, bool) {
    let v = ordered(items);
    let full = if v.len() >= 8 { 5 } else { v.len() };
    let texts: Vec<(String, String)> = v.iter().take(full).map(|o| (o.title.clone(), o.title.clone())).collect();
    let link = if allow_goal { goals::find_link(lk.goals, &texts) } else { None };
    let link_idx = link.as_ref().and_then(|(_, label)| v.iter().take(full).position(|o| &o.title == label));
    let mut used = Used::default();
    let mut parts: Vec<String> = v
        .iter()
        .take(full)
        .enumerate()
        .map(|(i, o)| blurb(o, i, lk, &mut used, if link_idx == Some(i) { link.as_ref().map(|(g, _)| *g) } else { None }))
        .collect();
    if v.len() > full {
        let rest: Vec<String> = v[full..].iter().map(|o| inline_phrase(o)).collect();
        parts.push(format!("The smaller bits are {}, which can go wherever there's a gap.", join(&rest)));
    }
    (parts.join(" "), link_idx.is_some())
}

/// "a few things" style opener quantity.
pub fn how_many(n: usize) -> &'static str {
    match n {
        0 => "nothing",
        1 => "one thing to do",
        2 => "a couple of things to do",
        _ => "a few things to do",
    }
}

/// Cosy evening sign-off lines; `{n}` is replaced by ", name" or nothing.
pub fn signoff(d: NaiveDate, name: &str, with_name: bool) -> String {
    let n = if with_name { name_suffix(name) } else { String::new() };
    let pool: [String; 4] = [
        format!("Nothing else needs your attention tonight{n}, so get comfy and enjoy a quiet one."),
        format!("That's everything for now{n}, so there's nothing else to think about tonight. Put your feet up and get cosy."),
        format!("Nothing more needs doing tonight{n}, so settle in with a cup of tea and let the evening be easy."),
        format!("Nothing else needs your attention tonight{n}, so get comfy and switch off for a bit."),
    ];
    pool[idx(d, 77, pool.len())].clone()
}
