//! Miranda's one understanding of what a task is. A task title is matched against a single table of everyday
//! concepts, and everything else follows from that one match: the category it is filed under, how long it usually
//! takes (a haircut is half an hour or more plus getting there, not ten minutes), whether it means going somewhere,
//! and how to say it ("go to the doctor" is "a doctor's appointment"). The person's own correction (a duration set on
//! the task, or learned from an earlier task with the same name) always wins over the table.

use crate::tasks::Occurrence;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typical {
    /// rough range in minutes, the person's own time including the wait (not travel)
    pub min: u32,
    pub max: u32,
    /// it means going somewhere (travel on top of the time above)
    pub outing: bool,
    /// a spoken subject for a sentence ("a haircut", "a GP visit")
    pub what: &'static str,
    /// the person said how long it takes (not a guess from the table)
    pub set_by_user: bool,
}

pub struct Concept {
    /// matches when ALL keys appear as words (or word starts, for keys of 5+ letters) in the title
    pub keys: &'static [&'static str],
    /// the category it belongs to, when the concept decides it (None: the general keyword lists decide)
    pub cat: Option<&'static str>,
    pub min: u32,
    pub max: u32,
    pub outing: bool,
    pub what: &'static str,
    /// how to say a bare "go to / see / get X" ("a doctor's appointment"); None: no special noun
    pub say: Option<&'static str>,
}

const fn c(keys: &'static [&'static str], cat: Option<&'static str>, min: u32, max: u32, outing: bool, what: &'static str, say: Option<&'static str>) -> Concept {
    Concept { keys, cat, min, max, outing, what, say }
}

const APPT: Option<&str> = Some("Appointments");
const ERR: Option<&str> = Some("Errands");
const EX: Option<&str> = Some("Exercise");
const CHORE: Option<&str> = Some("Chores");
const ADMIN: Option<&str> = Some("Admin & Money");
const SOCIAL: Option<&str> = Some("Social");
const STUDY: Option<&str> = Some("Study");
const WORK: Option<&str> = Some("Work");
const CARE: Option<&str> = Some("Self-care");

