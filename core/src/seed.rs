//! Fresh-start state and the optional demo data.
use crate::env::Env;
use crate::model::*;
use crate::util::fmt_date;
use chrono::Duration;
use serde_json::{json, Value};

pub const CATEGORY_DEFAULTS: [(&str, &str); 10] = [
    ("Chores", "#C9A26B"),
    ("Exercise", "#9CB89A"),
    ("Study", "#8FA9D6"),
    ("Work", "#7FA7A3"),
    ("Errands", "#D39A7E"),
    ("Appointments", "#C98BB0"),
    ("Social", "#E0B36A"),
    ("Self-care", "#B8A6E3"),
    ("Admin & Money", "#A3A38A"),
    ("Other", "#8C847D"),
];

/// One exercise of a default template.
struct Def {
    name: &'static str,
    start: Option<f64>,
    note: &'static str,
    assisted: bool,
}

fn ex(name: &'static str, start: Option<f64>) -> Def {
    Def { name, start, note: "", assisted: false }
}

impl Def {
    fn note(mut self, n: &'static str) -> Def {
        self.note = n;
        self
    }
    fn assisted(mut self) -> Def {
        self.assisted = true;
        self
    }
}

/// Every strength exercise is 4 sets of 10.
const SETS: i64 = 4;
const REPS: i64 = 10;

const CARDIO_OPTIONS: [&str; 4] = ["Swimming", "Rowing machine", "Stair machine", "Intense walking/hiking"];
const OLD_DEFAULT_NAMES: [&str; 6] = ["Push A", "Push B", "Pull A", "Pull B", "Legs A", "Legs B"];
const ROTATION: [&str; 8] = ["Push 1", "Pull 1", "Legs + Abs 1", "Cardio", "Push 2", "Pull 2", "Legs + Abs 2", "Cardio"];

fn strength_templates() -> Vec<(&'static str, &'static str, Vec<Def>)> {
    vec![
        ("Push 1", "Chest + Triceps + Front Deltoid", vec![
            ex("Incline dumbbell chest press", Some(15.0)),
            ex("Flat barbell chest press", Some(5.0)),
            ex("Counterweighted dip", Some(37.5)).assisted(),
            ex("Pectoral fly", Some(40.0)),
            ex("Seated dumbbell shoulder press", Some(17.5)),
            ex("Dumbbell shoulder raises", Some(5.0)),
            ex("Tricep cable pushdown (two-handed)", Some(12.5)).note("Bar or rope"),
        ]),
        ("Push 2", "", vec![
            ex("Incline dumbbell chest press", Some(15.0)),
            ex("Flat machine chest press", Some(10.0)),
            ex("High-to-low cable crossover", None),
            ex("Incline seated dumbbell pectoral fly", Some(5.0)).note("Or cable fly / cable crossover"),
            ex("Machine shoulder press", Some(15.0)),
            ex("Machine shoulder raises", Some(20.0)),
            ex("Cable shoulder raises", Some(5.0)),
            ex("Post-cranial seated tricep dumbbell lift", Some(15.0)),
        ]),
        ("Pull 1", "Back + Biceps + Rear Deltoid", vec![
            ex("Counterweighted pull-up, wide grip", Some(68.0)).assisted(),
            ex("Seated cable pulldown, normal width", Some(35.0)),
            ex("Seated cable machine row", Some(17.5)),
            ex("Seated rear deltoid", Some(40.0)),
            ex("Face pulls", Some(25.0)),
            ex("Two-handed cable bicep curl", Some(15.0)),
        ]),
        ("Pull 2", "Other options: T-bar row", vec![
            ex("Counterweighted pull-up, narrow grip", Some(61.0)).assisted(),
            ex("Seated cable pulldown, wide or narrow grip", None),
            ex("Seated cable machine row", Some(17.5)),
            ex("Lying incline dumbbell rear deltoid", None),
            ex("Incline standing rear deltoid fly machine", Some(33.0)),
            ex("Dumbbell bicep curl", Some(7.0)).note("Or barbell bicep curl, 17.5 kg"),
            ex("Forward-facing dumbbell lift", Some(6.0)),
        ]),
        ("Legs + Abs 1", "", vec![
            ex("Incline leg press", Some(50.0)),
            ex("Leg curl", Some(40.0)),
            ex("Leg extension", Some(33.0)),
            ex("Glute hip bridge", Some(10.0)),
            ex("Calf press", Some(40.0)),
            ex("Weighted ab curls", Some(40.0)),
            ex("Knee lifts", None),
        ]),
        ("Legs + Abs 2", "", vec![
            ex("Flat leg press", Some(50.0)).note("Or goblet squat"),
            ex("Leg curl", Some(40.0)),
            ex("Leg extension", Some(33.0)),
            ex("Romanian dumbbell deadlift", Some(10.0)),
            ex("Hip adduction (inwards)", Some(61.0)),
            ex("Hip abduction (outwards)", Some(75.0)),
            ex("Weighted ab curls", Some(40.0)),
            ex("Knee lifts", None),
        ]),
    ]
}

