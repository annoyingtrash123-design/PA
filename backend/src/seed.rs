//! Friendly demo data inserted on the very first run (empty database).
use crate::db::{fmt_date, get_setting, set_setting, today};
use chrono::Duration;
use rusqlite::{params, Connection};
use std::collections::HashMap;

struct T<'a> {
    title: &'a str,
    day: Option<i64>, // offset from today for one-offs
    time: Option<&'a str>,
    part: Option<&'a str>,
    cat: &'a str,
    notes: &'a str,
    rec: Option<&'a str>,
    steps: &'a [&'a str],
}

pub fn seed_if_empty(conn: &Connection) -> rusqlite::Result<()> {
    if get_setting(conn, "seeded")?.is_some() {
        return Ok(());
    }
    let empty: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM tasks)+(SELECT COUNT(*) FROM categories)+(SELECT COUNT(*) FROM templates)",
        [],
        |r| r.get(0),
    )?;
    if empty > 0 {
        set_setting(conn, "seeded", "1")?;
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    seed(&tx)?;
    set_setting(&tx, "seeded", "1")?;
    tx.commit()?;
    tracing::info!("seeded demo data");
    Ok(())
}

fn seed(conn: &Connection) -> rusqlite::Result<()> {
    let today = today();
    let day = |off: i64| fmt_date(today + Duration::days(off));

    // categories
    let mut cats: HashMap<&str, i64> = HashMap::new();
    for (n, c) in [("Health", "#4CAF50"), ("Work", "#2196F3"), ("Home", "#FF9800"), ("Personal", "#9C27B0")] {
        conn.execute("INSERT INTO categories(name,color) VALUES(?1,?2)", params![n, c])?;
        cats.insert(n, conn.last_insert_rowid());
    }

    let tasks = [
        T { title: "Morning routine", day: None, time: Some("07:30"), part: None, cat: "Health", notes: "", rec: Some(r#"{"kind":"daily"}"#), steps: &["Make bed", "Drink water", "Stretch"] },
        T { title: "Plan tomorrow", day: None, time: None, part: Some("evening"), cat: "Work", notes: "Five quiet minutes to line up the next day.", rec: Some(r#"{"kind":"weekdays"}"#), steps: &[] },
        T { title: "Laundry", day: None, time: None, part: Some("afternoon"), cat: "Home", notes: "", rec: Some(r#"{"kind":"weekly","days":[6]}"#), steps: &["Wash", "Dry", "Fold and put away"] },
        T { title: "Pay electricity bill", day: Some(-1), time: None, part: Some("anytime"), cat: "Home", notes: "Yesterday's leftover, it rolls over until done.", rec: None, steps: &[] },
        T { title: "Reply to Sam's email", day: Some(0), time: None, part: Some("morning"), cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Team stand-up", day: Some(0), time: Some("10:00"), part: None, cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Call Mum", day: Some(0), time: Some("18:30"), part: None, cat: "Personal", notes: "", rec: None, steps: &[] },
        T { title: "Book dentist appointment", day: Some(0), time: None, part: Some("anytime"), cat: "Health", notes: "", rec: None, steps: &[] },
        T { title: "Grocery shopping", day: Some(1), time: None, part: Some("afternoon"), cat: "Home", notes: "Eggs, oats, spinach, bananas.", rec: None, steps: &[] },
        T { title: "Project check-in", day: Some(1), time: Some("09:30"), part: None, cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Coffee with Alex", day: Some(2), time: Some("15:00"), part: None, cat: "Personal", notes: "", rec: None, steps: &[] },
        T { title: "Submit weekly report", day: Some(3), time: None, part: Some("morning"), cat: "Work", notes: "", rec: None, steps: &[] },
        T { title: "Fix the leaky tap", day: Some(4), time: None, part: Some("anytime"), cat: "Home", notes: "", rec: None, steps: &[] },
        T { title: "Evening walk", day: Some(4), time: None, part: Some("evening"), cat: "Health", notes: "", rec: None, steps: &[] },
    ];
    for t in &tasks {
        let date = t.day.map(day);
        conn.execute(
            "INSERT INTO tasks(title,date,time,part_of_day,category_id,notes,recurrence) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![t.title, date, t.time, t.part, cats[t.cat], t.notes, t.rec],
        )?;
        let tid = conn.last_insert_rowid();
        for (pos, s) in t.steps.iter().enumerate() {
            conn.execute("INSERT INTO checklist_items(task_id,text,pos) VALUES(?1,?2,?3)", params![tid, s, pos as i64])?;
        }
    }

    // gym
    type Ex = (&'static str, i64, i64, i64);
    let templates: [(&str, [Ex; 5]); 6] = [
        ("Push A", [("Bench Press", 3, 6, 10), ("Overhead Press", 3, 8, 12), ("Incline DB Press", 3, 8, 12), ("Lateral Raise", 3, 12, 15), ("Tricep Pushdown", 3, 10, 15)]),
        ("Pull A", [("Deadlift", 3, 5, 8), ("Pull-ups", 3, 6, 10), ("Barbell Row", 3, 8, 12), ("Face Pull", 3, 12, 15), ("Bicep Curl", 3, 10, 12)]),
        ("Legs A", [("Squat", 3, 6, 10), ("Romanian Deadlift", 3, 8, 12), ("Leg Press", 3, 10, 12), ("Leg Curl", 3, 10, 15), ("Calf Raise", 3, 12, 15)]),
        ("Push B", [("Incline DB Press", 3, 8, 12), ("Dips", 3, 8, 12), ("Seated DB Shoulder Press", 3, 8, 12), ("Cable Fly", 3, 12, 15), ("Overhead Tricep Extension", 3, 10, 15)]),
        ("Pull B", [("Lat Pulldown", 3, 8, 12), ("Seated Cable Row", 3, 8, 12), ("Chin-ups", 3, 6, 10), ("Rear Delt Fly", 3, 12, 15), ("Hammer Curl", 3, 10, 12)]),
        ("Legs B", [("Front Squat", 3, 6, 10), ("Hip Thrust", 3, 8, 12), ("Walking Lunges", 3, 10, 12), ("Leg Extension", 3, 10, 15), ("Seated Calf Raise", 3, 12, 15)]),
    ];
    let mut ex_ids: HashMap<&str, i64> = HashMap::new();
    let mut tpl_ids: HashMap<&str, i64> = HashMap::new();
    for (name, list) in &templates {
        conn.execute("INSERT INTO templates(name) VALUES(?1)", params![name])?;
        let tid = conn.last_insert_rowid();
        tpl_ids.insert(name, tid);
        for (pos, (en, sets, lo, hi)) in list.iter().enumerate() {
            let eid = match ex_ids.get(en) {
                Some(i) => *i,
                None => {
                    conn.execute("INSERT INTO exercises(name) VALUES(?1)", params![en])?;
                    let i = conn.last_insert_rowid();
                    ex_ids.insert(en, i);
                    i
                }
            };
            conn.execute(
                "INSERT INTO template_exercises(template_id,exercise_id,pos,sets,rep_min,rep_max) VALUES(?1,?2,?3,?4,?5,?6)",
                params![tid, eid, pos as i64, sets, lo, hi],
            )?;
        }
    }

    // rotation: Push A, Pull A, Legs A, Rest, Push B, Pull B, Legs B, Rest
    let order = ["Push A", "Pull A", "Legs A", "", "Push B", "Pull B", "Legs B", ""];
    for (i, n) in order.iter().enumerate() {
        if n.is_empty() {
            conn.execute("INSERT INTO rotation_items(pos,template_id,rest) VALUES(?1,NULL,1)", params![i as i64])?;
        } else {
            conn.execute("INSERT INTO rotation_items(pos,template_id,rest) VALUES(?1,?2,0)", params![i as i64, tpl_ids[n]])?;
        }
    }
    // Push A (-8d), Pull A (-6d), Legs A (-4d) done, rest taken (-3d) => today is Push B.
    set_setting(conn, "rotation_next", "4")?;

    // past sessions: (template, day offset, [(exercise, weight, reps per set)])
    type Log = (&'static str, f64, [i64; 3]);
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
    for (tpl, off, logs) in &sessions {
        conn.execute(
            "INSERT INTO sessions(template_id,name,date,completed) VALUES(?1,?2,?3,1)",
            params![tpl_ids[tpl], tpl, day(*off)],
        )?;
        let sid = conn.last_insert_rowid();
        for (en, w, reps) in logs {
            for (i, r) in reps.iter().enumerate() {
                conn.execute(
                    "INSERT INTO session_sets(session_id,exercise_id,set_no,weight,reps,done) VALUES(?1,?2,?3,?4,?5,1)",
                    params![sid, ex_ids[en], (i + 1) as i64, w, r],
                )?;
            }
        }
    }
    set_setting(conn, "rest_taken_date", &day(-3))?;
    Ok(())
}
