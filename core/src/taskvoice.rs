//! How the morning and evening summaries talk about tasks: the no-AI template wording (a sentence or two per
//! task, from the signals a task carries) and the matching rules for the Gemini prompts.
use crate::tasks::Occurrence;
use crate::util::*;
use chrono::NaiveDate;

/// Prompt rules for the per-task treatment, gym, weather, leisure and sign-off (morning and evening only).
pub const PROMPT_RULES: &str = "TASK NAMES: the person types tasks as quick notes or commands ('go to the doctor', 'call mum', 'add compliance notes to Excel'). Never read a title out as typed or drop it into a sentence as a bare command (never 'you\'ve got go to the doctor'). Say each one the way a person would: 'you\'ve got a doctor\'s appointment later today', 'you\'ll be calling your mum', 'there\'s a haircut on the cards', 'you\'ve got the compliance notes to add to that Excel sheet'. Keep names, places and specifics exactly, but fix capitals and grammar. \
WORK TASKS: tasks in the Work category are done AT work, inside the work block, as part of the workday. Talk about them together with the work hours ('once you're at work, you've got…', 'at work today, between ten and five, there's…'), never as something separate from work, and never suggest doing them before work, after work, at lunch or in the evening. On a day off, a holiday or the weekend, a Work task can wait for the next workday unless it is dated that day. \
COMMON SENSE ABOUT TIME: give realistic durations the way a sensible person would. Use a task's 'usually takes' hint when the context gives one. Anything that means going somewhere (a haircut, the dentist, a GP visit, a grocery shop, a pick-up) also needs travel and often a wait, so never call it a ten-minute job: a haircut is about half an hour to 45 minutes in the chair plus getting there, a GP or dentist visit about an hour all up. Only suggest the lunch break for things that genuinely fit in it with travel (a quick call, a bill, booking something, a nearby errand). Booking or phoning about something is quick even when the thing itself is not. Never squeeze an outing into a gap that is too short, and keep times and suggestions physically possible on the day. \
TASK TALK (morning: today's tasks; evening: tomorrow's tasks): speak warmly and naturally, mixing a collaborative 'we' ('so looking at what we've got on today, <name>…') with plain 'you' ('you've got the dentist at three', 'you might want to…') the way a real friend or assistant would; don't overuse either, and use the person's name now and then, never in every sentence. \
Go through EVERY undone task for that day by name. Never say 'plus a few more', 'a few other things' or leave a task out; if there are many, give the bigger or timed ones a sentence or two each and group the very small ones into one breath, but still name each of them. \
For EACH task say, in one or two natural sentences: (a) what it practically involves, with a concrete first step (use its notes and steps if they are given, otherwise a sensible common-sense first step, without inventing facts about the person's life); (b) why it is worthwhile, linking it to one of the goals or the person's interests ONLY where that genuinely fits (never forced; linking is fine for tasks but never for news); (c) when and for how long to fit it, using its time, part of day, size hint and the work block ('set aside fifteen to twenty minutes in the morning if you can, otherwise save it for an evening read'); (d) where it fits, with a touch of genuine, thoughtful commentary such as a pro and a con or what to watch for. \
Illustration with made-up tasks (match the style, never reuse these tasks): 'So looking at what we've got on today, we've got a few things to do. First, renewing the car registration, which is mostly finding the reminder letter and paying online. It's worth doing now because a late fee is a silly way to lose money, so set aside ten minutes before work. Then there's sorting the garden shed, and I'd start by clearing just the floor. It'll feel good to get it done, though it can swallow an afternoon, so give it a proper hour on a day with no plans.' \
GYM: when there is a workout, say what it trains in plain words (Push is chest, shoulders and triceps; Pull is back, biceps and rear delts; Legs and Abs is legs, glutes and core; Cardio is heart, lungs and endurance; work it out from the workout's name, notes and exercises) and mention the key lift or the progression hint. \
WEATHER: when you recap or mention it, be conversational and grammatical, never reading the label out as a sentence (not 'it was thunderstorm today' but 'pretty wild weather today, wasn't it? Lots of rain and a thunderstorm.'). \
LEISURE: when you suggest something for free time you may point to the person's list generally ('your watch list is there if you fancy it') or to one specific item. ";