/// Checked in order: more specific concepts first.
pub const CONCEPTS: &[Concept] = &[
    // asking, booking or calling about something is quick, whatever the something is
    c(&["inquir"], ADMIN, 10, 20, false, "making inquiries", None),
    c(&["enquir"], ADMIN, 10, 20, false, "making enquiries", None),
    c(&["book"], None, 5, 10, false, "booking it", None),
    c(&["call", "bank"], ADMIN, 15, 40, false, "a call like that", None),
    c(&["call", "insurance"], ADMIN, 15, 40, false, "a call like that", None),
    c(&["call", "council"], ADMIN, 15, 40, false, "a call like that", None),
    c(&["call", "ato"], ADMIN, 15, 40, false, "a call like that", None),
    c(&["call", "centrelink"], ADMIN, 20, 60, false, "a call like that", None),
    c(&["call"], None, 5, 20, false, "a call", None),
    c(&["ring"], None, 5, 20, false, "a call", None),
    c(&["email"], None, 5, 15, false, "an email", None),
    c(&["reply"], None, 5, 15, false, "a reply", None),
    c(&["text"], None, 2, 5, false, "a quick message", None),
    c(&["message"], None, 2, 5, false, "a quick message", None),
    c(&["pay"], ADMIN, 5, 10, false, "paying it online", None),
    c(&["make", "dinner"], CHORE, 45, 75, false, "making dinner", None),
    c(&["make", "lunch"], CHORE, 15, 30, false, "making lunch", None),
    // personal care and appointments: time in the chair or the room, plus the wait
    c(&["haircut"], APPT, 30, 45, true, "a haircut", Some("a haircut")),
    c(&["hairdresser"], APPT, 30, 60, true, "a hairdresser visit", Some("a haircut")),
    c(&["barber"], APPT, 20, 40, true, "a barber visit", Some("a haircut")),
    c(&["hair"], APPT, 30, 60, true, "a hair appointment", Some("a haircut")),
    c(&["nails"], APPT, 45, 60, true, "a nail appointment", Some("a nail appointment")),
    c(&["massage"], CARE, 60, 75, true, "a massage", Some("a massage")),
    c(&["facial"], CARE, 60, 75, true, "a facial", Some("a facial")),
    c(&["dentist"], APPT, 45, 60, true, "a dentist visit", Some("a dentist appointment")),
    c(&["dental"], APPT, 45, 60, true, "a dental appointment", Some("a dentist appointment")),
    c(&["doctor"], APPT, 30, 60, true, "a doctor's visit, with the wait,", Some("a doctor's appointment")),
    c(&["gp"], APPT, 30, 60, true, "a GP visit, with the wait,", Some("a doctor's appointment")),
    c(&["physio"], APPT, 45, 60, true, "a physio session", Some("a physio appointment")),
    c(&["optometrist"], APPT, 30, 45, true, "an eye test", Some("an eye appointment")),
    c(&["optician"], APPT, 30, 45, true, "an eye test", Some("an eye appointment")),
    c(&["eye", "test"], APPT, 30, 45, true, "an eye test", None),
    c(&["blood", "test"], APPT, 20, 45, true, "a blood test, with the wait,", None),
    c(&["specialist"], APPT, 45, 90, true, "a specialist appointment, with the wait,", Some("a specialist appointment")),
    c(&["vet"], APPT, 30, 60, true, "a vet visit", Some("a vet visit")),
    c(&["appointment"], APPT, 30, 60, true, "an appointment like that", None),
    c(&["check-up"], APPT, 30, 60, true, "a check-up", None),
    c(&["checkup"], APPT, 30, 60, true, "a check-up", None),
    c(&["interview"], None, 45, 90, true, "an interview", None),
    c(&["car", "service"], ERR, 120, 240, true, "a car service", None),
    // errands
    c(&["grocery"], ERR, 40, 60, true, "a grocery shop", None),
    c(&["groceries"], ERR, 40, 60, true, "a grocery shop", None),
    c(&["supermarket"], ERR, 40, 60, true, "a supermarket run", Some("a trip to the supermarket")),
    c(&["shopping"], ERR, 45, 90, true, "a shopping trip", None),
    c(&["shops"], ERR, 45, 90, true, "a trip to the shops", Some("a trip to the shops")),
    c(&["post", "office"], ERR, 15, 30, true, "a post office trip", Some("a trip to the post office")),
    c(&["bank"], ADMIN, 20, 40, true, "a bank visit", Some("a trip to the bank")),
    c(&["library"], ERR, 30, 60, true, "a library visit", Some("a trip to the library")),
    c(&["pick", "up"], ERR, 15, 30, true, "a pick-up", None),
    c(&["collect"], ERR, 15, 30, true, "a pick-up", None),
    c(&["drop", "off"], ERR, 15, 30, true, "a drop-off", None),
    c(&["return"], ERR, 15, 30, true, "a return", None),
    c(&["pharmacy"], ERR, 15, 25, true, "a pharmacy run", Some("a trip to the pharmacy")),
    c(&["chemist"], ERR, 15, 25, true, "a chemist run", Some("a trip to the chemist")),
    // exercise
    c(&["gym"], EX, 60, 75, true, "a gym session", Some("a gym session")),
    c(&["workout"], EX, 45, 60, false, "a workout", None),
    c(&["swim"], EX, 45, 60, true, "a swim, with changing,", Some("a swim")),
    c(&["walk"], EX, 30, 45, false, "a walk", Some("a walk")),
    c(&["hike"], EX, 120, 240, true, "a hike", Some("a hike")),
    c(&["yoga"], EX, 45, 75, true, "a yoga class", Some("a yoga class")),
    c(&["badminton"], EX, 60, 120, true, "a badminton session", None),
    c(&["basketball"], EX, 60, 120, true, "a basketball game", None),
    // home
    c(&["laundry"], CHORE, 10, 20, false, "the hands-on part of the laundry (the machine does the rest)", None),
    c(&["washing"], CHORE, 10, 20, false, "the hands-on part of the washing (the machine does the rest)", None),
    c(&["vacuum"], CHORE, 20, 30, false, "a vacuum", None),
    c(&["clean", "bathroom"], CHORE, 30, 45, false, "a bathroom clean", None),
    c(&["clean", "kitchen"], CHORE, 20, 40, false, "a kitchen clean", None),
    c(&["clean"], CHORE, 30, 60, false, "a proper clean", None),
    c(&["tidy"], CHORE, 20, 40, false, "a tidy-up", None),
    c(&["declutter"], CHORE, 45, 90, false, "a declutter", None),
    c(&["meal", "prep"], CHORE, 60, 120, false, "meal prep", None),
    c(&["cook"], CHORE, 45, 75, false, "cooking", None),
    c(&["bake"], CHORE, 60, 120, false, "baking", None),
    c(&["water", "plants"], CHORE, 5, 15, false, "watering the plants", None),
    // work and study
    c(&["report"], WORK, 60, 120, false, "a report", None),
    c(&["essay"], STUDY, 90, 180, false, "an essay", None),
    c(&["tax"], ADMIN, 60, 120, false, "the tax return", None),
    c(&["study"], STUDY, 45, 90, false, "a study session", None),
    c(&["revise"], STUDY, 45, 90, false, "a revision session", None),
    c(&["practice"], None, 30, 60, false, "a practice session", None),
    c(&["practise"], None, 30, 60, false, "a practice session", None),
    c(&["read"], None, 30, 60, false, "a good reading session", None),
    c(&["plan"], None, 5, 15, false, "a quick plan", None),
    // social and leisure
    c(&["coffee"], SOCIAL, 45, 90, true, "a coffee catch-up", None),
    c(&["lunch"], SOCIAL, 60, 90, true, "a lunch", None),
    c(&["dinner"], SOCIAL, 90, 150, true, "a dinner", None),
    c(&["drinks"], SOCIAL, 90, 150, true, "drinks", None),
    c(&["catch", "up"], SOCIAL, 60, 120, true, "a catch-up", None),
    c(&["movie"], None, 120, 150, true, "a movie", None),
    c(&["film"], None, 120, 150, true, "a film", None),
    c(&["museum"], None, 90, 180, true, "a museum visit", None),
    c(&["gallery"], None, 60, 120, true, "a gallery visit", None),
    c(&["club"], SOCIAL, 90, 150, true, "a club meet-up", None),
];