/// The owner's routine: exercises, templates (shared exercises by name) and the 9-item rotation (8 workouts + rest).
/// Ids come from `next`.
pub fn default_gym(mut next: impl FnMut() -> i64) -> Gym {
    let mut gym = Gym::default();
    let mut templates: Vec<(&str, &str, Vec<Def>)> = strength_templates();
    // the cardio template goes in the 4th slot of the rotation, but templates list order is free
    for (name, note, list) in templates.drain(..) {
        let mut exercises = vec![];
        for d in list {
            let id = match gym.exercises.iter_mut().find(|e| e.name == d.name) {
                Some(e) => {
                    e.assisted |= d.assisted;
                    e.id
                }
                None => {
                    let id = next();
                    gym.exercises.push(Exercise { id, name: d.name.to_string(), assisted: d.assisted });
                    id
                }
            };
            exercises.push(TemplateExercise { exercise_id: id, name: d.name.to_string(), sets: SETS, rep_min: REPS, rep_max: REPS, start_weight: d.start, note: d.note.to_string() });
        }
        gym.templates.push(Template { id: next(), name: name.to_string(), kind: "strength".into(), note: note.to_string(), exercises, options: vec![], minutes_min: None, minutes_max: None });
    }
    gym.templates.push(Template {
        id: next(),
        name: "Cardio".into(),
        kind: "cardio".into(),
        note: "Do one of the following for 20\u{2013}40 mins".into(),
        exercises: vec![],
        options: CARDIO_OPTIONS.iter().map(|s| s.to_string()).collect(),
        minutes_min: Some(20),
        minutes_max: Some(40),
    });
    for n in ROTATION {
        let tid = gym.templates.iter().find(|t| t.name == n).map(|t| t.id);
        gym.rotation.items.push(RotItem { template_id: tid, rest: false });
    }
    // a full rest day closes the cycle: Legs + Abs 2 -> light Cardio -> Rest -> Push 1
    gym.rotation.items.push(RotItem { template_id: None, rest: true });
    gym.rotation.next_index = 0;
    gym
}

/// A real fresh start: defaults only (no tasks, goals or sessions).
pub fn init_state(_today: &str) -> State {
    let mut st = State::default();
    for (i, (n, c)) in CATEGORY_DEFAULTS.iter().enumerate() {
        st.categories.push(Category { id: i as i64 + 1, name: n.to_string(), color: c.to_string() });
    }
    let mut n = 0i64;
    st.gym = default_gym(|| {
        n += 1;
        n
    });
    st.next_id = st.next_id.max(n + 1);
    st
}

/// Bring an older State up to `STATE_VERSION`. Returns true when something changed.
/// v1 -> v2: a gym section still holding exactly the six old generic templates (never customised)
/// is replaced by the owner's routine; anything else only gains the new fields (serde defaults).
pub fn migrate(st: &mut State) -> bool {
    if st.version >= STATE_VERSION {
        return false;
    }
    if st.version < 2 {
        migrate_v1_gym(st);
    }
    // v2 -> v3: the untouched 8-workout default rotation gains a rest day at the end of the cycle
    let names: Vec<Option<String>> = st
        .gym
        .rotation
        .items
        .iter()
        .map(|it| if it.rest { None } else { it.template_id.and_then(|id| st.gym.templates.iter().find(|t| t.id == id)).map(|t| t.name.clone()) })
        .collect();
    let default: Vec<Option<String>> = ROTATION.iter().map(|n| Some(n.to_string())).collect();
    if names == default {
        st.gym.rotation.items.push(RotItem { template_id: None, rest: true });
    }
    st.version = STATE_VERSION;
    true
}

fn migrate_v1_gym(st: &mut State) {
    let mut names: Vec<&str> = st.gym.templates.iter().map(|t| t.name.as_str()).collect();
    names.sort_unstable();
    let mut old: Vec<&str> = OLD_DEFAULT_NAMES.to_vec();
    old.sort_unstable();
    if names == old {
        st.next_id = st.next_id.max(st.max_id() + 1);
        let old_ids: Vec<i64> = st.gym.templates.iter().map(|t| t.id).collect();
        let all_old = st.gym.sessions.iter().all(|s| s.template_id.is_some_and(|t| old_ids.contains(&t)));
        let kept_sessions = if all_old { vec![] } else { std::mem::take(&mut st.gym.sessions) };
        let kept_ex: Vec<Exercise> = if all_old {
            vec![]
        } else {
            st.gym.exercises.iter().filter(|e| kept_sessions.iter().any(|s| s.sets.iter().any(|x| x.exercise_id == e.id))).cloned().collect()
        };
        let mut next = st.next_id;
        let mut gym = default_gym(|| {
            next += 1;
            next - 1
        });
        st.next_id = next;
        // keep history for exercises that sessions still reference (merged by name where possible)
        let mut sessions = kept_sessions;
        for e in kept_ex {
            if let Some(new) = gym.exercises.iter().find(|n| n.name.eq_ignore_ascii_case(&e.name)) {
                let nid = new.id;
                for s in sessions.iter_mut() {
                    for x in s.sets.iter_mut().filter(|x| x.exercise_id == e.id) {
                        x.exercise_id = nid;
                    }
                }
            } else {
                gym.exercises.push(e);
            }
        }
        gym.sessions = sessions;
        st.gym = gym;
    }
}

