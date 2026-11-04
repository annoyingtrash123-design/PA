//! Common sense about how long everyday things take. A task title is matched against a small table of typical
//! durations (a haircut is half an hour or more plus getting there, not ten minutes; a GP visit is about an hour with
//! the wait). Used for the spoken timing lines, the big/quick guess, the lunch-break suggestion and the AI's context.

use crate::tasks::Occurrence;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Typical {
    /// rough range in minutes, the person's own time including the wait (not travel)
    pub min: u32,
    pub max: u32,
    /// it means going somewhere (travel on top of the time above)
    pub outing: bool,
    /// a spoken subject for the sentence ("a haircut", "a GP visit")
    pub what: &'static str,
}

const fn t(min: u32, max: u32, outing: bool, what: &'static str) -> Typical {
    Typical { min, max, outing, what }
}

/// (keywords, typical). A rule matches when ALL its keywords appear as words (or word starts, for keywords of 5+
/// letters) in the title. Checked in order: more specific rules first.
const RULES: &[(&[&str], Typical)] = &[
    // booking or calling about something is quick, whatever the something is
    (&["book"], t(5, 10, false, "booking it")),
    (&["call", "bank"], t(15, 40, false, "a call like that")),
    (&["call", "insurance"], t(15, 40, false, "a call like that")),
    (&["call", "council"], t(15, 40, false, "a call like that")),
    (&["call", "ato"], t(15, 40, false, "a call like that")),
    (&["call", "centrelink"], t(20, 60, false, "a call like that")),
    (&["call"], t(5, 20, false, "a call")),
    (&["ring"], t(5, 20, false, "a call")),
    (&["email"], t(5, 15, false, "an email")),
    (&["reply"], t(5, 15, false, "a reply")),
    (&["text"], t(2, 5, false, "a quick message")),
    (&["message"], t(2, 5, false, "a quick message")),
    (&["pay"], t(5, 10, false, "paying it online")),
    // personal care and appointments: time in the chair or the room, plus the wait
    (&["haircut"], t(30, 45, true, "a haircut")),
    (&["hairdresser"], t(30, 60, true, "a hairdresser visit")),
    (&["barber"], t(20, 40, true, "a barber visit")),
    (&["hair"], t(30, 60, true, "a hair appointment")),
    (&["nails"], t(45, 60, true, "a nail appointment")),
    (&["massage"], t(60, 75, true, "a massage")),
    (&["facial"], t(60, 75, true, "a facial")),
    (&["dentist"], t(45, 60, true, "a dentist visit")),
    (&["dental"], t(45, 60, true, "a dental appointment")),
    (&["doctor"], t(30, 60, true, "a doctor's visit, with the wait")),
    (&["gp"], t(30, 60, true, "a GP visit, with the wait")),
    (&["physio"], t(45, 60, true, "a physio session")),
    (&["optometrist"], t(30, 45, true, "an eye test")),
    (&["eye", "test"], t(30, 45, true, "an eye test")),
    (&["blood", "test"], t(20, 45, true, "a blood test, with the wait")),
    (&["specialist"], t(45, 90, true, "a specialist appointment, with the wait")),
    (&["vet"], t(30, 60, true, "a vet visit")),
    (&["appointment"], t(30, 60, true, "an appointment like that")),
    (&["check-up"], t(30, 60, true, "a check-up")),
    (&["checkup"], t(30, 60, true, "a check-up")),
    (&["interview"], t(45, 90, true, "an interview")),
    (&["car", "service"], t(120, 240, true, "a car service")),
    // errands
    (&["grocery"], t(40, 60, true, "a grocery shop")),
    (&["groceries"], t(40, 60, true, "a grocery shop")),
    (&["supermarket"], t(40, 60, true, "a supermarket run")),
    (&["shopping"], t(45, 90, true, "a shopping trip")),
    (&["post", "office"], t(15, 30, true, "a post office trip")),
    (&["bank"], t(20, 40, true, "a bank visit")),
    (&["pick", "up"], t(15, 30, true, "a pick-up")),
    (&["collect"], t(15, 30, true, "a pick-up")),
    (&["drop", "off"], t(15, 30, true, "a drop-off")),
    (&["return"], t(15, 30, true, "a return")),
    (&["pharmacy"], t(15, 25, true, "a pharmacy run")),
    (&["chemist"], t(15, 25, true, "a chemist run")),
    // exercise
    (&["gym"], t(60, 75, true, "a gym session")),
    (&["workout"], t(45, 60, false, "a workout")),
    (&["swim"], t(45, 60, true, "a swim, with changing")),
    (&["walk"], t(30, 45, false, "a walk")),
    (&["hike"], t(120, 240, true, "a hike")),
    (&["yoga"], t(45, 75, true, "a yoga class")),
    (&["badminton"], t(60, 120, true, "a badminton session")),
    (&["basketball"], t(60, 120, true, "a basketball game")),
    // home
    (&["laundry"], t(10, 20, false, "the hands-on part of the laundry (the machine does the rest)")),
    (&["washing"], t(10, 20, false, "the hands-on part of the washing (the machine does the rest)")),
    (&["vacuum"], t(20, 30, false, "a vacuum")),
    (&["clean", "bathroom"], t(30, 45, false, "a bathroom clean")),
    (&["clean", "kitchen"], t(20, 40, false, "a kitchen clean")),
    (&["clean"], t(30, 60, false, "a proper clean")),
    (&["tidy"], t(20, 40, false, "a tidy-up")),
    (&["declutter"], t(45, 90, false, "a declutter")),
    (&["meal", "prep"], t(60, 120, false, "meal prep")),
    (&["cook"], t(45, 75, false, "cooking")),
    (&["bake"], t(60, 120, false, "baking")),
    (&["water", "plants"], t(5, 15, false, "watering the plants")),
    // work and study
    (&["report"], t(60, 120, false, "a report")),
    (&["essay"], t(90, 180, false, "an essay")),
    (&["tax"], t(60, 120, false, "the tax return")),
    (&["study"], t(45, 90, false, "a study session")),
    (&["revise"], t(45, 90, false, "a revision session")),
    (&["practice"], t(30, 60, false, "a practice session")),
    (&["practise"], t(30, 60, false, "a practice session")),
    (&["read"], t(30, 60, false, "a good reading session")),
    (&["plan"], t(5, 15, false, "a quick plan")),
    // social and leisure
    (&["coffee"], t(45, 90, true, "a coffee catch-up")),
    (&["lunch"], t(60, 90, true, "a lunch")),
    (&["dinner"], t(90, 150, true, "a dinner")),
    (&["drinks"], t(90, 150, true, "drinks")),
    (&["catch", "up"], t(60, 120, true, "a catch-up")),
    (&["movie"], t(120, 150, true, "a movie")),
    (&["film"], t(120, 150, true, "a film")),
    (&["museum"], t(90, 180, true, "a museum visit")),
    (&["gallery"], t(60, 120, true, "a gallery visit")),
    (&["club"], t(90, 150, true, "a club meet-up")),
];

fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '-')).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// The typical duration for a task title, if it is a common, recognisable thing.
pub fn typical_for(title: &str) -> Option<Typical> {
    let w = words(title);
    let has = |k: &str| w.iter().any(|x| x == k || (k.len() >= 5 && x.starts_with(k)) || (x.len() > 4 && x.trim_end_matches('s') == k));
    // "book a haircut" is booking, not the haircut; but "book club" is a club
    let first = w.first().map(String::as_str).unwrap_or("");
    RULES
        .iter()
        .filter(|(keys, _)| keys != &&["book"][..] || (first == "book" && w.get(1).is_some_and(|n| n != "club")))
        .find(|(keys, _)| {
            // the quick verbs only count when the title starts with them ("Call Mum", not "Mum's call")
            let quick_verb = ["call", "ring", "email", "reply", "text", "message", "pay"].contains(&keys[0]);
            if quick_verb && first != keys[0] {
                return false;
            }
            keys.iter().all(|k| has(k))
        })
        .map(|(_, t)| *t)
}

pub fn typical(o: &Occurrence) -> Option<Typical> {
    typical_for(&o.title)
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

/// The context line for the AI: "usually about 30 to 45 minutes, plus getting there".
pub fn context_hint(t: &Typical) -> String {
    format!("usually takes {}{}", range_text(t), if t.outing { ", plus getting there and back" } else { "" })
}

/// A spoken timing line for the template ("a haircut usually takes about 30 to 45 minutes, plus getting there, so…").
pub fn timing_line(o: &Occurrence, t: &Typical, work_day: bool) -> String {
    let base = format!("{} usually takes {}{}", t.what, range_text(t), if t.outing { ", plus getting there" } else { "" });
    let fit = if o.time.is_some() {
        if t.outing { "so leave a little buffer either side of it" } else { "so block out the time around it" }
    } else if t.outing && t.max >= 40 {
        match o.part_of_day.as_str() {
            "morning" if work_day => "so it fits best before work if you can get in early",
            "morning" => "so the morning suits it nicely",
            "afternoon" if work_day => "so it'd be best after work rather than squeezed into a break",
            "afternoon" => "so the afternoon suits it nicely",
            "evening" => "so tonight's a good time for it",
            _ if work_day => "so it's one for before or after work, or the weekend, rather than a lunch break",
            _ => "so pick a time when you're out and about anyway",
        }
    } else if t.max <= 20 {
        "so it'll fit in any little gap"
    } else if t.max <= 40 {
        "so a free half hour is all it needs"
    } else {
        "so give it a proper slot when you've got the time"
    };
    format!("{base}, {fit}")
}