/// Verbs that only count as their own concept at the very start of a title ("Call Mum", not "Mum's call").
const LEAD_VERBS: &[&str] = &["call", "ring", "email", "reply", "text", "message", "pay", "book"];
/// Titles that start with one of these are ABOUT a thing rather than the thing itself ("research gyms", "cancel the
/// dentist", "check gym prices"), so only that verb's own concept can match.
const ABOUT_VERBS: &[&str] = &["book", "call", "ring", "email", "text", "message", "check", "research", "find", "cancel", "reschedule", "organise", "organize", "arrange", "confirm", "look", "ask", "compare", "review", "renew"];

fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '-')).filter(|w| !w.is_empty()).map(String::from).collect()
}

fn word_is(x: &str, k: &str) -> bool {
    x == k || (k.len() >= 5 && x.starts_with(k)) || (x.len() > 4 && x.trim_end_matches('s') == k)
}

/// The one concept a title matches, if it is a common, recognisable thing.
pub fn concept_for(title: &str) -> Option<&'static Concept> {
    let w = words(title);
    let first = w.first().map(String::as_str).unwrap_or("");
    let has = |k: &str| w.iter().any(|x| word_is(x, k));
    // "book club" is a club, not a booking
    let book_club = first == "book" && w.get(1).is_some_and(|n| n == "club");
    let about = ABOUT_VERBS.contains(&first) && !book_club;
    CONCEPTS.iter().find(|c| {
        if LEAD_VERBS.contains(&c.keys[0]) && (first != c.keys[0] || (c.keys[0] == "book" && book_club)) {
            return false;
        }
        if about && c.keys[0] != first {
            return false;
        }
        c.keys.iter().all(|k| has(k))
    })
}

/// The typical duration for a title (the table's guess).
pub fn typical_for(title: &str) -> Option<Typical> {
    concept_for(title).map(|c| Typical { min: c.min, max: c.max, outing: c.outing, what: c.what, set_by_user: false })
}

/// The category a title belongs to, when its concept decides it.
pub fn category_for(title: &str) -> Option<&'static str> {
    concept_for(title).and_then(|c| c.cat)
}

/// How long a task takes: what the person said (on the task), else the table's guess.
pub fn typical(o: &Occurrence) -> Option<Typical> {
    let guess = typical_for(&o.title);
    match o.minutes {
        Some(m) if m > 0 => Some(Typical { min: m, max: m, outing: guess.is_some_and(|g| g.outing), what: guess.map(|g| g.what).unwrap_or("that one"), set_by_user: true }),
        _ => guess,
    }
}

