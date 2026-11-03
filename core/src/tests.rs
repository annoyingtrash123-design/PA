use crate::briefing::{self, Kind};
use crate::env::Env;
use crate::model::{Ctx, State};
use crate::util::*;
use crate::{call, call_typed, has_stats, init_state};
use chrono::NaiveDate;
use serde_json::{json, Value};

const TODAY: &str = "2026-11-02"; // a Monday

struct App {
    state: String,
    today: String,
    holidays: Value,
    weather: Value,
}

impl App {
    fn new(today: &str) -> App {
        App { state: init_state(today), today: today.into(), holidays: json!([]), weather: json!({}) }
    }
    fn ctx(&self) -> String {
        json!({"today": self.today, "now": "07:42", "has_key": true, "weather": self.weather, "holidays": self.holidays}).to_string()
    }
    fn req(&mut self, method: &str, path: &str, body: Value) -> (u16, Value) {
        let raw = call(&self.state, method, path, &body.to_string(), &self.ctx());
        let r: Value = serde_json::from_str(&raw).unwrap();
        if !r["state"].is_null() {
            self.state = r["state"].to_string();
        }
        (r["status"].as_u64().unwrap() as u16, r["body"].clone())
    }
    fn get(&mut self, path: &str) -> Value {
        let (s, b) = self.req("GET", path, json!({}));
        assert_eq!(s, 200, "GET {path}: {b}");
        b
    }
    fn post(&mut self, path: &str, body: Value) -> Value {
        let (s, b) = self.req("POST", path, body);
        assert_eq!(s, 200, "POST {path}: {b}");
        b
    }
    fn st(&self) -> State {
        serde_json::from_str(&self.state).unwrap()
    }
    fn with_sample(today: &str) -> App {
        let mut a = App::new(today);
        a.post("/api/sample-data", json!({}));
        a
    }
}

fn d(s: &str) -> NaiveDate {
    parse_date(s).unwrap()
}

#[test]
fn init_state_defaults() {
    let st: State = serde_json::from_str(&init_state(TODAY)).unwrap();
    assert_eq!(st.version, 2);
    assert_eq!(st.next_id, 100);
    assert_eq!(st.categories.len(), 10);
    assert_eq!(st.categories[0].name, "Chores");
    let names: Vec<&str> = st.gym.templates.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Push 1", "Push 2", "Pull 1", "Pull 2", "Legs + Abs 1", "Legs + Abs 2", "Cardio"]);
    assert_eq!(st.gym.rotation.items.len(), 8);
    let rot: Vec<&str> = st.gym.rotation.items.iter().map(|i| st.gym.templates.iter().find(|t| Some(t.id) == i.template_id).unwrap().name.as_str()).collect();
    assert_eq!(rot, ["Push 1", "Pull 1", "Legs + Abs 1", "Cardio", "Push 2", "Pull 2", "Legs + Abs 2", "Cardio"]);
    assert!(st.gym.templates.iter().filter(|t| t.kind == "strength").all(|t| t.exercises.iter().all(|e| e.sets == 4 && e.rep_min == 10 && e.rep_max == 10)));
    assert!(st.tasks.is_empty() && st.goals.is_empty() && st.gym.sessions.is_empty() && st.memory.notes.is_empty());
    assert_eq!(st.settings.location_name, "Sydney");
    assert_eq!(st.memory.work.holiday_region, "AU-NSW");
}

#[test]
fn recurrence_matching() {
    use crate::model::Recurrence::*;
    let (mon, sat, sun) = (d("2026-11-02"), d("2026-11-07"), d("2026-11-08"));
    assert!(Daily.matches(sat));
    assert!(Weekdays.matches(mon) && !Weekdays.matches(sat) && !Weekdays.matches(sun));
    let w = Weekly { days: vec![0, 6] };
    assert!(w.matches(mon) && w.matches(sun) && !w.matches(sat));

    let mut a = App::new(TODAY);
    a.post("/api/tasks", json!({"title": "Bins", "recurrence": {"kind": "weekly", "days": [2, 0, 2]}, "category_id": null}));
    let days = a.get("/api/days?start=2026-11-02&days=7");
    let with_bins: Vec<&str> = days.as_array().unwrap().iter().filter(|x| !x["items"].as_array().unwrap().is_empty()).map(|x| x["weekday"].as_str().unwrap()).collect();
    assert_eq!(with_bins, vec!["Monday", "Wednesday"]);
    // weekly days are sorted and deduped
    assert_eq!(a.get("/api/tasks")[0]["recurrence"], json!({"kind": "weekly", "days": [0, 2]}));
    assert_eq!(a.req("POST", "/api/tasks", json!({"title": "x", "recurrence": {"kind": "weekly", "days": [9]}})).0, 400);
}

#[test]
fn rollover_moves_unfinished_one_offs_and_returns_state() {
    let mut a = App::new(TODAY);
    let late = a.post("/api/tasks", json!({"title": "Pay rent", "date": "2026-10-30"}));
    let done = a.post("/api/tasks", json!({"title": "Old done thing", "date": "2026-10-30"}));
    a.post("/api/occurrences/toggle", json!({"task_id": done["id"], "date": "2026-10-30", "done": true}));
    let raw = call(&a.state, "GET", "/api/days?start=2026-11-02&days=1", "", &a.ctx());
    let r: Value = serde_json::from_str(&raw).unwrap();
    assert!(!r["state"].is_null(), "rollover must return the new state");
    a.state = r["state"].to_string();
    let items = r["body"][0]["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Pay rent");
    assert_eq!(items[0]["rolled_over_from"], "2026-10-30");
    assert_eq!(a.get(&format!("/api/tasks/{}", late["id"]))["date"], TODAY);
    assert_eq!(a.get(&format!("/api/tasks/{}", done["id"]))["date"], "2026-10-30");
    // second call: nothing moves, state stays null; the original date is kept
    let raw = call(&a.state, "GET", "/api/days?start=2026-11-02&days=1", "", &a.ctx());
    let r: Value = serde_json::from_str(&raw).unwrap();
    assert!(r["state"].is_null());
    assert_eq!(r["body"][0]["items"][0]["rolled_over_from"], "2026-10-30");
    // a range that excludes today does not roll over
    let mut b = App::new(TODAY);
    b.post("/api/tasks", json!({"title": "Old", "date": "2026-10-30"}));
    let raw = call(&b.state, "GET", "/api/days?start=2026-10-26&days=3", "", &b.ctx());
    assert!(serde_json::from_str::<Value>(&raw).unwrap()["state"].is_null());
}