struct T<'a> {
    title: &'a str,
    day: Option<i64>, // offset from today for one-offs
    time: Option<&'a str>,
    part: Option<&'a str>,
    cat: &'a str,
    notes: &'a str,
    rec: Option<Recurrence>,
    steps: &'a [&'a str],
}

type Log = (&'static str, f64, [i64; 4]);

/// Load the old seed's demo tasks, goals and sessions, relative to `today`. Skips what is already there.
pub fn sample_data(env: &mut Env) -> Value {
    let today = env.today;
    let day = |off: i64| fmt_date(today + Duration::days(off));
    let tasks = [
        T { title: "Morning routine", day: None, time: Some("07:30"), part: None, cat: "Self-care", notes: "", rec: Some(Recurrence::Daily), steps: &["Make bed", "Drink water", "Stretch"] },
        T { title: "Plan tomorrow", day: None, time: None, part: Some("evening"), cat: "Work", notes: "Five quiet minutes to line up the next day.", rec: Some(Recurrence::Weekdays), steps: &[] },
        T { title: "Laundry", day: None, time: None, part: Some("afternoon"), cat: "Chores", notes: "", rec: Some(Recurrence::Weekly { days: vec![6] }), steps: &["Wash", "Dry", "Fold and put away"] },
        T { title: "Pay electricity bill", day: Some(-1), time: None, part: Some("anytime"), cat: "Admin & Money", notes: "Yesterday's leftover, it rolls over until done.", rec: None, steps: &[] },
        T { title: "Reply to Sam's email", day: Some(0), time: None, part: Some("morning"), cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Team stand-up", day: Some(0), time: Some("10:15"), part: None, cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Call Mum", day: Some(0), time: Some("18:30"), part: None, cat: "Social", notes: "", rec: None, steps: &[] },
        T { title: "Book dentist appointment", day: Some(0), time: None, part: Some("anytime"), cat: "Appointments", notes: "", rec: None, steps: &[] },
        T { title: "Grocery shopping", day: Some(1), time: None, part: Some("evening"), cat: "Errands", notes: "Eggs, oats, spinach, bananas.", rec: None, steps: &[] },
        T { title: "Project check-in", day: Some(1), time: Some("10:30"), part: None, cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Coffee with Alex", day: Some(2), time: Some("18:00"), part: None, cat: "Social", notes: "", rec: None, steps: &[] },
        T { title: "Submit weekly report", day: Some(3), time: None, part: Some("morning"), cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Fix the leaky tap", day: Some(4), time: None, part: Some("anytime"), cat: "Chores", notes: "", rec: None, steps: &[] },
        T { title: "Evening walk", day: Some(4), time: None, part: Some("evening"), cat: "Exercise", notes: "", rec: None, steps: &[] },
    ];
    let mut added_tasks = 0;
    for t in &tasks {
        let date = t.day.map(day);
        if env.st.tasks.iter().any(|x| x.title == t.title && x.date == date && x.recurrence == t.rec) {
            continue;
        }
        let cat = env.st.categories.iter().find(|c| c.name == t.cat).map(|c| c.id);
        let id = env.st.new_id();
        let checklist = t.steps.iter().map(|s| ChecklistItem { id: env.st.new_id(), text: s.to_string() }).collect();
        env.st.tasks.push(Task {
            id,
            title: t.title.into(),
            date,
            time: t.time.map(String::from),
            part_of_day: match t.time {
                Some(tm) => (if tm < "12:00" { "morning" } else if tm < "17:00" { "afternoon" } else { "evening" }).to_string(),
                None => t.part.unwrap_or("anytime").to_string(),
            },
            category_id: cat,
            category_auto: true,
            notes: t.notes.into(),
            recurrence: t.rec.clone(),
            checklist,
        });
        added_tasks += 1;
    }

    // Push 1 (-8d), Pull 1 (-6d), Legs + Abs 1 (-4d) and Cardio (-3d) done => today is Push 2.
    // Push 1 hits every rep on the incline press at 15 kg (hint: 17.5); the assisted pull-up hits every rep at 68 (hint: 65.5).
    let sessions: [(&str, i64, Vec<Log>, Option<(&str, i64)>); 4] = [
        ("Push 1", -8, vec![
            ("Incline dumbbell chest press", 15.0, [10, 10, 10, 10]),
            ("Flat barbell chest press", 5.0, [10, 10, 9, 8]),
            ("Counterweighted dip", 37.5, [10, 10, 9, 8]),
            ("Pectoral fly", 40.0, [10, 10, 10, 9]),
            ("Seated dumbbell shoulder press", 17.5, [10, 9, 9, 8]),
            ("Dumbbell shoulder raises", 5.0, [10, 10, 10, 10]),
            ("Tricep cable pushdown (two-handed)", 12.5, [10, 10, 10, 9]),
        ], None),
        ("Pull 1", -6, vec![
            ("Counterweighted pull-up, wide grip", 68.0, [10, 10, 10, 10]),
            ("Seated cable pulldown, normal width", 35.0, [10, 10, 9, 9]),
            ("Seated cable machine row", 17.5, [10, 10, 10, 10]),
            ("Seated rear deltoid", 40.0, [10, 9, 9, 8]),
            ("Face pulls", 25.0, [10, 10, 10, 10]),
            ("Two-handed cable bicep curl", 15.0, [10, 10, 9, 8]),
        ], None),
        ("Legs + Abs 1", -4, vec![
            ("Incline leg press", 50.0, [10, 10, 10, 10]),
            ("Leg curl", 40.0, [10, 10, 9, 9]),
            ("Leg extension", 33.0, [10, 10, 10, 9]),
            ("Glute hip bridge", 10.0, [10, 10, 10, 10]),
            ("Calf press", 40.0, [10, 10, 10, 8]),
            ("Weighted ab curls", 40.0, [10, 10, 9, 9]),
            ("Knee lifts", 0.0, [10, 10, 10, 10]),
        ], None),
        ("Cardio", -3, vec![], Some(("Rowing machine", 30))),
    ];
    let mut added_sessions = 0;
    for (tpl, off, logs, cardio) in &sessions {
        let Some(tid) = env.st.gym.templates.iter().find(|t| t.name == *tpl).map(|t| t.id) else { continue };
        let date = day(*off);
        if env.st.gym.sessions.iter().any(|s| s.template_id == Some(tid) && s.date == date) {
            continue;
        }
        let mut sets = vec![];
        for (en, w, reps) in logs {
            let eid = match env.st.gym.exercises.iter().find(|e| e.name == *en) {
                Some(e) => e.id,
                None => {
                    let id = env.st.new_id();
                    env.st.gym.exercises.push(Exercise { id, name: en.to_string(), assisted: false });
                    id
                }
            };
            for (i, r) in reps.iter().enumerate() {
                sets.push(SetRec { exercise_id: eid, set_no: i as i64 + 1, weight: *w, reps: *r, done: true });
            }
        }
        let id = env.st.new_id();
        let cardio = cardio.map(|(a, m)| Cardio { activity: a.to_string(), minutes: m });
        env.st.gym.sessions.push(Session { id, template_id: Some(tid), name: tpl.to_string(), date, completed: true, sets, cardio });
        added_sessions += 1;
    }
    if added_sessions > 0 {
        // the default rotation is Push 1, Pull 1, Legs + Abs 1, Cardio, Push 2, ...: next is Push 2
        if let Some(i) = env.st.gym.rotation.items.iter().position(|it| it.template_id == env.st.gym.templates.iter().find(|t| t.name == "Push 2").map(|t| t.id)) {
            env.st.gym.rotation.next_index = i;
        }
        env.st.gym.rest_taken_date = None;
    }

    let goals: [(&str, &str, Option<i64>, &str); 3] = [
        ("Get stronger: bench 80 kg by spring", "medium", Some(120), "Bench has sat around 60 kg for a while and I want to feel properly strong again."),
        ("Stay on top of work admin", "medium", Some(75), "Replies and weekly reports done on time, so Fridays feel light instead of a scramble."),
        ("Build a calmer, more organised routine", "long", None, "Fewer loose ends and more headspace for the people I care about."),
    ];
    let mut added_goals = 0;
    for (title, horizon, dl, why) in goals {
        if env.st.goals.iter().any(|g| g.title == title) {
            continue;
        }
        let id = env.st.new_id();
        env.st.goals.push(Goal { id, title: title.into(), horizon: horizon.into(), deadline: dl.map(day), why: why.into(), status: "active".into(), achieved_on: None, created: day(0) });
        added_goals += 1;
    }
    env.changed = true;
    json!({"ok": true, "tasks": added_tasks, "goals": added_goals, "sessions": added_sessions})
}