/// "Go to the doctor", "see dentist", "get a haircut", "go gym" as the thing they are ("a doctor's appointment"),
/// when the title is just a going-verb plus one known place or thing.
pub fn outing_noun(title: &str) -> Option<String> {
    let w: Vec<String> = title.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '-')).filter(|x| !x.is_empty()).map(String::from).collect();
    let verb = w.first()?.as_str();
    if !["go", "see", "visit", "get", "attend", "have", "head", "pop"].contains(&verb) {
        return None;
    }
    let rest: Vec<&str> = w[1..].iter().map(String::as_str).filter(|x| !["to", "the", "a", "an", "my", "into", "down", "over", "off", "in", "at", "s"].contains(x)).collect();
    if rest.is_empty() {
        return None;
    }
    let c = concept_for(&rest.join(" "))?;
    let say = c.say?;
    // nothing more specific than the place itself ("go to the doctor about my knee" stays as it is)
    let plain = rest.iter().all(|x| c.keys.iter().any(|k| word_is(x, k)) || ["appointment", "appt", "visit", "session", "trip", "class"].contains(x));
    plain.then(|| say.to_string())
}

fn mins(n: u32) -> String {
    match n {
        60 => "an hour".into(),
        90 => "an hour and a half".into(),
        120 => "two hours".into(),
        150 => "two and a half hours".into(),
        180 => "three hours".into(),
        240 => "four hours".into(),
        30 => "half an hour".into(),
        n if n >= 60 && n % 60 == 0 => format!("{} hours", n / 60),
        n => format!("{n} minutes"),
    }
}

/// "about 30 to 45 minutes", "about an hour to an hour and a half", "five or ten minutes".
pub fn range_text(t: &Typical) -> String {
    match (t.min, t.max) {
        (a, b) if b <= 10 => format!("{} or {} minutes", spoken_small(a), spoken_small(b)),
        (a, b) if b <= 20 => format!("about {} to {} minutes", spoken_small(a), spoken_num(b)),
        (a, b) if a < 60 && b < 60 => format!("about {a} to {b} minutes"),
        (60, 120) => "an hour or two".into(),
        (120, 240) => "a few hours".into(),
        (a, b) if a < 60 && b == 60 => format!("about {a} minutes to an hour"),
        (a, b) => format!("about {} to {}", mins(a), mins(b)),
    }
}

fn spoken_num(n: u32) -> String {
    match n {
        15 => "fifteen".into(),
        20 => "twenty".into(),
        n => spoken_small(n),
    }
}

fn spoken_small(n: u32) -> String {
    ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"].get(n as usize).map(|s| s.to_string()).unwrap_or_else(|| n.to_string())
}


/// "about 45 minutes" for an exact figure, else the range.
pub fn length_text(t: &Typical) -> String {
    if t.min == t.max {
        return match t.min {
            m if m <= 10 => format!("about {} minutes", spoken_small(m)),
            m => format!("about {}", mins(m)),
        };
    }
    range_text(t)
}

/// The context line for the AI: "usually takes about 30 to 45 minutes, plus getting there and back".
pub fn context_hint(t: &Typical) -> String {
    let travel = if t.outing { ", plus getting there and back" } else { "" };
    if t.set_by_user { format!("takes {}{travel} (the person said so)", length_text(t)) } else { format!("usually takes {}{travel}", length_text(t)) }
}

/// One plain sentence about the task that needs proper time: how long, and where it fits.
pub fn fit_sentence(o: &Occurrence, work_day: bool) -> String {
    let name = crate::util::cap(&crate::taskvoice::inline_phrase(o));
    let Some(t) = typical(o) else {
        return format!("{name} is the one that needs proper time, so give it a real slot{}.", if work_day { " before or after work" } else { "" });
    };
    let travel = if t.outing { ", plus getting there" } else { "" };
    let fit = match (t.outing, o.part_of_day.as_str(), work_day) {
        (_, "morning", true) => "so it suits the time before work",
        (_, "morning", false) => "so the morning suits it",
        (_, "afternoon", false) => "so the afternoon suits it",
        (_, "evening", _) => "so this evening suits it",
        (true, _, true) => "so it's one for before or after work rather than a lunch break",
        (true, _, false) => "so do it when you're out and about anyway",
        (false, _, true) => "so give it a real slot before or after work",
        (false, _, false) => "so give it a real slot",
    };
    format!("{name} is the long one, {}{travel}, {fit}.", length_text(&t))
}