#[test]
fn toggle_and_checklist() {
    let mut a = App::new(TODAY);
    let t = a.post("/api/tasks", json!({"title": "Morning routine", "time": "07:30", "recurrence": {"kind": "daily"}, "checklist": [{"text": "Make bed"}, {"text": "Stretch"}]}));
    let item = t["checklist"][0]["id"].clone();
    assert_eq!(t["part_of_day"], "morning");
    let o = a.post("/api/occurrences/checklist", json!({"task_id": t["id"], "date": TODAY, "item_id": item, "done": true}));
    assert_eq!(o["checklist"][0]["done"], true);
    assert_eq!(o["checklist"][1]["done"], false);
    let o = a.post("/api/occurrences/toggle", json!({"task_id": t["id"], "date": TODAY, "done": true}));
    assert_eq!(o["done"], true);
    // other days are independent for recurring tasks
    let days = a.get("/api/days?start=2026-11-02&days=2");
    assert_eq!(days[0]["items"][0]["done"], true);
    assert_eq!(days[1]["items"][0]["done"], false);
    assert_eq!(a.req("POST", "/api/occurrences/checklist", json!({"task_id": t["id"], "date": TODAY, "item_id": 99999, "done": true})).0, 404);
    assert_eq!(a.req("POST", "/api/occurrences/toggle", json!({"task_id": 9999, "date": TODAY, "done": true})).0, 404);
}

#[test]
fn keyword_categorisation_of_sample_titles() {
    let cases = [
        ("Hoover the lounge", "Chores"),
        ("Revise chapter 3", "Study"),
        ("Dentist checkup", "Appointments"),
        ("Pay council tax", "Admin & Money"),
        ("Leg day", "Exercise"),
        ("Drinks with Jess", "Social"),
        ("Pick up parcel", "Errands"),
        ("Bubble bath", "Self-care"),
        ("Client call", "Work"),
        ("Fix bike", "Chores"),
    ];
    let mut a = App::new(TODAY);
    for (title, want) in cases {
        let t = a.post("/api/tasks", json!({"title": title, "date": TODAY, "category_id": "auto"}));
        assert_eq!(t["category_auto"], true, "{title}");
        let cid = t["category_id"].as_i64().unwrap();
        let name = a.st().categories.iter().find(|c| c.id == cid).unwrap().name.clone();
        assert_eq!(name, want, "{title}");
    }
    // no match -> Other; manual choice clears the auto flag
    let t = a.post("/api/tasks", json!({"title": "Zzz qqq", "date": TODAY, "category_id": "auto"}));
    let other = a.st().categories.iter().find(|c| c.name == "Other").unwrap().id;
    assert_eq!(t["category_id"], other);
    let t = a.post("/api/tasks", json!({"title": "Zzz qqq", "date": TODAY, "category_id": 1}));
    assert_eq!(t["category_auto"], false);
    assert_eq!(a.req("POST", "/api/tasks", json!({"title": "x", "date": TODAY, "category_id": 4242})).0, 400);
}

#[test]
fn categorise_prompt_and_set_category() {
    let mut a = App::new(TODAY);
    let t = a.post("/api/tasks", json!({"title": "Zzz qqq", "date": TODAY, "category_id": "auto", "notes": "n"}));
    let p = a.get(&format!("/api/tasks/{}/categorise-prompt", t["id"]));
    assert_eq!(p["names"].as_array().unwrap().len(), 10);
    assert!(p["user_prompt"].as_str().unwrap().contains("Task title: Zzz qqq"));
    let t2 = a.req("PUT", &format!("/api/tasks/{}/category", t["id"]), json!({"name": "study.", "auto": true}));
    assert_eq!(t2.0, 200);
    assert_eq!(t2.1["category_auto"], true);
    assert_eq!(a.req("PUT", &format!("/api/tasks/{}/category", t["id"]), json!({"name": "nope"})).0, 400);
}

