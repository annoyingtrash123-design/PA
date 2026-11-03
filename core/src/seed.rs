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

type Ex = (&'static str, i64, i64, i64);
const TEMPLATES: [(&str, [Ex; 5]); 6] = [
    ("Push A", [("Bench Press", 3, 6, 10), ("Overhead Press", 3, 8, 12), ("Incline DB Press", 3, 8, 12), ("Lateral Raise", 3, 12, 15), ("Tricep Pushdown", 3, 10, 15)]),
    ("Pull A", [("Deadlift", 3, 5, 8), ("Pull-ups", 3, 6, 10), ("Barbell Row", 3, 8, 12), ("Face Pull", 3, 12, 15), ("Bicep Curl", 3, 10, 12)]),
    ("Legs A", [("Squat", 3, 6, 10), ("Romanian Deadlift", 3, 8, 12), ("Leg Press", 3, 10, 12), ("Leg Curl", 3, 10, 15), ("Calf Raise", 3, 12, 15)]),
    ("Push B", [("Incline DB Press", 3, 8, 12), ("Dips", 3, 8, 12), ("Seated DB Shoulder Press", 3, 8, 12), ("Cable Fly", 3, 12, 15), ("Overhead Tricep Extension", 3, 10, 15)]),
    ("Pull B", [("Lat Pulldown", 3, 8, 12), ("Seated Cable Row", 3, 8, 12), ("Chin-ups", 3, 6, 10), ("Rear Delt Fly", 3, 12, 15), ("Hammer Curl", 3, 10, 12)]),
    ("Legs B", [("Front Squat", 3, 6, 10), ("Hip Thrust", 3, 8, 12), ("Walking Lunges", 3, 10, 12), ("Leg Extension", 3, 10, 15), ("Seated Calf Raise", 3, 12, 15)]),
];
const ROTATION: [&str; 8] = ["Push A", "Pull A", "Legs A", "", "Push B", "Pull B", "Legs B", ""];

/// A real fresh start: defaults only (no tasks, goals or sessions).
pub fn init_state(_today: &str) -> State {
    let mut st = State::default();
    for (i, (n, c)) in CATEGORY_DEFAULTS.iter().enumerate() {
        st.categories.push(Category { id: i as i64 + 1, name: n.to_string(), color: c.to_string() });
    }
    let mut ex_id = 0i64;
    for (ti, (name, list)) in TEMPLATES.iter().enumerate() {
        let mut exercises = vec![];
        for (en, sets, lo, hi) in list {
            let id = match st.gym.exercises.iter().find(|e| e.name == *en) {
                Some(e) => e.id,
                None => {
                    ex_id += 1;
                    st.gym.exercises.push(Exercise { id: ex_id, name: en.to_string() });
                    ex_id
                }
            };
            exercises.push(TemplateExercise { exercise_id: id, name: en.to_string(), sets: *sets, rep_min: *lo, rep_max: *hi });
        }
        st.gym.templates.push(Template { id: ti as i64 + 1, name: name.to_string(), exercises });
    }
    for n in ROTATION {
        if n.is_empty() {
            st.gym.rotation.items.push(RotItem { template_id: None, rest: true });
        } else {
            let tid = st.gym.templates.iter().find(|t| t.name == n).map(|t| t.id);
            st.gym.rotation.items.push(RotItem { template_id: tid, rest: false });
        }
    }
    st.gym.rotation.next_index = 0;
    st
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

type Log = (&'static str, f64, [i64; 3]);

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

    // Push A (-8d), Pull A (-6d), Legs A (-4d) done, rest taken (-3d) => today is Push B.
    let sessions: [(&str, i64, Vec<Log>); 3] = [
        ("Push A", -8, vec![
            ("Bench Press", 60.0, [10, 9, 8]),
            ("Overhead Press", 40.0, [10, 9, 8]),
            ("Incline DB Press", 22.5, [12, 12, 12]),
            ("Lateral Raise", 10.0, [15, 14, 12]),
            ("Tricep Pushdown", 30.0, [13, 12, 11]),
        ]),
        ("Pull A", -6, vec![
            ("Deadlift", 100.0, [8, 7, 6]),
            ("Pull-ups", 0.0, [8, 7, 6]),
            ("Barbell Row", 55.0, [10, 10, 9]),
            ("Face Pull", 20.0, [15, 15, 14]),
            ("Bicep Curl", 14.0, [12, 11, 10]),
        ]),
        ("Legs A", -4, vec![
            ("Squat", 80.0, [8, 8, 7]),
            ("Romanian Deadlift", 70.0, [10, 10, 9]),
            ("Leg Press", 140.0, [12, 12, 12]),
            ("Leg Curl", 35.0, [12, 11, 10]),
            ("Calf Raise", 50.0, [15, 15, 13]),
        ]),
    ];
    let mut added_sessions = 0;
    for (tpl, off, logs) in &sessions {
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
                    env.st.gym.exercises.push(Exercise { id, name: en.to_string() });
                    id
                }
            };
            for (i, r) in reps.iter().enumerate() {
                sets.push(SetRec { exercise_id: eid, set_no: i as i64 + 1, weight: *w, reps: *r, done: true });
            }
        }
        let id = env.st.new_id();
        env.st.gym.sessions.push(Session { id, template_id: Some(tid), name: tpl.to_string(), date, completed: true, sets });
        added_sessions += 1;
    }
    if added_sessions > 0 {
        // the default rotation is Push A, Pull A, Legs A, Rest, Push B, ...: next is Push B
        let len = env.st.gym.rotation.items.len();
        if len > 4 {
            env.st.gym.rotation.next_index = 4;
        }
        env.st.gym.rest_taken_date = Some(day(-3));
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
