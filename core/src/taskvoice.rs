//! How the morning and evening summaries talk about tasks: the no-AI template wording (a sentence or two per
//! task, from the signals a task carries) and the matching rules for the Gemini prompts.
use crate::briefing::is_big;
use crate::digest::item_phrase;
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

/// Why it is worth doing, as a clause that follows "it" or stands alone after a capital.
fn why(cat: &str, i: usize) -> &'static str {
    match cat {
        "Work" => "it clears the decks for the rest of the week",
        "Admin & Money" => "it's the sort of thing that's nicer off your mind",
        "Chores" => "it keeps the place easy to live in",
        "Errands" => "doing it now saves a scramble later",
        "Study" => "little and often is how study sticks",
        "Social" => "time with people is worth protecting",
        "Exercise" => "it keeps your energy up",
        "Self-care" => "it keeps your rhythm going",
        "Appointments" => "it's one of the fixed points",
        _ => ["it's one less thing hanging over you", "it'll feel good to have it ticked off", "getting it out of the way means one fewer thing nagging at you"][i % 3],
    }
}

/// A clause (starting with "it" or an imperative) about when and for how long to do it.
fn timing(o: &Occurrence, big: bool, lk: &Look, i: usize) -> String {
    let alt = |a: &str, b: &str| if (i + lk.d.num_days_from_ce() as usize) % 2 == 0 { a.to_string() } else { b.to_string() };
    if o.time.is_some() {
        return alt("it's pinned to the clock, so we'll plan the rest of the day around it", "it's a fixed point, so give yourself a little buffer beforehand");
    }
    match (o.part_of_day.as_str(), big) {
        ("morning", true) => "it's a bigger one, so set aside a proper half hour in the morning while you're fresh".into(),
        ("morning", false) => "it's a quick one, so ten minutes in the morning will do it".into(),
        ("afternoon", true) => "it's a bigger one, so set aside a proper half hour in the afternoon".into(),
        ("afternoon", false) => "it's a quick one, so ten minutes in the afternoon will do it".into(),
        ("evening", true) => "it's a bigger one, so set aside a proper half hour in the evening, once everything else is done".into(),
        ("evening", false) => "it's a quick one, so ten minutes in the evening is plenty".into(),
        (_, true) if lk.work => alt("it's a bigger one, so set aside a proper half hour before work if you can, otherwise save it for the evening", "it's a bigger one, so set aside a proper half hour, before work if you can, or save it for an evening when you're winding down"),
        (_, true) => alt("it's a bigger one, so set aside a proper half hour somewhere comfortable in the day", "it's a bigger one, so give it a proper half hour while you're fresh"),
        (_, false) if lk.work => alt("it's a quick one, maybe ten minutes, so the middle of the day works if you have a moment", "it only needs ten minutes or so, so squeeze it in before work or in a spare moment"),
        (_, false) => alt("it's a quick one, maybe ten minutes, so do it whenever suits", "it only needs ten minutes or so, so slot it in wherever there's a gap"),
    }
}

/// The notes text, cut to a short spoken fragment without its closing punctuation.
fn note_fragment(o: &Occurrence) -> Option<String> {
    let first = o.notes.lines().map(str::trim).find(|l| !l.is_empty())?;
    let words: Vec<&str> = first.split_whitespace().take(28).collect();
    let t = words.join(" ");
    let t = t.trim_end_matches(['.', '!', '?', ',', ';', ':']).to_string();
    (!t.is_empty()).then_some(t)
}

/// One or two sentences (three for a bigger untimed one) about a single task.
pub fn blurb(o: &Occurrence, i: usize, lk: &Look, used: &mut Vec<String>) -> String {
    let cat = cat_name(o);
    let big = is_big(o);
    let title = item_phrase(o);
    let mut s1 = match (cat_opener(&cat), (i + lk.d.num_days_from_ce() as usize) % 3) {
        (Some(op), 0) => format!("{op}, there's {title}"),
        (Some(op), 1) => format!("{op}, you've got {title}"),
        (Some(op), _) => format!("{op}, we've got {title}"),
        (None, _) if i == 0 => format!("First up, there's {title}"),
        (None, 0) => format!("There's also {title}"),
        (None, _) => format!("Then there's {title}"),
    };
    if let Some(n) = note_fragment(o) {
        s1.push_str(&format!(", and your note on it says {n}"));
    } else if let Some(step) = o.checklist.iter().find(|c| !c.done) {
        s1.push_str(&format!(", and the checklist starts with {}", step.text.trim().trim_end_matches('.').to_lowercase()));
    }
    s1.push('.');
    // a goal is tied to at most one task per summary, so it never sounds like a refrain
    let linked = goals::find_link(lk.goals, &[(o.title.clone(), o.title.clone())]).filter(|(g, _)| !used.contains(&g.title));
    let reason = match linked {
        Some((g, _)) => {
            used.push(g.title.clone());
            match i % 3 {
                0 => format!("it feeds into {}", goals::goal_ref(g)),
                1 => format!("it quietly helps with {}", goals::goal_ref(g)),
                _ => format!("it's a nudge toward {}", goals::goal_ref(g)),
            }
        }
        None => why(&cat, i).to_string(),
    };
    let mut s2 = format!("{}, and {}.", cap(&reason), timing(o, big, lk, i));
    if big && o.time.is_none() {
        s2.push_str(if i % 2 == 0 { " The catch is that it can drag, so I wouldn't start it when you're tired." } else { " The upside is that once it's done it's properly out of the way." });
    }
    format!("{s1} {s2}")
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
pub fn walk_through(items: &[&Occurrence], lk: &Look) -> String {
    let v = ordered(items);
    let full = if v.len() >= 8 { 5 } else { v.len() };
    let mut used: Vec<String> = vec![];
    let mut parts: Vec<String> = v.iter().take(full).enumerate().map(|(i, o)| blurb(o, i, lk, &mut used)).collect();
    if v.len() > full {
        let rest: Vec<String> = v[full..].iter().map(|o| item_phrase(o)).collect();
        parts.push(format!("The smaller bits are {}, which can go wherever there's a gap.", join(&rest)));
    }
    parts.join(" ")
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