#[test]
fn holiday_filter_keeps_global_or_matching_county() {
    let mut a = App::new(TODAY);
    let raw = json!([
        {"date": "2026-12-25", "name": "Christmas Day", "global": true, "counties": null},
        {"date": "2026-10-05", "name": "Labour Day", "global": false, "counties": ["AU-NSW", "AU-ACT"]},
        {"date": "2026-06-08", "name": "Queen's Birthday", "global": false, "counties": ["AU-VIC"]},
        {"date": "2026-01-26", "name": "Australia Day", "global": true}
    ]);
    let out = a.post("/api/holidays/filter", json!({"region": "AU-NSW", "raw": raw}));
    let names: Vec<&str> = out.as_array().unwrap().iter().map(|h| h["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["Christmas Day", "Labour Day", "Australia Day"]);
    assert_eq!(out[1], json!({"date": "2026-10-05", "name": "Labour Day"}));
    assert_eq!(a.post("/api/holidays/filter", json!({"region": "", "raw": raw})), json!([]));
}

#[test]
fn weather_summary_table() {
    let mut a = App::new(TODAY);
    assert_eq!(a.get("/api/weather-summary?code=61"), json!({"summary": "Light rain", "icon": "rain"}));
    assert_eq!(a.get("/api/weather-summary?code=0")["summary"], "Clear sky");
    assert_eq!(a.get("/api/weather-summary?code=99")["summary"], "Thunderstorm with hail");
    assert_eq!(a.get("/api/weather-summary?code=1234")["summary"], "Mixed weather");
}

#[test]
fn stats_guard() {
    assert!(has_stats("You did 3 of 5 things."));
    assert!(has_stats("7 out of 10"));
    assert!(has_stats("half done, 50 percent"));
    assert!(has_stats("80% there"));
    assert!(!has_stats("Bench went from 60 to 62.5 kilos, one of the best."));
}

fn tpl(a: &App, name: &str) -> crate::model::Template {
    a.st().gym.templates.iter().find(|t| t.name == name).unwrap().clone()
}
fn ex_id(a: &App, name: &str) -> i64 {
    a.st().gym.exercises.iter().find(|e| e.name == name).unwrap().id
}
fn plan_ex(plan: &Value, name: &str) -> Value {
    plan["exercises"].as_array().unwrap().iter().find(|e| e["name"] == name).unwrap_or_else(|| panic!("{name} not in plan")).clone()
}

#[test]
fn gym_projection_over_the_eight_item_cycle() {
    let mut a = App::with_sample(TODAY);
    // sample data: Push 1, Pull 1, Legs + Abs 1, Cardio done => today is Push 2
    let days = a.get("/api/days?start=2026-11-01&days=10");
    assert!(days[0]["workout"].is_null(), "past days have no workout");
    let names: Vec<&str> = (1..10).map(|i| days[i]["workout"]["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Push 2", "Pull 2", "Legs + Abs 2", "Cardio", "Push 1", "Pull 1", "Legs + Abs 1", "Cardio", "Push 2"]);
    assert_eq!(days[4]["workout"]["kind"], "cardio");
    assert_eq!(a.get("/api/gym/next")["name"], "Push 2");
}

#[test]
fn progression_up_for_normal_and_down_for_assisted() {
    let mut a = App::with_sample(TODAY);
    let days = a.get("/api/days?start=2026-11-02&days=2");
    let push2 = &days[0]["workout"];
    // incline press: 15 kg 4x10 with rep_max 10 -> 17.5 (shared with Push 1)
    let inc = plan_ex(push2, "Incline dumbbell chest press");
    assert_eq!(inc["suggested_weight"], 17.5);
    assert_eq!(inc["hint"], "You hit 10 reps on every set last time, try 17.5 kg");
    assert_eq!(inc["last"]["sets"].as_array().unwrap().len(), 4);
    // assisted pull-up wide grip: hit every rep at 68 -> 65.5 and the hint says less counterweight
    let wide = a.post("/api/sessions", json!({"template_id": tpl(&a, "Pull 1").id}));
    let w = wide["sets"].as_array().unwrap().iter().find(|x| x["exercise_id"] == ex_id(&a, "Counterweighted pull-up, wide grip")).unwrap().clone();
    assert_eq!(w["weight"], 65.5);
    assert_eq!(w["reps"], 10);
    let plan = a.get("/api/gym/next");
    assert_eq!(plan["name"], "Push 2");
    let nxt = a.get("/api/days?start=2026-11-02&days=7");
    let pull1_plan = nxt.as_array().unwrap().iter().map(|d| &d["workout"]).find(|w| w["name"] == "Pull 1").unwrap();
    let pu = plan_ex(pull1_plan, "Counterweighted pull-up, wide grip");
    assert_eq!(pu["assisted"], true);
    assert_eq!(pu["suggested_weight"], 65.5);
    assert_eq!(pu["hint"], "You hit every rep last time, try 2.5 kg less counterweight");
    // history: lowest counterweight is the best, no 1RM
    let h = a.get(&format!("/api/exercises/{}/history", ex_id(&a, "Counterweighted pull-up, wide grip")));
    assert_eq!(h[0]["top_weight"], 68.0);
    assert!(h[0]["est_1rm"].is_null());
    // a normal lift still has a 1RM
    let h2 = a.get(&format!("/api/exercises/{}/history", ex_id(&a, "Flat barbell chest press")));
    assert!(h2[0]["est_1rm"].is_number());
}

#[test]
fn assisted_stays_when_a_rep_is_missed_and_floors_at_zero() {
    use crate::gym::suggest;
    use crate::gym::{Last, LastSet};
    let mk = |w: f64, reps: &[i64]| Last { date: "x".into(), sets: reps.iter().map(|r| LastSet { weight: w, reps: *r }).collect() };
    assert_eq!(suggest(&mk(40.0, &[10, 10, 10, 9]), 10, true).0, 40.0);
    assert_eq!(suggest(&mk(40.0, &[10, 10, 10, 10]), 10, true).0, 37.5);
    assert_eq!(suggest(&mk(1.0, &[10, 10, 10, 10]), 10, true).0, 0.0);
    assert_eq!(suggest(&mk(0.0, &[10, 10, 10, 10]), 10, true).0, 0.0);
    assert_eq!(suggest(&mk(40.0, &[10, 10, 10, 10]), 10, false).0, 42.5);
}

#[test]
fn start_weight_is_the_suggestion_without_history() {
    let mut a = App::new(TODAY);
    let plan = a.get("/api/gym/next");
    assert_eq!(plan["name"], "Push 1");
    assert_eq!(plan["note"], "Chest + Triceps + Front Deltoid");
    let inc = plan_ex(&plan, "Incline dumbbell chest press");
    assert_eq!(inc["suggested_weight"], 15.0);
    assert_eq!(inc["hint"], "Starting weight: 15 kg");
    assert!(inc["last"].is_null());
    assert_eq!(plan_ex(&plan, "Tricep cable pushdown (two-handed)")["note"], "Bar or rope");
    let dip = plan_ex(&plan, "Counterweighted dip");
    assert_eq!(dip["suggested_weight"], 37.5);
    assert_eq!(dip["assisted"], true);
    // a session pre-fills 4 rows of 10 reps at the start weight
    let s = a.post("/api/sessions", json!({"template_id": tpl(&a, "Push 1").id}));
    let sets = s["sets"].as_array().unwrap();
    assert_eq!(sets.len(), 28);
    let inc_sets: Vec<&Value> = sets.iter().filter(|x| x["exercise_id"] == ex_id(&a, "Incline dumbbell chest press")).collect();
    assert_eq!(inc_sets.len(), 4);
    assert!(inc_sets.iter().all(|x| x["weight"] == 15.0 && x["reps"] == 10 && x["done"] == false));
    // no start weight and no history: nothing suggested
    let p2 = a.get("/api/days?start=2026-11-06&days=1");
    let cross = plan_ex(&p2[0]["workout"], "High-to-low cable crossover");
    assert!(cross["suggested_weight"].is_null() && cross["hint"].is_null());
    // Knee lifts are bodyweight
    let legs = tpl(&a, "Legs + Abs 1");
    assert!(legs.exercises.iter().find(|e| e.name == "Knee lifts").unwrap().start_weight.is_none());
}

#[test]
fn exercises_are_shared_by_name_so_history_carries_over() {
    let mut a = App::new(TODAY);
    let (p1, p2) = (tpl(&a, "Pull 1"), tpl(&a, "Pull 2"));
    let row = ex_id(&a, "Seated cable machine row");
    assert!(p1.exercises.iter().any(|e| e.exercise_id == row) && p2.exercises.iter().any(|e| e.exercise_id == row));
    assert_eq!(a.st().gym.exercises.iter().filter(|e| e.name == "Seated cable machine row").count(), 1);
    assert_eq!(ex_id(&a, "Incline dumbbell chest press"), tpl(&a, "Push 2").exercises[0].exercise_id);
    // log the row in Pull 1 at 20 kg, all 10 reps
    let s = a.post("/api/sessions", json!({"template_id": p1.id}));
    let sets: Vec<Value> = s["sets"].as_array().unwrap().iter().map(|x| {
        let mut x = x.clone();
        if x["exercise_id"] == row {
            x["weight"] = json!(20.0);
            x["reps"] = json!(10);
        }
        x["done"] = json!(x["exercise_id"] == row);
        x
    }).collect();
    a.req("PUT", &format!("/api/sessions/{}/sets", s["id"]), json!({"sets": sets}));
    a.post(&format!("/api/sessions/{}/complete", s["id"]), json!({}));
    // Pull 2 now knows about it: last time + progression to 22.5
    let days = a.get("/api/days?start=2026-11-02&days=8");
    let pull2 = days.as_array().unwrap().iter().map(|d| &d["workout"]).find(|w| w["name"] == "Pull 2").unwrap();
    let r = plan_ex(pull2, "Seated cable machine row");
    assert_eq!(r["last"]["sets"][0]["weight"], 20.0);
    assert_eq!(r["suggested_weight"], 22.5);
}

#[test]
fn cardio_session_flow() {
    let mut a = App::with_sample(TODAY);
    let cardio = tpl(&a, "Cardio");
    assert_eq!(cardio.kind, "cardio");
    assert_eq!(cardio.options, ["Swimming", "Rowing machine", "Stair machine", "Intense walking/hiking"]);
    assert_eq!((cardio.minutes_min, cardio.minutes_max), (Some(20), Some(40)));
    let plan = a.get("/api/days?start=2026-11-05&days=1");
    assert_eq!(plan[0]["workout"]["kind"], "cardio");
    assert_eq!(plan[0]["workout"]["note"], "Do one of the following for 20\u{2013}40 mins");
    assert_eq!(plan[0]["workout"]["options"].as_array().unwrap().len(), 4);
    let s = a.post("/api/sessions", json!({"template_id": cardio.id}));
    assert_eq!(s["cardio"], json!({"activity": "", "minutes": 30}));
    assert_eq!(s["sets"].as_array().unwrap().len(), 0);
    let sid = s["id"].as_i64().unwrap();
    // cannot finish before picking something
    assert_eq!(a.req("POST", &format!("/api/sessions/{sid}/complete"), json!({})).0, 400);
    assert_eq!(a.req("PUT", &format!("/api/sessions/{sid}/cardio"), json!({"activity": "Swimming", "minutes": 0})).0, 400);
    let u = a.req("PUT", &format!("/api/sessions/{sid}/cardio"), json!({"activity": "Swimming", "minutes": 35}));
    assert_eq!(u.0, 200);
    assert_eq!(u.1["cardio"], json!({"activity": "Swimming", "minutes": 35}));
    let rot_before = a.get("/api/rotation")["next_index"].as_i64().unwrap();
    let c = a.post(&format!("/api/sessions/{sid}/complete"), json!({}));
    assert_eq!(c["completed"], true);
    assert_eq!(c["cardio"]["activity"], "Swimming");
    // the first Cardio item at/after next_index (index 7, since next is Push 2 at 4) was passed
    assert_eq!(rot_before, 4);
    assert_eq!(a.get("/api/rotation")["next_index"], 0);
    // a strength session is not a cardio one
    let push = a.post("/api/sessions", json!({"template_id": tpl(&a, "Push 1").id}));
    assert_eq!(a.req("PUT", &format!("/api/sessions/{}/cardio", push["id"]), json!({"activity": "x", "minutes": 5})).0, 400);
    // cardio templates need options; validation on create
    let bad = a.req("POST", "/api/templates", json!({"name": "C2", "kind": "cardio", "options": []}));
    assert_eq!(bad.0, 400);
    assert_eq!(a.req("POST", "/api/templates", json!({"name": "C3", "kind": "yoga"})).0, 400);
}

#[test]
fn cardio_is_mentioned_naturally_in_the_week_and_evening() {
    let mut a = App::with_sample("2026-11-08"); // a Sunday; sample sessions at -8 (Oct 31), -6, -4 and cardio at -3 (Nov 5)
    let t = a.get("/api/briefing?kind=weekly&date=2026-11-08&ai=false");
    let text = t["template_text"].as_str().unwrap();
    assert!(text.contains("30 minutes on the rowing machine") || text.contains("rowing machine"), "{text}");
    let up = t["user_prompt"].as_str().unwrap();
    assert!(up.contains("Cardio completed this week"), "{up}");
    assert!(up.contains("30 minutes on the rowing machine"), "{up}");
    assert_eq!(crate::gym::cardio_phrase(&crate::model::Cardio { activity: "Swimming".into(), minutes: 30 }), "a swim");
    assert_eq!(crate::gym::cardio_phrase(&crate::model::Cardio { activity: "Rowing machine".into(), minutes: 40 }), "40 minutes on the rowing machine");
}

#[test]
fn gym_session_flow_advances_rotation() {
    let mut a = App::with_sample(TODAY);
    let push2 = tpl(&a, "Push 2").id;
    let s = a.post("/api/sessions", json!({"template_id": push2}));
    assert_eq!(s["completed"], false);
    assert_eq!(s["date"], TODAY);
    let sets = s["sets"].as_array().unwrap().clone();
    assert_eq!(sets.len(), 32); // 8 exercises x 4 sets
    // pre-filled: incline press progressed to 17.5 kg, back to rep_min (10)
    let inc = sets.iter().find(|x| x["weight"] == 17.5).unwrap();
    assert_eq!(inc["reps"], 10);
    assert_eq!(a.post("/api/sessions", json!({"template_id": push2}))["id"], s["id"]);
    let mut upd = sets.clone();
    upd[0]["done"] = json!(true);
    let s2 = a.req("PUT", &format!("/api/sessions/{}/sets", s["id"]), json!({"sets": upd}));
    assert_eq!(s2.0, 200);
    assert_eq!(s2.1["sets"][0]["done"], true);
    let c = a.post(&format!("/api/sessions/{}/complete", s["id"]), json!({}));
    assert_eq!(c["completed"], true);
    assert_eq!(a.get("/api/rotation")["next_index"], 5);
    let days = a.get("/api/days?start=2026-11-02&days=2");
    assert_eq!(days[0]["workout"]["name"], "Push 2");
    assert_eq!(days[1]["workout"]["name"], "Pull 2");
    // no rest items in the default rotation
    assert_eq!(a.req("POST", "/api/gym/rest", json!({})).0, 400);
    assert_eq!(a.get("/api/sessions?limit=2").as_array().unwrap().len(), 2);
}

#[test]
fn template_and_exercise_editing_with_new_fields() {
    let mut a = App::new(TODAY);
    let t = a.post("/api/templates", json!({"name": "Mine", "note": " hello ", "exercises": [
        {"name": "Sled push", "sets": 4, "rep_min": 10, "rep_max": 10, "start_weight": 20, "note": "alt: prowler"},
        {"name": "Assisted chin", "sets": 4, "rep_min": 10, "rep_max": 10, "assisted": true}]}));
    assert_eq!(t["kind"], "strength");
    assert_eq!(t["note"], "hello");
    assert_eq!(t["exercises"][0]["start_weight"], 20.0);
    assert_eq!(t["exercises"][0]["note"], "alt: prowler");
    assert!(t["exercises"][1]["start_weight"].is_null());
    let chin = a.st().gym.exercises.iter().find(|e| e.name == "Assisted chin").unwrap().clone();
    assert!(chin.assisted);
    let e = a.req("PUT", &format!("/api/exercises/{}", chin.id), json!({"name": "Assisted chin", "assisted": false}));
    assert_eq!(e.1["assisted"], false);
    let c = a.post("/api/templates", json!({"name": "Spin", "kind": "cardio", "note": "n", "options": ["Bike", "bike", "Run"], "minutes_min": 15, "minutes_max": 45}));
    assert_eq!(c["options"], json!(["Bike", "Run"]));
    assert_eq!(c["minutes_max"], 45);
    assert_eq!(a.req("POST", "/api/templates", json!({"name": "x", "exercises": [{"name": "y", "sets": 3, "rep_min": 5, "rep_max": 8, "start_weight": -1}]})).0, 400);
}

fn v1_state_with_old_defaults(with_sessions: bool) -> String {
    let mut v: Value = serde_json::from_str(&init_state(TODAY)).unwrap();
    let names = ["Push A", "Pull A", "Legs A", "Push B", "Pull B", "Legs B"];
    let exs = ["Bench Press", "Deadlift", "Squat", "Dips", "Lat Pulldown", "Hip Thrust"];
    let mut exercises = vec![];
    let mut templates = vec![];
    for i in 0..6 {
        exercises.push(json!({"id": i + 1, "name": exs[i]}));
        templates.push(json!({"id": i + 11, "name": names[i], "exercises": [{"exercise_id": i + 1, "name": exs[i], "sets": 3, "rep_min": 6, "rep_max": 10}]}));
    }
    let items: Vec<Value> = (0..6).map(|i| json!({"template_id": i + 11})).collect();
    let sessions = if with_sessions {
        json!([{"id": 50, "template_id": 11, "name": "Push A", "date": "2026-10-30", "completed": true, "sets": [{"exercise_id": 1, "set_no": 1, "weight": 60, "reps": 10, "done": true}]}])
    } else {
        json!([])
    };
    v["version"] = json!(1);
    v["gym"] = json!({"exercises": exercises, "templates": templates, "rotation": {"items": items, "next_index": 2}, "sessions": sessions});
    v.to_string()
}

#[test]
fn migration_replaces_untouched_v1_defaults_and_returns_state() {
    for with_sessions in [false, true] {
        let mut a = App::new(TODAY);
        a.state = v1_state_with_old_defaults(with_sessions);
        // any call returns the migrated state
        let raw = call(&a.state, "GET", "/api/templates", "{}", &a.ctx());
        let r: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(r["status"], 200);
        assert!(!r["state"].is_null(), "migrated state must be returned so the UI persists it");
        assert_eq!(r["state"]["version"], 2);
        a.state = r["state"].to_string();
        let st = a.st();
        let names: Vec<&str> = st.gym.templates.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Push 1", "Push 2", "Pull 1", "Pull 2", "Legs + Abs 1", "Legs + Abs 2", "Cardio"]);
        assert_eq!(st.gym.rotation.items.len(), 8);
        assert_eq!(st.gym.rotation.next_index, 0);
        assert!(st.gym.sessions.is_empty(), "sessions of the old defaults are dropped");
        assert!(st.gym.exercises.iter().all(|e| e.name != "Bench Press"));
        // ids do not clash with anything else
        assert!(st.next_id > st.max_id());
        // second call: nothing more to migrate
        let again: Value = serde_json::from_str(&call(&a.state, "GET", "/api/templates", "{}", &a.ctx())).unwrap();
        assert!(again["state"].is_null());
        // the explicit endpoint works as well
        let m = a.get("/api/state/migrate");
        assert_eq!(m["version"], 2);
    }
}

#[test]
fn migration_endpoint_on_v1_state_and_customised_state() {
    // explicit endpoint migrates and returns the state
    let mut a = App::new(TODAY);
    a.state = v1_state_with_old_defaults(false);
    let r: Value = serde_json::from_str(&call(&a.state, "GET", "/api/state/migrate", "{}", &a.ctx())).unwrap();
    assert_eq!(r["body"]["version"], 2);
    assert_eq!(r["state"]["gym"]["templates"].as_array().unwrap().len(), 7);
    // customised: a renamed template -> keep everything, only bump the version
    let mut v: Value = serde_json::from_str(&v1_state_with_old_defaults(true)).unwrap();
    v["gym"]["templates"][0]["name"] = json!("My Push");
    a.state = v.to_string();
    let r: Value = serde_json::from_str(&call(&a.state, "GET", "/api/state/migrate", "{}", &a.ctx())).unwrap();
    assert_eq!(r["state"]["version"], 2);
    let g = &r["state"]["gym"];
    assert_eq!(g["templates"].as_array().unwrap().len(), 6);
    assert_eq!(g["templates"][0]["name"], "My Push");
    assert_eq!(g["templates"][0]["kind"], "strength");
    assert_eq!(g["exercises"][0]["assisted"], false);
    assert_eq!(g["sessions"].as_array().unwrap().len(), 1);
    // an old template/session still works
    a.state = r["state"].to_string();
    assert_eq!(a.get("/api/gym/next")["name"], "Legs A");
    // importing a v1 backup migrates it too
    let mut b = App::new(TODAY);
    let imp = b.post("/api/import", serde_json::from_str(&v1_state_with_old_defaults(false)).unwrap());
    assert_eq!(imp["version"], 2);
    assert_eq!(imp["gym"]["templates"][6]["name"], "Cardio");
}

#[test]
fn work_day_shape_with_holiday_and_day_off() {
    let mut a = App::new(TODAY);
    a.holidays = json!([{"date": "2026-11-04", "name": "Test Holiday"}]);
    a.post("/api/days-off", json!({"date": "2026-11-05"}));
    let days = a.get("/api/days?start=2026-11-02&days=7");
    let days = days.as_array().unwrap();
    assert_eq!(days.len(), 7);
    let mon = &days[0];
    assert_eq!(mon["date"], "2026-11-02");
    assert_eq!(mon["weekday"], "Monday");
    assert_eq!(mon["work"], json!({"start": "10:00", "end": "17:00"}));
    assert!(mon["holiday"].is_null());
    assert_eq!(mon["day_off"], false);
    assert!(mon["items"].as_array().unwrap().is_empty());
    assert!(!mon["workout"].is_null(), "today has a projected workout");
    assert!(days[1]["work"].is_object());
    // holiday: no work, holiday name
    assert!(days[2]["work"].is_null());
    assert_eq!(days[2]["holiday"], "Test Holiday");
    // day off
    assert!(days[3]["work"].is_null());
    assert_eq!(days[3]["day_off"], true);
    // weekend
    assert!(days[5]["work"].is_null() && days[6]["work"].is_null());
    assert_eq!(a.get("/api/days-off"), json!(["2026-11-05"]));
    assert_eq!(a.req("DELETE", "/api/days-off/2026-11-05", json!({})).1, json!([]));
    // work disabled: nothing is a work day
    a.req("PUT", "/api/memory/work", json!({"enabled": false}));
    assert!(a.get("/api/days?start=2026-11-02&days=1")[0]["work"].is_null());
}

#[test]
fn memory_validation_and_notes() {
    let mut a = App::new(TODAY);
    assert_eq!(a.req("PUT", "/api/memory/weather", json!({"comfortable_max": 30})).0, 400);
    assert_eq!(a.req("PUT", "/api/memory/weather", json!({"comfortable_max": 17, "likes_rain": false})).0, 200);
    assert_eq!(a.req("PUT", "/api/memory/work", json!({"start": "18:00"})).0, 400);
    assert_eq!(a.req("PUT", "/api/memory/work", json!({"lunch_minutes": 500})).0, 400);
    assert_eq!(a.req("PUT", "/api/memory/work", json!({"days": [4, 0, 0], "holiday_region": ""})).1["work"]["days"], json!([0, 4]));
    assert_eq!(a.req("PUT", "/api/memory/rhythm", json!({"wake_from": "08:00"})).0, 400);
    let m = a.post("/api/memory/notes", json!({"text": "I like tea"}));
    let id = m["notes"][0]["id"].as_i64().unwrap();
    assert_eq!(a.req("PUT", &format!("/api/memory/notes/{id}"), json!({"text": "I like coffee"})).1["notes"][0]["text"], "I like coffee");
    assert_eq!(a.req("DELETE", "/api/memory/notes/424242", json!({})).0, 404);
    assert_eq!(a.req("DELETE", &format!("/api/memory/notes/{id}"), json!({})).1["notes"], json!([]));
    assert_eq!(a.req("POST", "/api/memory/notes", json!({"text": ""})).0, 400);
}

#[test]
fn settings_and_errors() {
    let mut a = App::new(TODAY);
    let s = a.get("/api/settings");
    assert_eq!(s["gemini_api_key_set"], true);
    assert_eq!(s["assistant_name"], "Sunny");
    let s = a.req("PUT", "/api/settings", json!({"user_name": " Sam ", "gemini_api_key": "SECRET", "latitude": 51.5}));
    assert_eq!(s.1["user_name"], "Sam");
    assert!(!a.state.contains("SECRET"));
    assert!(s.1.get("gemini_api_key").is_none());
    assert_eq!(a.req("PUT", "/api/settings", json!({"latitude": 100})).0, 400);
    assert_eq!(a.req("PUT", "/api/settings", json!({"assistant_name": " "})).0, 400);
    assert_eq!(a.req("GET", "/api/nope", json!({})).0, 404);
    assert_eq!(a.req("GET", "/api/tasks/77", json!({})).0, 404);
    assert_eq!(call_typed("not json", "GET", "/api/settings", "", "{}").status, 400);
    assert_eq!(call_typed(&init_state(TODAY), "GET", "/api/days", "", "{}").status, 400);
}

#[test]
fn goals_crud_and_order() {
    let mut a = App::new(TODAY);
    let g1 = a.post("/api/goals", json!({"title": "Later", "horizon": "long"}));
    let g2 = a.post("/api/goals", json!({"title": "Soon", "horizon": "medium", "deadline": "2026-12-01"}));
    let list = a.get("/api/goals");
    assert_eq!(list[0]["title"], "Soon");
    assert_eq!(g1["created"], TODAY);
    let done = a.req("PUT", &format!("/api/goals/{}", g2["id"]), json!({"title": "Soon", "horizon": "medium", "status": "achieved"}));
    assert_eq!(done.1["achieved_on"], TODAY);
    assert_eq!(a.get("/api/goals")[0]["title"], "Later");
    assert_eq!(a.req("POST", "/api/goals", json!({"title": "x", "horizon": "weird"})).0, 400);
    assert_eq!(a.req("DELETE", &format!("/api/goals/{}", g1["id"]), json!({})).0, 200);
    assert_eq!(a.req("DELETE", &format!("/api/goals/{}", g1["id"]), json!({})).0, 404);
}

#[test]
fn export_import_roundtrip() {
    let a = App::with_sample(TODAY);
    let mut b = App::new(TODAY);
    let mut a = a;
    let exported = a.get("/api/export");
    let (s, imported) = b.req("POST", "/api/import", exported.clone());
    assert_eq!(s, 200);
    assert_eq!(imported, exported);
    assert_eq!(b.st().tasks.len(), a.st().tasks.len());
    assert_eq!(b.req("POST", "/api/import", json!({"hello": 1})).0, 400);
    assert_eq!(b.req("POST", "/api/import", json!({"version": 99})).0, 400);
}

#[test]
fn sample_data_is_relative_to_today_and_idempotent() {
    let mut a = App::new("2027-03-10");
    a.post("/api/sample-data", json!({}));
    let n = a.st().tasks.len();
    a.post("/api/sample-data", json!({}));
    assert_eq!(a.st().tasks.len(), n);
    assert_eq!(a.st().goals.len(), 3);
    assert_eq!(a.st().gym.sessions.len(), 4);
    assert_eq!(a.st().gym.sessions[0].date, "2027-03-02");
    assert!(a.st().gym.sessions.iter().any(|s| s.cardio.as_ref().is_some_and(|c| c.activity == "Rowing machine" && c.minutes == 30)));
    assert!(a.st().tasks.iter().any(|t| t.date.as_deref() == Some("2027-03-09")), "yesterday's leftover");
}

fn morning(a: &mut App, date: &str) -> String {
    let b = a.get(&format!("/api/briefing?kind=morning&date={date}"));
    b["template_text"].as_str().unwrap().to_string()
}

#[test]
fn morning_template_word_count_on_sample_data() {
    let weather = json!({"summary": "Light rain", "temp_max": 14, "temp_min": 8, "precip_prob": 80, "uv_max": 1.5});
    for with_weather in [true, false] {
        let mut a = App::with_sample(TODAY);
        for off in 0..28 {
            let day = fmt_date(d(TODAY) + chrono::Duration::days(off));
            a.today = day.clone();
            if with_weather {
                a.weather = json!({ day.clone(): weather });
            }
            let text = morning(&mut a, &day);
            let n = words_in(&text);
            assert!((320..=450).contains(&n), "{day} weather={with_weather}: {n} words\n{text}");
            assert!(!has_stats(&text), "{day}: {text}");
        }
    }
}

#[test]
fn morning_template_prefers_meaningful_fixed_points() {
    let mut a = App::with_sample(TODAY);
    let text = morning(&mut a, TODAY);
    assert!(text.contains("Team stand-up at 10:15 AM"), "{text}");
    // the 7:30 routine is not presented as a fixed point or the one to watch
    for s in text.split(". ") {
        if s.contains("fixed points") || s.contains("one to watch") || s.contains("only get one thing") || s.starts_with("Start with") {
            assert!(!s.contains("Morning routine at 7:30"), "{s}");
        }
    }
    // planning habit is not filed under work, but under looking after yourself
    assert!(!text.contains("work front you've got Plan tomorrow") && !text.contains("For work, there's Plan tomorrow"), "{text}");
    for s in text.split(". ") {
        if s.contains("Plan tomorrow") && (s.contains("work") || s.contains("Work")) {
            assert!(s.contains("Reply to Sam") || !s.contains("front"), "{s}");
        }
    }
    assert!(text.contains("looking after yourself") || text.contains("routine side"), "{text}");
    let b = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    assert_eq!(b["max_words"], 450);
    assert!(b["cache_key"].as_str().unwrap().starts_with("briefing:morning:2026-11-02:"));
    assert!(b["user_prompt"].as_str().unwrap().contains("Plan tomorrow"));
    assert_eq!(b["stats"]["total"], 7);
    assert_eq!(b["stats"]["done"], 0);
    assert!(b["affirmation"].as_str().unwrap().len() > 10);
    println!("{text}");
}

#[test]
fn briefing_cache_key_tracks_done_state_and_inputs() {
    let mut a = App::with_sample(TODAY);
    let k1 = a.get("/api/briefing?kind=morning")["cache_key"].clone();
    assert_eq!(a.get("/api/briefing?kind=morning")["cache_key"], k1);
    let day = a.get("/api/days?start=2026-11-02&days=1");
    let tid = day[0]["items"][1]["task_id"].clone();
    a.post("/api/occurrences/toggle", json!({"task_id": tid, "date": TODAY, "done": true}));
    assert_ne!(a.get("/api/briefing?kind=morning")["cache_key"], k1);
    let e = a.get("/api/briefing?kind=evening");
    assert!(e["template_text"].as_str().unwrap().contains("Goodnight") || e["template_text"].as_str().unwrap().contains("Sleep well") || e["template_text"].as_str().unwrap().contains("Rest up"));
    let w = a.get("/api/briefing?kind=weekly");
    assert!(w["weather"].is_null());
    assert_eq!(a.req("GET", "/api/briefing?kind=bogus", json!({})).0, 400);
}

#[test]
fn digest_and_day_note_prepare() {
    let mut a = App::with_sample(TODAY);
    a.weather = json!({TODAY: {"summary": "Light rain", "temp_max": 14, "temp_min": 8, "precip_prob": 80, "uv_max": 1.5}});
    let g = a.get(&format!("/api/digest?date={TODAY}"));
    assert_eq!(g["weather"]["summary"], "Light rain");
    assert!(g["template"]["weather_comment"].as_str().unwrap().contains("14"));
    assert!(g["template"]["overview"].as_str().unwrap().len() > 30);
    assert!(g["cache_key"].as_str().unwrap().starts_with("digest:2026-11-02:"));
    assert!(g["user_prompt"].as_str().unwrap().contains("Team stand-up"));
    let g2 = a.get("/api/digest?date=2026-11-03");
    assert!(g2["weather"].is_null() && g2["template"]["weather_comment"].is_null());
    let n = a.get(&format!("/api/day-note?date={TODAY}"));
    assert!(n["template_text"].as_str().unwrap().len() > 20);
    assert_eq!(n["skip_ai"], false);
    assert!(n["cache_key"].as_str().unwrap().starts_with("daynote:2026-11-02:"));
    assert!(n["template_text"].as_str().unwrap().contains("Team stand-up"), "{}", n["template_text"]);
    // an empty day skips AI
    let e = a.get("/api/day-note?date=2026-12-26");
    assert_eq!(e["skip_ai"], false); // recurring sample tasks exist
    let mut b = App::new(TODAY);
    assert_eq!(b.get("/api/day-note")["skip_ai"], true);
}

#[test]
fn evening_and_weekly_templates_render() {
    let mut a = App::with_sample("2026-11-08");
    a.today = "2026-11-08".into();
    for kind in [Kind::Evening, Kind::Weekly, Kind::Morning] {
        let st: State = a.st();
        let mut st2 = st.clone();
        let ctx: Ctx = serde_json::from_str(&a.ctx()).unwrap();
        let mut env = Env { st: &mut st2, ctx: &ctx, today: d("2026-11-08"), changed: false };
        let t = briefing::template_for(&mut env, kind, d("2026-11-08"));
        assert!(words_in(&t) > 40, "{kind:?}: {t}");
        assert!(!has_stats(&t));
    }
}