/// Evening-only rule: the cosy sign-off.
pub const EVENING_SIGNOFF_RULE: &str = "SIGN-OFF: end with a cosy, specific sign-off that says nothing else needs attention tonight and invites the person to get comfy, for example 'Nothing else needs your attention tonight, so get comfy and enjoy a quiet one.' ";

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
    ("gather", "gathering"), ("collect", "collecting"), ("drink", "drinking"), ("stretch", "stretching"), ("meet", "meeting"),
    ("empty", "emptying"), ("vacuum", "vacuuming"), ("mop", "mopping"), ("fold", "folding"), ("hang", "hanging"), ("feed", "feeding"),
    ("study", "studying"), ("revise", "revising"), ("register", "registering"),
    ("use", "using"), ("add", "adding"), ("transfer", "transferring"), ("move", "moving"), ("upload", "uploading"),
    ("share", "sharing"), ("scan", "scanning"), ("fill", "filling in"), ("migrate", "migrating"), ("compile", "compiling"),
    ("finalise", "finalising"), ("finalize", "finalizing"), ("edit", "editing"), ("proofread", "proofreading"),
    ("summarise", "summarising"), ("summarize", "summarizing"), ("translate", "translating"), ("archive", "archiving"),
];
/// Verbs that take "some" before a bare plural ("making some book club inquiries").
const SOME_VERBS: &[&str] = &["make", "buy", "send", "write", "order", "print", "book"];
/// Object words that never take "the" ("reading about…", "drinking water", "replying to…").
const NO_ARTICLE: &[&str] = &["to", "for", "with", "on", "in", "at", "up", "out", "off", "about", "into", "over", "back", "down", "through", "from", "of", "and", "or", "it", "them", "him", "me", "us", "water", "lunch", "dinner", "breakfast", "homework", "coffee", "tea", "food", "money", "time", "exercise", "cardio", "yoga", "piano", "guitar", "some", "every", "each", "two", "three", "four", "five", "ten", "next", "last"];
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
    let noun = || Phrase::Noun(mid_title(t));
    let rest: Vec<&str> = it.collect();
    let key = first.to_lowercase();
    if let Some(n) = crate::taskbrain::outing_noun(t) {
        return Phrase::Noun(n);
    }
    let Some((_, ger)) = VERBS.iter().find(|(v, _)| *v == key) else { return noun() };
    // "Book club", "Order of service" and the like are nouns
    if let Some(n) = rest.first().map(|w| w.to_lowercase()) {
        if (key == "book" && matches!(n.as_str(), "club" | "shelf" | "launch")) || (key == "order" && n == "of") || (key == "sign" && n == "off") {
            return noun();
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
        let plain = !DETERMINERS.contains(&first_obj.as_str()) && !first_obj.chars().any(|c| c.is_ascii_digit()) && rest[0].chars().next().is_some_and(|c| c.is_lowercase());
        if SOME_VERBS.contains(&key.as_str()) && plural && plain {
            out.push_str("some ");
        } else if plain && !plural && rest.len() >= 2 && !NO_ARTICLE.contains(&first_obj.as_str()) && !first_obj.contains('\'') && !first_obj.ends_with("ing") {
            // "submit weekly report" reads as "submitting the weekly report"
            out.push_str("the ");
        }
        out.push_str(&rest.join(" "));
    }
    Phrase::Gerund(out)
}

/// A plain title as it reads mid-sentence: sentence-case titles ("Coffee with Alex", "Team stand-up") lose their
/// leading capital; names, possessives, acronyms, single words and Title Case ("Powerhouse Museum") keep it.
pub fn mid_title(t: &str) -> String {
    let words: Vec<&str> = t.split_whitespace().collect();
    let first = words.first().copied().unwrap_or("");
    // a single common word ("Dentist", "Laundry") is a thing, not a name
    let common_single = words.len() == 1 && crate::taskbrain::typical_for(first).is_some();
    let sentence_case = common_single || (words.len() >= 2 && words[1..].iter().any(|w| w.chars().next().is_some_and(|c| c.is_lowercase())));
    let lowerable = first.chars().skip(1).all(|c| c.is_lowercase() || c == '-') && !first.contains('\'') && first != "I";
    if sentence_case && lowerable {
        let mut c = t.chars();
        let low = c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default();
        with_article(&low)
    } else {
        t.to_string()
    }
}

/// People one goes to see ("dentist" reads as "the dentist") and countable events ("dentist check-up" as "a dentist
/// check-up"), when the title has no "a", "the" or "my" of its own.
fn with_article(low: &str) -> String {
    const PEOPLE: &[&str] = &["dentist", "doctor", "gp", "physio", "vet", "hairdresser", "barber", "optometrist", "optician"];
    const EVENTS: &[&str] = &["check-up", "checkup", "appointment", "haircut", "visit", "session", "interview", "meeting", "stand-up", "standup", "check-in", "lesson", "class", "catch-up", "massage", "review", "call", "walk", "swim", "run", "hike"];
    let words: Vec<&str> = low.split_whitespace().collect();
    let Some(first) = words.first() else { return low.to_string() };
    if DETERMINERS.contains(first) || first.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return low.to_string();
    }
    if words.len() == 1 && PEOPLE.contains(first) {
        return format!("the {low}");
    }
    // the head noun: the last word before any "with / at / for / about …" tail
    let head = words.iter().take_while(|w| !["with", "at", "for", "about", "on", "in", "to", "and"].contains(w)).last().copied().unwrap_or("");
    if EVENTS.contains(&head) {
        let an = first.starts_with(['a', 'e', 'i', 'o', 'u']);
        return format!("{} {low}", if an { "an" } else { "a" });
    }
    low.to_string()
}

/// The task as it reads mid-sentence, with its time ("calling mum at 3 PM").
pub fn inline_phrase(o: &Occurrence) -> String {
    let p = task_phrase(&o.title).text().to_string();
    match &o.time {
        Some(t) => format!("{p} at {}", say_time(t)),
        None => p,
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

/// A checklist step as it reads mid-sentence ("gather last quarter's numbers" -> "gathering last quarter's numbers").
pub fn step_phrase(step: &str) -> String {
    let t = step.trim().trim_end_matches('.');
    match task_phrase(t) {
        Phrase::Gerund(g) => g,
        Phrase::Noun(n) => crate::util::lower_first_word(&n),
    }
}
