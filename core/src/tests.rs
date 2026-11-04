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
    news: Value,
    news_at: String,
}

impl App {
    fn new(today: &str) -> App {
        App { state: init_state(today), today: today.into(), holidays: json!([]), weather: json!({}), news: json!([]), news_at: String::new() }
    }
    fn ctx(&self) -> String {
        json!({"today": self.today, "now": "07:42", "has_key": true, "weather": self.weather, "holidays": self.holidays, "news": self.news, "news_generated_at": self.news_at}).to_string()
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
    assert_eq!(st.version, 8);
    assert_eq!(st.next_id, 100);
    assert_eq!(st.categories.len(), 10);
    assert_eq!(st.categories[0].name, "Chores");
    let names: Vec<&str> = st.gym.templates.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["Push 1", "Push 2", "Pull 1", "Pull 2", "Legs + Abs 1", "Legs + Abs 2", "Cardio"]);
    assert_eq!(st.gym.rotation.items.len(), 10);
    let rot: Vec<&str> = st.gym.rotation.items.iter().map(|i| if i.rest { "Rest" } else { st.gym.templates.iter().find(|t| Some(t.id) == i.template_id).unwrap().name.as_str() }).collect();
    assert_eq!(rot, ["Push 1", "Pull 1", "Cardio", "Rest", "Legs + Abs 1", "Push 2", "Pull 2", "Cardio", "Rest", "Legs + Abs 2"]);
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
fn gym_projection_over_the_ten_item_cycle() {
    let mut a = App::with_sample(TODAY);
    // sample data: the first half of the cycle is done => today is Push 2
    let days = a.get("/api/days?start=2026-11-01&days=10");
    assert!(days[0]["workout"].is_null(), "past days have no workout");
    let names: Vec<&str> = (1..10).map(|i| if days[i]["workout"]["rest"] == true { "Rest" } else { days[i]["workout"]["name"].as_str().unwrap() }).collect();
    assert_eq!(names, ["Push 2", "Pull 2", "Cardio", "Rest", "Legs + Abs 2", "Push 1", "Pull 1", "Cardio", "Rest"]);
    assert_eq!(days[3]["workout"]["kind"], "cardio");
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
    let p2 = a.get("/api/days?start=2026-11-07&days=1");
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
    let plan = a.get("/api/days?start=2026-11-04&days=1");
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
    // the first Cardio item at/after next_index (index 7, since next is Push 2 at 5) was passed
    assert_eq!(rot_before, 5);
    assert_eq!(a.get("/api/rotation")["next_index"], 8, "the rest day follows the second cardio");
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
    assert_eq!(a.get("/api/rotation")["next_index"], 6);
    let days = a.get("/api/days?start=2026-11-02&days=2");
    assert_eq!(days[0]["workout"]["name"], "Push 2");
    assert_eq!(days[1]["workout"]["name"], "Pull 2");
    // the next item (Pull 2) is not a rest day
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
        assert_eq!(r["state"]["version"], 8);
        a.state = r["state"].to_string();
        let st = a.st();
        let names: Vec<&str> = st.gym.templates.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Push 1", "Push 2", "Pull 1", "Pull 2", "Legs + Abs 1", "Legs + Abs 2", "Cardio"]);
        assert_eq!(st.gym.rotation.items.len(), 10);
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
        assert_eq!(m["version"], 8);
    }
}

#[test]
fn migration_endpoint_on_v1_state_and_customised_state() {
    // explicit endpoint migrates and returns the state
    let mut a = App::new(TODAY);
    a.state = v1_state_with_old_defaults(false);
    let r: Value = serde_json::from_str(&call(&a.state, "GET", "/api/state/migrate", "{}", &a.ctx())).unwrap();
    assert_eq!(r["body"]["version"], 8);
    assert_eq!(r["state"]["gym"]["templates"].as_array().unwrap().len(), 7);
    // customised: a renamed template -> keep everything, only bump the version
    let mut v: Value = serde_json::from_str(&v1_state_with_old_defaults(true)).unwrap();
    v["gym"]["templates"][0]["name"] = json!("My Push");
    a.state = v.to_string();
    let r: Value = serde_json::from_str(&call(&a.state, "GET", "/api/state/migrate", "{}", &a.ctx())).unwrap();
    assert_eq!(r["state"]["version"], 8);
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
    assert_eq!(imp["version"], 8);
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
fn per_day_work_hours() {
    let mut a = App::new(TODAY);
    let r = a.req("PUT", "/api/memory/work", json!({
        "weekday_hours": {"1": {"start": "10:00", "end": "20:00"}},
        "monthly_rules": [{"nth": 1, "weekday": 2, "start": "09:00", "end": "13:00"}, {"nth": -1, "weekday": 4, "start": "08:00", "end": "12:00"}]
    }));
    assert_eq!(r.0, 200);
    a.holidays = json!([{"date": "2026-11-18", "name": "Test Holiday"}]);
    let days = a.get("/api/days?start=2026-11-02&days=28");
    let by = |d: &str| days.as_array().unwrap().iter().find(|x| x["date"] == d).unwrap().clone();
    assert_eq!(by("2026-11-02")["work"], json!({"start": "10:00", "end": "17:00"}));
    assert_eq!(by("2026-11-03")["work"], json!({"start": "10:00", "end": "20:00"}));
    assert_eq!(by("2026-11-04")["work"], json!({"start": "09:00", "end": "13:00"}), "first Wednesday");
    assert_eq!(by("2026-11-11")["work"], json!({"start": "10:00", "end": "17:00"}), "second Wednesday");
    assert_eq!(by("2026-11-27")["work"], json!({"start": "08:00", "end": "12:00"}), "last Friday");
    assert_eq!(by("2026-11-20")["work"], json!({"start": "10:00", "end": "17:00"}));
    // holiday still wins over a rule
    a.holidays = json!([{"date": "2026-11-04", "name": "Test Holiday"}]);
    let days = a.get("/api/days?start=2026-11-04&days=1");
    assert!(days[0]["work"].is_null());
    // weekend override is ignored when it is not a work day
    a.req("PUT", "/api/memory/work", json!({"weekday_hours": {"5": {"start": "10:00", "end": "12:00"}}}));
    assert!(a.get("/api/days?start=2026-11-07&days=1")[0]["work"].is_null());
    // the AI context and template use the resolved hours
    a.req("PUT", "/api/memory/work", json!({"weekday_hours": {"1": {"start": "10:00", "end": "20:00"}}}));
    let b = a.get("/api/briefing?kind=morning&date=2026-11-03");
    assert!(b["user_prompt"].as_str().unwrap().contains("work: 10:00 to 20:00 (a fixed block"), "{}", b["user_prompt"]);
    let sys = a.get("/api/briefing?kind=morning&date=2026-11-11");
    assert!(!sys["user_prompt"].as_str().unwrap().contains("work: 10:00 to 20:00 (a fixed block"));
    // validation
    for bad in [
        json!({"weekday_hours": {"7": {"start": "10:00", "end": "12:00"}}}),
        json!({"weekday_hours": {"1": {"start": "12:00", "end": "10:00"}}}),
        json!({"weekday_hours": {"1": {"start": "9am", "end": "10:00"}}}),
        json!({"monthly_rules": [{"nth": 0, "weekday": 1, "start": "10:00", "end": "12:00"}]}),
        json!({"monthly_rules": [{"nth": 6, "weekday": 1, "start": "10:00", "end": "12:00"}]}),
        json!({"monthly_rules": [{"nth": 1, "weekday": 7, "start": "10:00", "end": "12:00"}]}),
        json!({"monthly_rules": [{"nth": 1, "weekday": 1, "start": "13:00", "end": "12:00"}]}),
    ] {
        assert_eq!(a.req("PUT", "/api/memory/work", bad).0, 400);
    }
    // profile codes accept the same fields
    let p = a.req("POST", "/api/profile/apply", json!({"v": 1, "updated": "2026-11-02", "work": {"weekday_hours": {"3": {"start": "11:00", "end": "19:00"}}}}));
    assert_eq!(p.0, 200, "{:?}", p.1);
    assert_eq!(a.get("/api/days?start=2026-11-05&days=1")[0]["work"], json!({"start": "11:00", "end": "19:00"}));
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
    assert_eq!(s["assistant_name"], "Miranda");
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
            assert!((360..=780).contains(&n), "{day} weather={with_weather}: {n} words\n{text}");
            assert!(!has_stats(&text), "{day}: {text}");
        }
    }
}

#[test]
fn morning_template_parts_are_separate_and_specific() {
    let weather = json!({"summary": "Light rain", "temp_max": 14, "temp_min": 8, "precip_prob": 80, "uv_max": 1.5});
    for with_weather in [true, false] {
        let mut a = App::with_sample(TODAY);
        for off in 0..14 {
            let day = fmt_date(d(TODAY) + chrono::Duration::days(off));
            a.today = day.clone();
            a.weather = if with_weather { json!({ day.clone(): weather }) } else { json!({}) };
            let b = a.get(&format!("/api/briefing?kind=morning&date={day}"));
            let p = &b["template_parts"];
            let s = |k: &str| p[k].as_str().unwrap_or("").to_string();
            let (greeting, aff, body) = (s("greeting"), s("affirmation"), s("briefing"));
            assert!(!greeting.is_empty() && words_in(&greeting) <= 12, "{greeting}");
            assert!(greeting.contains(&weekday_name(d(&day))) || greeting.contains("morning") || greeting.contains("Morning"), "{greeting}");
            assert_eq!(p["weather_comment"].is_null(), !with_weather, "{day}");
            if with_weather {
                assert!(s("weather_comment").contains("14"), "{}", s("weather_comment"));
            }
            // the body has no greeting and no weather
            for w in ["degrees", "umbrella", "sun cream", "forecast", "Good morning", "Rise and shine", "Happy "] {
                assert!(!body.contains(w), "{day}: body contains {w:?}\n{body}");
            }
            // affirmation: short, specific (names a task from the plan), no stats, no cliches
            let titles: Vec<String> = a.get(&format!("/api/days?start={day}&days=1"))[0]["items"].as_array().unwrap().iter().map(|i| i["title"].as_str().unwrap().to_string()).collect();
            assert!(titles.iter().any(|t| aff.to_lowercase().contains(&t.to_lowercase()) || aff.contains(crate::taskvoice::task_phrase(t).text())), "{day}: affirmation names no task: {aff} / {titles:?}");
            assert!(words_in(&aff) <= 60, "{aff}");
            for part in [&greeting, &aff, &body, &s("weather_comment")] {
                assert!(!has_stats(part), "{part}");
            }
            for c in ["you've got this", "believe in yourself", "you are enough"] {
                assert!(!aff.to_lowercase().contains(c));
            }
            // template_text is the parts joined
            let joined = [greeting.clone(), s("weather_comment"), aff.clone(), body.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n\n");
            assert_eq!(b["template_text"].as_str().unwrap(), joined);
        }
    }
    let mut a = App::with_sample(TODAY);
    assert!(a.get("/api/briefing?kind=evening")["template_parts"].is_null());
    let sp = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"))["system_prompt"].as_str().unwrap().to_string();
    assert!(sp.contains("affirmation") && sp.contains("WITHOUT any weather"));
}

#[test]
fn morning_template_prefers_meaningful_fixed_points() {
    let mut a = App::with_sample(TODAY);
    let text = morning(&mut a, TODAY);
    assert!(text.contains("team stand-up at 10:15 AM"), "{text}");
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
    assert_eq!(b["max_words"], 760);
    assert!(b["cache_key"].as_str().unwrap().starts_with("briefing:morning:2026-11-02:"));
    assert!(b["user_prompt"].as_str().unwrap().contains("Plan tomorrow"));
    assert_eq!(b["stats"]["total"], 7);
    assert_eq!(b["stats"]["done"], 0);
    assert!(b["affirmation"].as_str().unwrap().len() > 10);
    println!("{text}");
}

#[test]
fn briefing_cache_key_morning_ignores_done_state_evening_tracks_it() {
    let mut a = App::with_sample(TODAY);
    let k1 = a.get("/api/briefing?kind=morning")["cache_key"].clone();
    assert_eq!(a.get("/api/briefing?kind=morning")["cache_key"], k1);
    let day = a.get("/api/days?start=2026-11-02&days=1");
    let tid = day[0]["items"][1]["task_id"].clone();
    let ev1 = a.get("/api/briefing?kind=evening")["cache_key"].clone();
    a.post("/api/occurrences/toggle", json!({"task_id": tid, "date": TODAY, "done": true}));
    assert_eq!(a.get("/api/briefing?kind=morning")["cache_key"], k1);
    assert_ne!(a.get("/api/briefing?kind=evening")["cache_key"], ev1);
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
    assert!(n["template_text"].as_str().unwrap().contains("team stand-up"), "{}", n["template_text"]);
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

#[test]
fn old_default_rotations_become_push_pull_cardio_rest_legs_keeping_next_workout() {
    let names = |st: &State| -> Vec<String> {
        st.gym.rotation.items.iter().map(|i| if i.rest { "Rest".into() } else { st.gym.templates.iter().find(|t| Some(t.id) == i.template_id).unwrap().name.clone() }).collect()
    };
    let fresh: State = serde_json::from_str(&init_state(TODAY)).unwrap();
    let by = |n: &str| crate::model::RotItem { template_id: fresh.gym.templates.iter().find(|t| t.name == n).map(|t| t.id), rest: false };
    let old8: Vec<_> = ["Push 1", "Pull 1", "Legs + Abs 1", "Cardio", "Push 2", "Pull 2", "Legs + Abs 2", "Cardio"].iter().map(|n| by(n)).collect();
    // v2 (8 items), next = Legs + Abs 1
    let mut v2 = fresh.clone();
    v2.version = 2;
    v2.gym.rotation.items = old8.clone();
    v2.gym.rotation.next_index = 2;
    assert!(crate::seed::migrate(&mut v2, TODAY));
    assert_eq!(v2.version, 8);
    assert_eq!(names(&v2), ["Push 1", "Pull 1", "Cardio", "Rest", "Legs + Abs 1", "Push 2", "Pull 2", "Cardio", "Rest", "Legs + Abs 2"]);
    assert_eq!(v2.gym.rotation.next_index, 4, "still Legs + Abs 1 next");
    // v3 (8 + rest), next = the rest day
    let mut v3 = fresh.clone();
    v3.version = 3;
    v3.gym.rotation.items = old8.clone();
    v3.gym.rotation.items.push(crate::model::RotItem { template_id: None, rest: true });
    v3.gym.rotation.next_index = 8;
    assert!(crate::seed::migrate(&mut v3, TODAY));
    assert_eq!(v3.gym.rotation.items.len(), 10);
    assert_eq!(v3.gym.rotation.next_index, 3);
    // a customised rotation is left alone
    let mut custom = fresh.clone();
    custom.version = 3;
    custom.gym.rotation.items = old8;
    custom.gym.rotation.items.swap(0, 1);
    assert!(crate::seed::migrate(&mut custom, TODAY));
    assert_eq!(custom.gym.rotation.items.len(), 8);
}

#[test]
fn v6_migration_renames_only_the_default_assistant() {
    let fresh: State = serde_json::from_str(&init_state(TODAY)).unwrap();
    let mut a = fresh.clone();
    a.version = 5;
    a.settings.assistant_name = "Sunny".into();
    assert!(crate::seed::migrate(&mut a, TODAY));
    assert_eq!(a.settings.assistant_name, "Miranda");
    let mut b = fresh.clone();
    b.version = 5;
    b.settings.assistant_name = "Poppy".into();
    assert!(crate::seed::migrate(&mut b, TODAY));
    assert_eq!(b.settings.assistant_name, "Poppy");
}

// ---- gym: missed days, skip-rest, skip

fn set_next(a: &mut App, index: usize, since: &str) {
    let mut st = a.st();
    st.gym.rotation.next_index = index;
    st.gym.next_since = Some(since.into());
    st.gym.rest_taken_date = None;
    a.state = serde_json::to_string(&st).unwrap();
}

#[test]
fn missed_days_never_advance_a_workout() {
    let mut a = App::new("2026-11-10");
    set_next(&mut a, 0, "2026-11-01");
    let r = call_typed(&a.state, "GET", "/api/gym/next", "", &a.ctx());
    assert!(r.state.is_none());
    assert_eq!(r.body["name"], "Push 1");
    assert_eq!(a.st().gym.rotation.next_index, 0);
}

#[test]
fn rest_next_since_yesterday_auto_advances_and_returns_state() {
    let mut a = App::new("2026-11-10");
    set_next(&mut a, 3, "2026-11-09");
    let r = call_typed(&a.state, "GET", "/api/gym/next", "", &a.ctx());
    assert_eq!(r.body["name"], "Legs + Abs 1");
    let st = r.state.expect("auto-advance returns the state");
    assert_eq!(st.gym.rotation.next_index, 4);
    assert_eq!(st.gym.next_since.as_deref(), Some("2026-11-10"));
    assert_eq!(st.gym.rest_taken_date.as_deref(), Some("2026-11-09"), "yesterday was the rest");
    a.state = serde_json::to_string(&st).unwrap();
    // the missed day was the rest: today goes straight to the next workout
    let days = a.get("/api/days?start=2026-11-10&days=2");
    assert_eq!(days[0]["workout"]["name"], "Legs + Abs 1");
    assert_eq!(days[1]["workout"]["name"], "Push 2");
    // stable on a second call
    assert!(call_typed(&a.state, "GET", "/api/gym/next", "", &a.ctx()).state.is_none());
}

#[test]
fn rest_next_since_today_does_not_advance_and_old_states_get_next_since() {
    let mut a = App::new("2026-11-10");
    set_next(&mut a, 3, "2026-11-10");
    let r = call_typed(&a.state, "GET", "/api/gym/next", "", &a.ctx());
    assert!(r.state.is_none());
    assert_eq!(r.body["rest"], true);
    // a v5 state without next_since is stamped, not advanced
    let mut st = a.st();
    st.gym.next_since = None;
    a.state = serde_json::to_string(&st).unwrap();
    let r = call_typed(&a.state, "GET", "/api/gym/next", "", &a.ctx());
    assert_eq!(r.state.unwrap().gym.next_since.as_deref(), Some("2026-11-10"));
    assert_eq!(r.body["rest"], true);
}

#[test]
fn consecutive_rests_need_one_passed_day_each() {
    let mut a = App::new("2026-11-10");
    let mut st = a.st();
    st.gym.rotation.items = vec![
        crate::model::RotItem { template_id: None, rest: true },
        crate::model::RotItem { template_id: None, rest: true },
        crate::model::RotItem { template_id: st.gym.templates.first().map(|t| t.id), rest: false },
    ];
    st.gym.rotation.next_index = 0;
    st.gym.next_since = Some("2026-11-09".into());
    a.state = serde_json::to_string(&st).unwrap();
    a.get("/api/gym/next");
    assert_eq!(a.st().gym.rotation.next_index, 1, "only one day passed");
    let mut st = a.st();
    st.gym.rotation.next_index = 0;
    st.gym.next_since = Some("2026-11-08".into());
    a.state = serde_json::to_string(&st).unwrap();
    assert_eq!(a.get("/api/gym/next")["name"], "Push 1");
    assert_eq!(a.st().gym.rotation.next_index, 2);
}

#[test]
fn skip_rest_makes_today_the_workout() {
    let mut a = App::new("2026-11-10");
    set_next(&mut a, 3, "2026-11-10");
    assert_eq!(a.get("/api/days?start=2026-11-10&days=1")[0]["workout"]["rest"], true);
    a.post("/api/gym/skip-rest", json!({}));
    assert_eq!(a.get("/api/gym/next")["name"], "Legs + Abs 1");
    assert_eq!(a.get("/api/days?start=2026-11-10&days=2")[0]["workout"]["name"], "Legs + Abs 1");
    assert_eq!(a.st().gym.next_since.as_deref(), Some("2026-11-10"));
    assert!(a.st().gym.rest_taken_date.is_none());
    // next is a workout now, so skip-rest is a 400
    assert_eq!(a.req("POST", "/api/gym/skip-rest", json!({})).0, 400);
}

#[test]
fn skip_advances_past_a_workout_or_rest_without_logging() {
    let mut a = App::new("2026-11-10");
    set_next(&mut a, 0, "2026-11-01");
    a.post("/api/gym/skip", json!({}));
    assert_eq!(a.get("/api/gym/next")["name"], "Pull 1");
    assert_eq!(a.get("/api/days?start=2026-11-10&days=1")[0]["workout"]["name"], "Pull 1");
    assert!(a.st().gym.sessions.is_empty());
    assert_eq!(a.st().gym.next_since.as_deref(), Some("2026-11-10"));
    set_next(&mut a, 3, "2026-11-10");
    a.post("/api/gym/skip", json!({}));
    assert_eq!(a.get("/api/gym/next")["name"], "Legs + Abs 1");
}

#[test]
fn hot_rain_is_never_the_kind_of_weather_they_like() {
    let st: State = serde_json::from_str(&init_state(TODAY)).unwrap();
    let w = crate::model::Weather { summary: "Light rain".into(), temp_max: 27, temp_min: 20, precip_prob: Some(80), uv_max: Some(2.0) };
    for i in 0..10 {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 1 + i).unwrap();
        let info = crate::tasks::DayInfo::default();
        let c = crate::digest::weather_comment_template(&w, &st.memory, &[], &None, &info, d);
        assert!(!c.contains("you like") && !c.contains("suits you") && !c.contains("don't mind"), "{c}");
        assert!(c.contains("27"), "{c}");
    }
    assert!(crate::memory::context_block(&st.memory).contains("Temperature outranks rain"));
}

#[test]
fn weather_lines_never_narrate_preferences() {
    let st: State = serde_json::from_str(&init_state(TODAY)).unwrap();
    let info = crate::tasks::DayInfo::default();
    for (summary, max) in [("Light rain", 14), ("Light rain", 24), ("Light rain", 28), ("Clear sky", 15), ("Clear sky", 20), ("Clear sky", 24), ("Clear sky", 30), ("Snow", 1)] {
        let w = crate::model::Weather { summary: summary.into(), temp_max: max, temp_min: max - 6, precip_prob: Some(10), uv_max: Some(5.0) };
        for i in 0..7 {
            let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 1 + i).unwrap();
            let c = crate::digest::weather_comment_template(&w, &st.memory, &[], &None, &info, d).to_lowercase();
            for bad in ["for you", "you like", "you enjoy", "comfort zone", "suits you", "you don't mind", "sweet spot", "you find"] {
                assert!(!c.contains(bad), "{bad:?} in {c}");
            }
        }
    }
}

fn cat_id(a: &mut App, name: &str) -> Value {
    a.get("/api/categories").as_array().unwrap().iter().find(|c| c["name"] == name).unwrap()["id"].clone()
}

#[test]
fn missed_untimed_rolls_timed_and_appointments_are_missed() {
    let mut a = App::new(TODAY);
    let appt = cat_id(&mut a, "Appointments");
    let plain = a.post("/api/tasks", json!({"title": "Pay rent", "date": "2026-10-30", "category_id": null}));
    let timed = a.post("/api/tasks", json!({"title": "Call bank", "date": "2026-10-30", "time": "10:00", "category_id": null}));
    let dent = a.post("/api/tasks", json!({"title": "Dentist", "date": "2026-10-29", "category_id": appt}));
    let rec = a.post("/api/tasks", json!({"title": "Stretch", "time": "07:00", "recurrence": {"kind": "daily"}, "category_id": null}));
    let m = a.get("/api/missed");
    let m = m.as_array().unwrap();
    assert_eq!(m.len(), 2);
    assert_eq!(m[0]["title"], "Dentist");
    assert_eq!(m[0]["date"], "2026-10-29");
    assert_eq!(m[1]["time"], "10:00");
    assert_eq!(m[1]["category"], Value::Null);
    assert_eq!(a.get(&format!("/api/tasks/{}", plain["id"]))["date"], TODAY);
    assert_eq!(a.get(&format!("/api/tasks/{}", timed["id"]))["date"], "2026-10-30");
    assert_eq!(a.get(&format!("/api/tasks/{}", dent["id"]))["date"], "2026-10-29");
    assert_eq!(a.get(&format!("/api/tasks/{}", rec["id"]))["date"], Value::Null);
    // the days view does not carry them over
    let day = a.get("/api/days?start=2026-11-02&days=1");
    let titles: Vec<&str> = day[0]["items"].as_array().unwrap().iter().map(|o| o["title"].as_str().unwrap()).collect();
    assert_eq!(titles, vec!["Stretch", "Pay rent"]);
    assert_eq!(a.st().version, 8);
    assert_eq!(a.st().missed.len(), 2);
}

#[test]
fn missed_move_to_today_dismiss_edit_and_delete_clear() {
    let mut a = App::new(TODAY);
    let mk = |a: &mut App, t: &str| a.post("/api/tasks", json!({"title": t, "date": "2026-10-30", "time": "10:00", "category_id": null}));
    let (x, y, z, w) = (mk(&mut a, "X"), mk(&mut a, "Y"), mk(&mut a, "Z"), mk(&mut a, "W"));
    assert_eq!(a.get("/api/missed").as_array().unwrap().len(), 4);
    // move to today keeps the time
    let o = a.post(&format!("/api/missed/{}/today", x["id"]), json!({}));
    assert_eq!(o["date"], TODAY);
    assert_eq!(o["time"], "10:00");
    assert_eq!(a.get(&format!("/api/tasks/{}", x["id"]))["date"], TODAY);
    // dismiss: done on the original date
    a.post(&format!("/api/missed/{}/dismiss", y["id"]), json!({}));
    assert_eq!(a.get(&format!("/api/tasks/{}", y["id"]))["date"], "2026-10-30");
    let day = a.get("/api/days?start=2026-10-30&days=1");
    assert_eq!(day[0]["items"][0]["done"], true);
    // editing the date to today or later clears it
    let mut body = a.get(&format!("/api/tasks/{}", z["id"]));
    body["date"] = json!("2026-11-05");
    body["time"] = json!("14:00");
    body["category_id"] = Value::Null;
    assert_eq!(a.req("PUT", &format!("/api/tasks/{}", z["id"]), body).0, 200);
    // delete clears
    a.req("DELETE", &format!("/api/tasks/{}", w["id"]), json!({}));
    assert!(a.get("/api/missed").as_array().unwrap().is_empty());
    assert!(a.st().missed.is_empty());
    assert_eq!(a.req("POST", &format!("/api/missed/{}/today", x["id"]), json!({})).0, 404);
}

#[test]
fn morning_template_mentions_a_missed_appointment() {
    let mut a = App::new(TODAY);
    let appt = cat_id(&mut a, "Appointments");
    a.post("/api/tasks", json!({"title": "Dentist", "date": "2026-10-29", "category_id": appt}));
    let b = a.get("/api/briefing?kind=morning");
    assert!(b["template_text"].as_str().unwrap().contains("Dentist from Thursday is waiting for a new time in Tasks."), "{}", b["template_text"]);
    assert!(b["user_prompt"].as_str().unwrap().contains("Missed and still waiting"));
    let mut c = App::new(TODAY);
    assert!(!c.get("/api/briefing?kind=morning")["template_text"].as_str().unwrap().contains("waiting for"));
}

// ------------------------------------------------------------ spoken-prose heuristics

const FINITE: &[&str] = &[
    "is", "are", "was", "were", "will", "have", "has", "had", "got", "do", "does", "did", "can", "could", "should", "would", "might", "may", "be", "been",
    "start", "take", "keep", "wear", "make", "try", "go", "get", "see", "need", "land", "lands", "fit", "fits", "look", "looks", "feel", "feels", "sounds", "waiting", "rolled", "moved", "let",
    "enjoy", "finish", "head", "plan", "pick", "book", "pay", "call", "bring", "grab", "give", "come", "comes", "stay", "rest", "wrap", "catch", "begin", "stack", "counts", "feeds", "ties",
    "helps", "opens", "carries", "wait", "fitted", "made", "hit", "leave", "left", "use", "put", "dim", "ease", "eases", "settle", "protect", "build", "run", "clear", "shout", "sleep", "sorted",
    "did", "know", "wrapped", "rise", "say", "happen", "stood", "loved", "mean", "means", "hope", "think", "choose", "drink", "dress", "layer", "listen", "go", "doing", "getting", "ended", "remember",
    "stayed", "snowed", "poured", "going",
];
const CONTRACTION_BASES: &[&str] = &["it", "that", "there", "here", "what", "let", "he", "she", "who", "today", "tomorrow", "everything", "nothing", "one"];

fn has_finite_verb(sentence: &str) -> bool {
    sentence.to_lowercase().split(|c: char| !(c.is_alphanumeric() || c == '\'')).any(|t| {
        FINITE.contains(&t)
            || t.strip_suffix("es").is_some_and(|b| FINITE.contains(&b))
            || t.strip_suffix('s').is_some_and(|b| FINITE.contains(&b))
            || t.ends_with("n't")
            || ["'re", "'ve", "'ll", "'d", "'m"].iter().any(|s| t.ends_with(s))
            || t.strip_suffix("'s").is_some_and(|b| CONTRACTION_BASES.contains(&b))
    })
}

fn sentences(text: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '\n' {
            out.push(std::mem::take(&mut cur));
            continue;
        }
        cur.push(*c);
        if matches!(c, '.' | '!' | '?') && chars.get(i + 1).map_or(true, |n| n.is_whitespace()) {
            out.push(std::mem::take(&mut cur));
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// Greetings and sign-offs ("Good morning, Sam!") are fine as short interjections.
fn is_interjection(s: &str) -> bool {
    let l = s.to_lowercase();
    s.split_whitespace().count() <= 4 && ["good ", "morning", "hey", "hi ", "hi,", "sleep ", "goodnight", "rest up"].iter().any(|p| l.starts_with(p))
}

fn assert_spoken(label: &str, text: &str) {
    for bad in ["°", "%", " / ", "&"] {
        assert!(!text.contains(bad), "{label}: {bad:?} in {text}");
    }
    for s in sentences(text) {
        if s.split_whitespace().count() >= 3 && !is_interjection(&s) {
            assert!(has_finite_verb(&s), "{label}: fragment {s:?} in\n{text}");
        }
    }
}

#[test]
fn heuristic_catches_headline_fragments() {
    for frag in ["Hot and muggy today, 27 degrees.", "Light clothes, water close, and an umbrella.", "A good backdrop for the walk.", "Lovely and cool today, 14 degrees at most."] {
        assert!(!has_finite_verb(frag), "{frag}");
    }
    for ok in ["It's going to be hot and muggy today, getting up to 27 degrees.", "I'd wear something light and keep some water with you.", "It should be a nice day for the walk."] {
        assert!(has_finite_verb(ok), "{ok}");
    }
}

#[test]
fn all_template_parts_are_complete_spoken_sentences() {
    let weathers = [
        ("Light rain", 14, 8),
        ("Light rain", 28, 20),
        ("Clear sky", 15, 8),
        ("Clear sky", 20, 12),
        ("Clear sky", 24, 15),
        ("Clear sky", 31, 22),
        ("Snow", 1, -3),
        ("Clear sky", 4, 0),
    ];
    for fresh in [false, true] {
        for (wi, (summary, hi, lo)) in weathers.iter().enumerate() {
            let mut a = if fresh { App::new(TODAY) } else { App::with_sample(TODAY) };
            a.holidays = json!([{"date": "2026-11-04", "name": "Test Holiday"}]);
            a.post("/api/days-off", json!({"date": "2026-11-05"}));
            if fresh {
                for (k, t, st, bl) in [("read", "A History of Venice", "current", ""), ("watch", "Wolf Hall", "someday", "It's a drama about Thomas Cromwell at the court of Henry the Eighth. It's slow and rich."), ("try", "The old tea house", "someday", "I don't know much about this one yet.")] {
                    a.post("/api/leisure", json!({"kind": k, "title": t, "status": st, "blurb": bl}));
                }
                // a missed item and a quiet week
                let appt = cat_id(&mut a, "Appointments");
                a.post("/api/tasks", json!({"title": "Dentist", "date": "2026-10-29", "category_id": appt}));
            }
            for off in 0..14 {
                let day = fmt_date(d(TODAY) + chrono::Duration::days(off));
                a.today = day.clone();
                a.weather = json!({ day.clone(): {"summary": summary, "temp_max": hi, "temp_min": lo, "precip_prob": 10, "uv_max": 5.0} });
                let label = format!("fresh={fresh} {day} {summary} {hi}");
                let m = a.get(&format!("/api/briefing?kind=morning&date={day}"));
                for k in ["greeting", "weather_comment", "affirmation", "briefing"] {
                    assert_spoken(&format!("{label} morning.{k}"), m["template_parts"][k].as_str().unwrap_or(""));
                }
                assert_spoken(&format!("{label} affirmation"), m["affirmation"].as_str().unwrap());
                if wi % 3 == 0 {
                    for kind in ["evening", "weekly"] {
                        let b = a.get(&format!("/api/briefing?kind={kind}&date={day}"));
                        assert_spoken(&format!("{label} {kind}"), b["template_text"].as_str().unwrap());
                        assert_spoken(&format!("{label} {kind} affirmation"), b["affirmation"].as_str().unwrap());
                    }
                    let dg = a.get(&format!("/api/digest?date={day}"));
                    assert_spoken(&format!("{label} overview"), dg["overview"].as_str().unwrap());
                }
            }
        }
    }
}

#[test]
fn morning_template_suggests_a_midday_slot_softly_without_the_times() {
    let mut a = App::with_sample(TODAY);
    let mut seen = 0;
    for off in 0..14 {
        let day = fmt_date(d(TODAY) + chrono::Duration::days(off));
        a.today = day.clone();
        let b = a.get(&format!("/api/briefing?kind=morning&date={day}"));
        let body = b["template_parts"]["briefing"].as_str().unwrap().to_string();
        if body.contains("working from") {
            seen += 1;
            assert!(body.contains("middle of the day"), "{day}: {body}");
            assert!(!body.contains("between 12") && !body.contains("lunch break, sometime") && !body.contains("12 and 2"), "{day}: {body}");
        }
        let up = b["user_prompt"].as_str().unwrap();
        assert!(up.contains("usually sometime between 12 and 2") && up.contains("never state or recite"), "{up}");
    }
    assert!(seen > 3);
}

#[test]
fn lunch_window_defaults_validates_and_saves() {
    let mut a = App::new(TODAY);
    let w = a.get("/api/memory")["work"].clone();
    assert_eq!((w["lunch_window_start"].as_str(), w["lunch_window_end"].as_str()), (Some("12:00"), Some("14:00")));
    // an old state without the fields gets the defaults
    let mut st: Value = serde_json::from_str(&a.state).unwrap();
    st["memory"]["work"].as_object_mut().unwrap().remove("lunch_window_start");
    st["memory"]["work"].as_object_mut().unwrap().remove("lunch_window_end");
    a.state = st.to_string();
    assert_eq!(a.st().memory.work.lunch_window_end, "14:00");
    assert_eq!(a.req("PUT", "/api/memory/work", json!({"lunch_window_start": "14:00", "lunch_window_end": "13:00"})).0, 400);
    assert_eq!(a.req("PUT", "/api/memory/work", json!({"lunch_window_start": "noon"})).0, 400);
    let (s, b) = a.req("PUT", "/api/memory/work", json!({"lunch_window_start": "11:30", "lunch_window_end": "13:30"}));
    assert_eq!(s, 200);
    assert_eq!(b["work"]["lunch_window_start"], "11:30");
    let up = a.get("/api/briefing?kind=morning")["user_prompt"].as_str().unwrap().to_string();
    assert!(up.contains("between 11:30 and 1:30"), "{up}");
}

// ------------------------------------------------------------ leisure

fn leisure_line(b: &Value) -> String {
    b["user_prompt"].as_str().unwrap().lines().find(|l| l.starts_with("Computed leisure suggestion")).unwrap_or("").to_string()
}

fn add_leisure(a: &mut App, kind: &str, title: &str, status: &str) -> Value {
    a.post("/api/leisure", json!({"kind": kind, "title": title, "status": status}))
}

#[test]
fn leisure_crud_and_validation() {
    let mut a = App::new(TODAY);
    assert_eq!(a.req("POST", "/api/leisure", json!({"kind": "dance", "title": "x"})).0, 400);
    assert_eq!(a.req("POST", "/api/leisure", json!({"kind": "read", "title": "  "})).0, 400);
    assert_eq!(a.req("POST", "/api/leisure", json!({"kind": "read", "title": "x", "status": "done"})).0, 400);
    let l = add_leisure(&mut a, "read", "Book A", "someday");
    assert_eq!(l["status"], "someday");
    assert_eq!(l["blurb"], "");
    let id = l["id"].as_i64().unwrap();
    let (s, u) = a.req("PUT", &format!("/api/leisure/{id}"), json!({"kind": "read", "title": "Book A", "status": "finished", "blurb": "It's a book."}));
    assert_eq!((s, u["finished_on"].as_str(), u["blurb"].as_str()), (200, Some(TODAY), Some("It's a book.")));
    // a PUT without blurb keeps it
    let (_, u) = a.req("PUT", &format!("/api/leisure/{id}"), json!({"kind": "read", "title": "Book A", "status": "current"}));
    assert_eq!((u["finished_on"].clone(), u["blurb"].as_str()), (Value::Null, Some("It's a book.")));
    add_leisure(&mut a, "watch", "Show", "someday");
    let list = a.get("/api/leisure");
    assert_eq!(list[0]["title"], "Book A");
    assert_eq!(a.req("DELETE", &format!("/api/leisure/{id}"), json!({})).0, 200);
    assert_eq!(a.req("DELETE", &format!("/api/leisure/{id}"), json!({})).0, 404);
    assert_eq!(a.get("/api/leisure").as_array().unwrap().len(), 1);
}

#[test]
fn leisure_state_v8_interests_and_sample_data() {
    let a = App::new(TODAY);
    assert!(a.st().memory.interests.starts_with("History and politics"));
    let mut st = a.st();
    st.version = 7;
    st.memory.interests = String::new();
    assert!(crate::seed::migrate(&mut st, TODAY));
    assert_eq!((st.version, st.memory.interests.starts_with("History and politics")), (8, true));
    let mut b = App::new(TODAY);
    let (s, m) = b.req("PUT", "/api/memory/interests", json!({"text": " Birds and maps "}));
    assert_eq!((s, m["interests"].as_str()), (200, Some("Birds and maps")));
    assert!(b.get("/api/briefing?kind=morning")["user_prompt"].as_str().unwrap().contains("Interests"));
    let c = App::with_sample(TODAY);
    let l = c.st().leisure;
    assert_eq!(l.len(), 4);
    assert!(l.iter().all(|x| !x.blurb.is_empty()));
}

#[test]
fn leisure_suggested_only_with_free_room() {
    // a packed work day: no suggestion in the morning (the evening of a normal work day does get one)
    let mut a = App::with_sample(TODAY);
    for kind in ["morning"] {
        let b = a.get(&format!("/api/briefing?kind={kind}"));
        assert!(leisure_line(&b).ends_with("none today"), "{kind}: {}", leisure_line(&b));
        for t in ["Piranesi", "Slow Horses", "Stardew", "Crown Street"] {
            assert!(!b["template_text"].as_str().unwrap().contains(t), "{kind} {t}");
        }
    }
    assert!(a.st().leisure.iter().all(|l| l.last_suggested.is_none()));
    assert!(!leisure_line(&a.get("/api/briefing?kind=evening")).ends_with("none today"));
    // a free Saturday gets exactly one
    let mut f = App::new("2026-11-07");
    add_leisure(&mut f, "read", "Book A", "someday");
    let b = f.get("/api/briefing?kind=morning");
    assert!(leisure_line(&b).contains("Book A"), "{}", leisure_line(&b));
    assert!(b["template_text"].as_str().unwrap().contains("Book A"));
    assert!(b["user_prompt"].as_str().unwrap().contains("Free room for leisure today: yes"));
    // a day off / holiday on a work day is free; a timed evening commitment is not
    let mut e = App::new("2026-11-03");
    add_leisure(&mut e, "watch", "Show", "someday");
    let appt = cat_id(&mut e, "Social");
    e.post("/api/tasks", json!({"title": "Dinner out", "date": "2026-11-03", "time": "19:00", "category_id": appt.clone()}));
    // one timed evening item is still a normal evening: suggest
    assert!(leisure_line(&e.get("/api/briefing?kind=evening")).contains("Show"));
    // a second timed item from 18:00 makes it packed
    e.post("/api/tasks", json!({"title": "Book club", "date": "2026-11-03", "time": "19:30", "category_id": appt.clone()}));
    assert!(leisure_line(&e.get("/api/briefing?kind=evening")).ends_with("none today"));
    e.post("/api/days-off", json!({"date": "2026-11-04"}));
    e.today = "2026-11-04".into();
    add_leisure(&mut e, "watch", "Other Show", "someday"); // "Show" was suggested yesterday evening
    assert!(leisure_line(&e.get("/api/briefing?kind=morning&date=2026-11-04")).contains("Other Show"));
}

#[test]
fn leisure_prefers_current_and_phrases_it_as_carrying_on() {
    let mut a = App::new("2026-11-07");
    add_leisure(&mut a, "watch", "Some Film", "someday");
    add_leisure(&mut a, "read", "Big Novel", "current");
    let m = a.get("/api/briefing?kind=morning")["template_text"].as_str().unwrap().to_string();
    assert!(m.contains("a few more chapters of Big Novel"), "{m}");
    assert!(!m.contains("Some Film"));
    let e = a.get("/api/briefing?kind=evening")["template_text"].as_str().unwrap().to_string();
    assert!(e.contains("curl up and read a few more chapters of Big Novel"), "{e}");
    // stable on repeat and recorded once
    let again = a.get("/api/briefing?kind=morning")["template_text"].as_str().unwrap().to_string();
    assert_eq!(m, again);
    assert_eq!(a.st().leisure.iter().filter(|l| l.last_suggested.as_deref() == Some("2026-11-07")).count(), 1);
}

#[test]
fn leisure_rotation_skips_items_suggested_in_the_last_five_days() {
    let mut a = App::new("2026-11-07");
    for t in ["Alpha", "Beta", "Gamma"] {
        add_leisure(&mut a, "read", t, "someday");
    }
    let mut seen: Vec<String> = vec![];
    for day in ["2026-11-07", "2026-11-08", "2026-11-09"] {
        a.today = day.into();
        let line = leisure_line(&a.get(&format!("/api/briefing?kind=morning&date={day}")));
        let t = ["Alpha", "Beta", "Gamma"].iter().find(|t| line.contains(**t)).unwrap_or_else(|| panic!("{day}: {line}"));
        assert!(!seen.contains(&t.to_string()), "{t} repeated: {seen:?}");
        seen.push(t.to_string());
    }
    // all three are recent now: nothing for a few days, then they come back
    a.today = "2026-11-10".into();
    assert!(leisure_line(&a.get("/api/briefing?kind=morning&date=2026-11-10")).ends_with("none today"));
    a.today = "2026-11-13".into();
    assert!(!leisure_line(&a.get("/api/briefing?kind=morning&date=2026-11-13")).ends_with("none today"));
}

#[test]
fn leisure_weather_interests_and_finished() {
    let mut a = App::new("2026-11-07");
    a.weather = json!({"2026-11-07": {"summary": "Light rain", "temp_max": 14, "temp_min": 9, "precip_prob": 80, "uv_max": 2.0}});
    add_leisure(&mut a, "try", "The tea house", "someday");
    add_leisure(&mut a, "watch", "A Film", "someday");
    assert!(leisure_line(&a.get("/api/briefing?kind=morning")).contains("A Film"));
    // interests raise priority
    let mut b = App::new("2026-11-07");
    b.post("/api/leisure", json!({"kind": "read", "title": "Cooking for one", "status": "someday"}));
    b.post("/api/leisure", json!({"kind": "read", "title": "The Meiji restoration", "status": "someday", "blurb": "It's a history of Japan in the nineteenth century."}));
    assert!(leisure_line(&b.get("/api/briefing?kind=morning")).contains("Meiji"));
    // finished items are never suggested, only a light weekly highlight
    let mut c = App::new("2026-11-08");
    let l = add_leisure(&mut c, "watch", "Done Show", "someday");
    c.req("PUT", &format!("/api/leisure/{}", l["id"]), json!({"kind": "watch", "title": "Done Show", "status": "finished"}));
    assert_eq!(leisure_line(&c.get("/api/briefing?kind=morning")), "");
    let w = c.get("/api/briefing?kind=weekly");
    assert!(w["template_text"].as_str().unwrap().contains("you also finished Done Show this week"));
}

#[test]
fn blurb_prompt_endpoint() {
    let mut a = App::new(TODAY);
    let l = a.post("/api/leisure", json!({"kind": "read", "title": "The Count of Monte Cristo", "note": "from Sam"}));
    let b = a.get(&format!("/api/leisure/{}/blurb-prompt", l["id"]));
    let sys = b["system_prompt"].as_str().unwrap();
    assert!(sys.contains("UNKNOWN") && sys.contains("spoilers") && sys.contains("silent"), "{sys}");
    let usr = b["user_prompt"].as_str().unwrap();
    assert!(usr.contains("The Count of Monte Cristo") && usr.contains("from Sam") && usr.contains("History and politics"), "{usr}");
    assert_eq!(a.req("GET", "/api/leisure/9999/blurb-prompt", json!({})).0, 404);
}

#[test]
fn leisure_evening_suggested_on_a_normal_work_evening_not_a_packed_one() {
    let mut a = App::new("2026-11-03");
    add_leisure(&mut a, "watch", "Show", "someday");
    let t = cat_id(&mut a, "Social");
    a.post("/api/tasks", json!({"title": "Call mum", "date": "2026-11-03", "category_id": t.clone()}));
    assert!(leisure_line(&a.get("/api/briefing?kind=evening")).contains("Show"));
    a.post("/api/tasks", json!({"title": "Late film", "date": "2026-11-03", "time": "20:30", "category_id": t}));
    let mut p = App::new("2026-11-03");
    add_leisure(&mut p, "watch", "Show", "someday");
    let t2 = cat_id(&mut p, "Social");
    p.post("/api/tasks", json!({"title": "Late film", "date": "2026-11-03", "time": "20:30", "category_id": t2}));
    assert!(leisure_line(&p.get("/api/briefing?kind=evening")).ends_with("none today"));
}

#[test]
fn on_this_day_flag_is_deterministic_two_or_three_a_week_and_off_when_packed() {
    use chrono::{Duration, NaiveDate};
    let start = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
    for w in 0..12 {
        let n = (0..7).filter(|d| crate::leisure::on_this_day(start + Duration::days(w * 7 + d), &[])).count();
        assert!((2..=3).contains(&n), "week {w}: {n}");
    }
    let day = (0..7).map(|d| start + Duration::days(d)).find(|d| crate::leisure::on_this_day(*d, &[])).unwrap();
    assert_eq!(crate::leisure::on_this_day(day, &[]), crate::leisure::on_this_day(day, &[]));
    // prompt carries the flag and the rule; the template never mentions it
    let ds = day.format("%Y-%m-%d").to_string();
    let mut a = App::new(&ds);
    let b = a.get("/api/briefing?kind=morning");
    let up = b["user_prompt"].as_str().unwrap();
    assert!(up.contains("On this day: yes"), "{up}");
    assert!(b["system_prompt"].as_str().unwrap().contains("certain"));
    assert!(!b["template_text"].as_str().unwrap().to_lowercase().contains("on this day"));
    // a packed morning turns it off
    let cat = cat_id(&mut a, "Social");
    for (i, t) in ["09:00", "11:00", "13:00"].iter().enumerate() {
        a.post("/api/tasks", json!({"title": format!("Meet {i}"), "date": ds, "time": t, "category_id": cat.clone()}));
    }
    assert!(a.get("/api/briefing?kind=morning")["user_prompt"].as_str().unwrap().contains("On this day: no"));
}

#[test]
fn prompts_say_to_suggest_only_listed_works() {
    let mut a = App::new(TODAY);
    add_leisure(&mut a, "watch", "Show", "someday");
    for kind in ["morning", "evening"] {
        let up = a.get(&format!("/api/briefing?kind={kind}"))["user_prompt"].as_str().unwrap().to_string();
        assert!(up.contains("never introduce new titles"), "{kind}");
        assert!(up.contains("Chinese, Korean or Japanese"), "{kind}");
    }
}

#[test]
fn private_profile_apply_and_clear() {
    let mut a = App::new(TODAY);
    a.post("/api/goals", json!({"title": "Learn pottery", "horizon": "medium"}));
    let body = json!({
        "v": 1, "updated": "2026-11-03", "interests": "Pottery and tide pools",
        "notes": ["Example note one", "Example note two"],
        "work": {"enabled": true, "days": [1, 2, 3], "start": "08:30", "end": "16:00", "lunch_minutes": 30, "lunch_window_start": "12:00", "lunch_window_end": "13:30", "holiday_region": "AU-VIC"},
        "weather": {"comfortable_max": 20, "hot_above": 24, "too_hot_above": 28, "likes_rain": false, "avoid_sunburn": false},
        "add_goals": [{"title": "learn POTTERY", "horizon": "long"}, {"title": "Run a 10k", "horizon": "medium", "why": "health", "deadline": "2027-03-01"}],
        "add_leisure": [{"kind": "read", "title": "A Made-up Book", "note": "n"}, {"kind": "watch", "title": "Sample Film", "status": "current"}]
    });
    let r = a.post("/api/profile/apply", body.clone());
    assert_eq!(r, json!({"interests": true, "notes": 2, "work": true, "weather": true, "goals_added": 1, "leisure_added": 2, "tasks_added": 0}));
    let m = a.get("/api/memory");
    assert_eq!((m["interests"].as_str(), m["profile_updated"].as_str(), m["notes"].as_array().unwrap().len()), (Some("Pottery and tide pools"), Some("2026-11-03"), 2));
    assert_eq!((m["work"]["start"].as_str(), m["work"]["days"].clone(), m["weather"]["hot_above"].as_f64()), (Some("08:30"), json!([1, 2, 3]), Some(24.0)));
    // replace semantics + duplicate skip on re-apply
    let r = a.post("/api/profile/apply", body);
    assert_eq!((r["notes"].as_i64(), r["goals_added"].as_i64(), r["leisure_added"].as_i64()), (Some(2), Some(0), Some(0)));
    assert_eq!(a.get("/api/memory")["notes"].as_array().unwrap().len(), 2);
    assert_eq!(a.st().goals.len(), 2);
    // absent sections are left alone
    let r = a.post("/api/profile/apply", json!({"v": 1, "updated": "2026-11-04"}));
    assert_eq!((r["interests"].clone(), r["work"].clone()), (json!(false), json!(false)));
    assert_eq!(a.get("/api/memory")["interests"], "Pottery and tide pools");
    // validation errors leave the state untouched
    let before = a.st().memory.profile_updated.clone();
    for bad in [
        json!({"v": 2, "updated": "2026-11-05"}),
        json!({"v": 1, "updated": "nope"}),
        json!({"v": 1, "updated": "2026-11-05", "interests": "changed", "work": {"start": "18:00", "end": "09:00"}}),
        json!({"v": 1, "updated": "2026-11-05", "notes": ["ok", "  "]}),
        json!({"v": 1, "updated": "2026-11-05", "weather": {"comfortable_max": 30, "hot_above": 20}}),
        json!({"v": 1, "updated": "2026-11-05", "add_goals": [{"title": "Fine goal", "horizon": "long"}, {"title": "Bad", "horizon": "weekly"}]}),
        json!({"v": 1, "updated": "2026-11-05", "add_leisure": [{"kind": "swim", "title": "X"}]}),
    ] {
        let (s, _) = a.req("POST", "/api/profile/apply", bad);
        assert_eq!(s, 400);
    }
    assert_eq!((a.st().memory.profile_updated, a.st().memory.interests.as_str(), a.st().goals.len()), (before, "Pottery and tide pools", 2));
    // clear
    a.post("/api/profile/clear", json!({}));
    let m = a.get("/api/memory");
    assert_eq!((m["interests"].as_str(), m["profile_updated"].as_str(), m["notes"].as_array().unwrap().len(), m["work"]["start"].as_str(), m["weather"]["hot_above"].as_f64()), (Some(""), Some(""), 0, Some("10:00"), Some(22.0)));
    assert_eq!(a.st().goals.len(), 2);
}

#[test]
fn profile_add_tasks() {
    let mut a = App::new(TODAY);
    let cat = a.st().categories[0].name.clone();
    let body = json!({"v": 1, "updated": "2026-11-03", "add_tasks": [
        {"title": "Example errand", "date": "2026-11-10", "time": "14:00", "category": cat.to_lowercase(), "notes": "n", "checklist": ["one", "two"]},
        {"title": "Book the dentist", "date": "2026-11-11", "category": "auto", "part_of_day": "morning"},
        {"title": "example ERRAND", "date": "2026-11-10"}
    ]});
    let r = a.post("/api/profile/apply", body.clone());
    assert_eq!(r["tasks_added"], 2);
    let st = a.st();
    assert_eq!(st.tasks.len(), 2);
    assert_eq!(st.tasks[0].checklist.len(), 2);
    assert_eq!(st.tasks[0].category_id, Some(st.categories[0].id));
    assert!(st.tasks[1].category_auto);
    assert_eq!(a.post("/api/profile/apply", body)["tasks_added"], 0);
    let (s, _) = a.req("POST", "/api/profile/apply", json!({"v": 1, "updated": "2026-11-05", "add_tasks": [{"title": "X", "date": "2026-11-12", "category": "Nope"}]}));
    assert_eq!(s, 400);
    assert_eq!(a.st().tasks.len(), 2);
}

// ------------------------------------------------------------ news

fn story(id: &str, source: &str, title: &str, lead: &str, published: &str, topics: &[&str]) -> Value {
    json!({"id": id, "source": source, "title": title, "summary": lead, "link": format!("https://example.com/{id}"), "published": published, "topics": topics})
}

/// A sample feed for the morning of 2026-11-02 (generated 05:45 Sydney = 2026-11-01T18:45Z).
fn morning_news() -> Value {
    json!([
        story("a1", "ABC News", "Sydney ferry services disrupted by storm damage", "Several Sydney ferry services were suspended on Sunday after storm damage to wharves. Transport for NSW says repairs will take days.", "2026-11-01T17:30:00Z", &["australia"]),
        story("a2", "The Guardian", "NSW government announces new rental reforms", "The NSW government has announced changes to rental laws. Tenants will get longer notice.", "2026-11-01T16:00:00Z", &["australia"]),
        story("a3", "SBS News", "Federal budget update due this week", "The treasurer will deliver a mid-year update on Thursday.", "2026-11-01T12:00:00Z", &["australia"]),
        story("w1", "BBC News", "Ceasefire talks resume in Geneva", "Negotiators returned to the table on Sunday after a week of stalled talks. Mediators described the mood as cautious.", "2026-11-01T17:00:00Z", &["world"]),
        story("w2", "Al Jazeera", "Talks resume in Geneva on ceasefire", "Negotiators returned to the table after a week of stalled talks.", "2026-11-01T17:10:00Z", &["world"]),
        story("w3", "The Guardian", "European leaders meet over energy prices", "Leaders gathered in Brussels to discuss winter energy supply.", "2026-11-01T15:00:00Z", &["world"]),
        story("e1", "NHK World", "Japan and South Korea agree to resume summit talks", "The two governments said leaders will meet next month. It would be the first summit in a year.", "2026-11-01T14:00:00Z", &["east-asia", "world"]),
        story("e2", "Yonhap", "South Korea's exports rise for a fifth month", "Exports were driven by chips and ships. Officials expect growth to continue.", "2026-11-01T13:00:00Z", &["east-asia"]),
        story("s1", "BBC News", "Scientists map a new deep-sea ecosystem", "Researchers say the vents host species never seen before.", "2026-11-01T11:00:00Z", &["science-tech"]),
        story("s2", "The Conversation", "How a new telescope will change what we know about galaxies", "The telescope begins observations next year.", "2026-11-01T10:00:00Z", &["science-tech", "australia"]),
        story("i1", "Yonhap", "Samsung and SK hynix lift chip output as demand for memory chips grows", "The two semiconductor makers plan more capacity. Analysts say the cycle is turning.", "2026-11-01T09:00:00Z", &["east-asia"]),
        story("i2", "BBC News", "Japan to fund rare earth magnet production to cut reliance on imports", "Tokyo will back domestic magnet makers. Rare earth supply from China has been uncertain.", "2026-11-01T08:00:00Z", &["world"]),
        story("l1", "ABC News", "Backyard bakers find a following at the Parramatta markets", "A group of home bakers has turned a weekend stall into a small business.", "2026-11-01T07:00:00Z", &["life", "australia"]),
        story("l2", "The Guardian", "The joy of a long lunch: why slowing down is back in style", "Restaurants report a rise in long, leisurely lunches.", "2026-11-01T06:00:00Z", &["life"]),
    ])
}

fn news_app(date: &str) -> App {
    let mut a = App::with_sample(date);
    a.news = morning_news();
    a.news_at = "2026-11-01T18:45:00Z".into();
    a
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()).collect()
}

fn record_news(a: &mut App, kind: &str, date: &str, items: &Value, at: &str) {
    a.post("/api/news/record", json!({"kind": kind, "date": date, "items": items, "generated_at": at}));
}

#[test]
fn news_morning_is_balanced_and_deduplicated() {
    let mut a = news_app(TODAY);
    let b = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    let n = &b["news"];
    let items = n["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "two stories, each told in full");
    assert!(items[0]["topics"].as_array().unwrap().iter().any(|t| t == "australia"), "Australia leads: {}", items[0]);
    // the same story from two outlets is never both picked
    let got = ids(&n["items"]);
    assert!(!(got.contains(&"w1".to_string()) && got.contains(&"w2".to_string())), "{got:?}");
    // at most two from one source
    for src in ["ABC News", "The Guardian", "BBC News"] {
        assert!(items.iter().filter(|i| i["source"] == src).count() <= 2, "{src}");
    }
    assert!(n["sources"].as_array().unwrap().len() >= 2);
    assert!(b["system_prompt"].as_str().unwrap().contains("NEWS:"));
    assert!(b["system_prompt"].as_str().unwrap().contains("EXACTLY as given"));
    let up = b["user_prompt"].as_str().unwrap();
    assert!(up.contains("Lead: "), "{up}");
    assert!(up.contains(items[0]["title"].as_str().unwrap()));
    // no news in the ctx -> no news and no news instructions
    let mut none = App::with_sample(TODAY);
    let b0 = none.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    assert!(b0["news"].is_null());
    assert!(!b0["system_prompt"].as_str().unwrap().contains("NEWS:"));
}

#[test]
fn news_template_contains_each_selected_headline_verbatim_and_only_given_titles() {
    let mut a = news_app(TODAY);
    let b = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    let t = b["news"]["template_text"].as_str().unwrap();
    let items = b["news"]["items"].as_array().unwrap();
    for i in items {
        assert!(t.contains(&format!("'{}'", i["title"].as_str().unwrap())), "{t}");
    }
    assert!(t.contains("First up, from "));
    assert!(t.contains("And last, from "));
    assert!(t.contains(", the headline is: '"));
    // the quoted headlines are exactly the selected ones, nothing else
    let quotes = t.matches(" '").count();
    assert_eq!(quotes, items.len(), "{t}");
    for pool in morning_news().as_array().unwrap() {
        let title = pool["title"].as_str().unwrap();
        if !items.iter().any(|i| i["title"] == pool["title"]) {
            assert!(!t.contains(title), "{title} was not selected");
        }
    }
    // and the whole morning template carries it, within the new longer range
    let full = b["template_text"].as_str().unwrap();
    for i in items {
        assert!(full.contains(i["title"].as_str().unwrap()));
    }
    let n = words_in(full);
    assert!((480..=900).contains(&n), "{n} words");
    assert!(!has_stats(&b["news"]["template_text"].to_string()));
}

#[test]
fn news_position_varies_by_day_for_morning_and_is_the_end_for_evening() {
    let mut seen = std::collections::BTreeSet::new();
    for off in 0..30 {
        let day = fmt_date(d(TODAY) + chrono::Duration::days(off));
        assert_eq!(crate::news::position(Kind::Evening, d(&day)), 2);
        seen.insert(crate::news::position(Kind::Morning, d(&day)));
    }
    assert_eq!(seen.len(), 2, "{seen:?}");
    assert!(seen.iter().all(|p| *p == 0 || *p == 2), "{seen:?}");
}

#[test]
fn news_compose_inserts_at_the_chosen_paragraph() {
    let body = "One.\n\nTwo.\n\nThree.\n\nClosing.";
    assert_eq!(crate::news::compose(body, "NEWS", 0), "NEWS\n\nOne.\n\nTwo.\n\nThree.\n\nClosing.");
    assert_eq!(crate::news::compose(body, "NEWS", 1), "One.\n\nNEWS\n\nTwo.\n\nThree.\n\nClosing.");
    assert_eq!(crate::news::compose(body, "NEWS", 2), "One.\n\nTwo.\n\nThree.\n\nNEWS\n\nClosing.");
    assert_eq!(crate::news::compose("Only.", "NEWS", 2), "Only.\n\nNEWS");
}

#[test]
fn news_evening_only_has_stories_after_the_morning_cutoff_and_never_repeats_the_morning() {
    let mut a = news_app(TODAY);
    let b = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    let morning_items = b["news"]["items"].clone();
    record_news(&mut a, "morning", TODAY, &morning_items, "2026-11-01T20:30:00Z");
    assert_eq!(a.st().news_morning_cutoff.get(TODAY).map(String::as_str), Some("2026-11-01T20:30:00Z"));
    // re-viewing the morning gives the same stories
    let again = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    assert_eq!(ids(&again["news"]["items"]), ids(&morning_items));
    assert_eq!(again["news"]["recorded"], true);
    // the evening feed has new items plus all the old ones
    let mut feed = morning_news().as_array().unwrap().clone();
    feed.push(story("n1", "ABC News", "Hunter Valley bushfire contained overnight", "Crews contained the fire on Monday afternoon. No homes were lost.", "2026-11-02T03:00:00Z", &["australia"]));
    feed.push(story("n2", "BBC News", "Markets close higher after central bank signal", "Shares rose after the bank signalled a pause.", "2026-11-02T04:00:00Z", &["world"]));
    feed.push(story("n3", "NHK World", "Typhoon heads for southern Japan", "The typhoon is expected to make landfall on Wednesday.", "2026-11-02T04:30:00Z", &["east-asia"]));
    feed.push(story("n4", "The Guardian", "New study links sleep to memory", "Researchers followed 200 adults for a year.", "2026-11-02T05:00:00Z", &["science-tech"]));
    a.news = json!(feed);
    a.news_at = "2026-11-02T05:45:00Z".into();
    let e = a.get(&format!("/api/briefing?kind=evening&date={TODAY}"));
    let ev = ids(&e["news"]["items"]);
    assert_eq!(ev.len(), 2, "{ev:?}");
    for id in &ev {
        assert!(id.starts_with('n'), "old story {id} in the evening: {ev:?}");
    }
    for id in ids(&morning_items) {
        assert!(!ev.contains(&id));
    }
    assert!(e["news"]["position"] == 2);
    assert!(e["system_prompt"].as_str().unwrap().contains("since this morning"));
    // the evening template carries the news before the goodnight
    let t = e["template_text"].as_str().unwrap();
    let first = e["news"]["items"][0]["title"].as_str().unwrap();
    assert!(t.contains(first));
    assert!(t.find(first).unwrap() < t.rfind("\n\n").unwrap());
    // with nothing new since the morning there is no news at all
    a.news = morning_news();
    a.news_at = "2026-11-01T18:45:00Z".into();
    let none = a.get(&format!("/api/briefing?kind=evening&date={TODAY}"));
    assert!(none["news"].is_null());
    assert!(!none["system_prompt"].as_str().unwrap().contains("NEWS:"));
}

#[test]
fn news_next_morning_excludes_the_previous_evening_and_morning_and_same_story_other_outlets() {
    let mut a = news_app(TODAY);
    let m = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"))["news"]["items"].clone();
    record_news(&mut a, "morning", TODAY, &m, "2026-11-01T20:30:00Z");
    let mut feed = morning_news().as_array().unwrap().clone();
    feed.push(story("n1", "ABC News", "Hunter Valley bushfire contained overnight", "Crews contained the fire.", "2026-11-02T03:00:00Z", &["australia"]));
    feed.push(story("n2", "BBC News", "Markets close higher after central bank signal", "Shares rose.", "2026-11-02T04:00:00Z", &["world"]));
    feed.push(story("n3", "NHK World", "Typhoon heads for southern Japan", "Landfall on Wednesday.", "2026-11-02T04:30:00Z", &["east-asia"]));
    a.news = json!(feed.clone());
    a.news_at = "2026-11-02T05:45:00Z".into();
    let e = a.get(&format!("/api/briefing?kind=evening&date={TODAY}"))["news"]["items"].clone();
    record_news(&mut a, "evening", TODAY, &e, "");
    // next morning: same feed plus a rewrite of an evening story by another outlet and one fresh story
    feed.push(story("x1", "The Guardian", "Typhoon heads towards southern Japan", "Landfall is expected on Wednesday.", "2026-11-02T20:00:00Z", &["east-asia"]));
    feed.push(story("x2", "SBS News", "Bushfire in the Hunter Valley contained overnight", "Crews contained the fire.", "2026-11-02T19:00:00Z", &["australia"]));
    feed.push(story("f1", "ABC News", "Fresh story about a new Sydney tunnel opening", "The new tunnel opens to traffic on Friday after years of work. Drivers are being asked to plan ahead.", "2026-11-02T19:30:00Z", &["australia"]));
    a.news = json!(feed);
    a.news_at = "2026-11-02T18:45:00Z".into();
    a.today = "2026-11-03".into();
    let next = a.get("/api/briefing?kind=morning&date=2026-11-03");
    let got = ids(&next["news"]["items"]);
    let used: Vec<String> = ids(&m).into_iter().chain(ids(&e)).collect();
    for id in &got {
        assert!(!used.contains(id), "{id} repeated: {got:?}");
    }
    assert!(!got.contains(&"x1".to_string()) && !got.contains(&"x2".to_string()), "same story from another outlet: {got:?}");
    assert!(got.contains(&"f1".to_string()), "{got:?}");
    // old records are pruned after ~48 hours
    let mut st = a.st();
    crate::news::record(&mut st, Kind::Morning, d("2026-11-06"), vec![serde_json::from_value(feed_item_for_prune()).unwrap()], None);
    assert!(st.news_used.keys().all(|k| k.as_str() >= "2026-11-04"), "{:?}", st.news_used.keys());
}

fn feed_item_for_prune() -> Value {
    story("p", "ABC News", "Anything", "Lead.", "2026-11-06T01:00:00Z", &["world"])
}

#[test]
fn news_selection_is_varied_with_life_most_days_and_industry_tech_sometimes() {
    let mut life = 0;
    let mut industry = 0;
    let mut days = 0;
    for off in 0..20 {
        let day = fmt_date(d(TODAY) + chrono::Duration::days(off));
        let mut a = news_app(&day);
        a.today = day.clone();
        let b = a.get(&format!("/api/briefing?kind=morning&date={day}"));
        let items = b["news"]["items"].as_array().unwrap();
        days += 1;
        if items.iter().any(|i| i["topics"].as_array().unwrap().iter().any(|t| t == "life")) {
            life += 1;
        }
        if items.iter().any(|i| i["topics"].as_array().unwrap().iter().any(|t| t == "industry-tech")) {
            industry += 1;
        }
    }
    assert!(life >= 5, "life story on {life}/{days} days");
    assert!(industry >= 2 && industry < days, "industry-tech on {industry}/{days} days (a good chance, not guaranteed)");
}

#[test]
fn news_industry_keywords_tag_chip_and_battery_stories() {
    assert!(crate::news::industry_score("Samsung and SK hynix lift chip output", "semiconductor makers") >= 2);
    assert!(crate::news::industry_score("Japan to fund rare earth magnet production", "") >= 2);
    assert!(crate::news::industry_score("Lithium prices fall as cathode makers cut output", "") >= 2);
    assert_eq!(crate::news::industry_score("Local council approves new bike lane", "Cyclists welcome it"), 0);
}

#[test]
fn news_first_sentence_is_short_and_complete() {
    use crate::news::first_sentence;
    assert_eq!(first_sentence("Several ferries were suspended. Repairs will take days."), "Several ferries were suspended.");
    assert_eq!(first_sentence("The U.S. said talks continue on Monday. More later."), "The U.S. said talks continue on Monday.");
    assert_eq!(first_sentence("No full stop here"), "No full stop here.");
    assert_eq!(first_sentence(""), "");
    assert!(words_in(&first_sentence(&"word ".repeat(80))) <= 33);
}

#[test]
fn news_select_endpoint_returns_prompts_and_refresh_avoids_the_recorded_stories() {
    let mut a = news_app(TODAY);
    let first = a.post("/api/news/select", json!({"kind": "morning"}));
    let ids1 = ids(&first["items"]);
    assert!(first["system_prompt"].as_str().unwrap().contains("\"news\""));
    record_news(&mut a, "morning", TODAY, &first["items"], "2026-11-01T20:30:00Z");
    let refreshed = a.post("/api/news/select", json!({"kind": "morning", "refresh": true}));
    for id in ids(&refreshed["items"]) {
        assert!(!ids1.contains(&id));
    }
    let (s, _) = a.req("POST", "/api/news/record", json!({"kind": "morning", "items": []}));
    assert_eq!(s, 400);
}

// ------------------------------------------------------------ per-task talk

/// A fresh app with five generic one-off tasks tomorrow and today (varied size, notes, steps, a timed one).
fn five_task_app() -> (App, Vec<&'static str>) {
    let mut a = App::new(TODAY);
    a.req("PUT", "/api/settings", json!({"user_name": "Alex"}));
    let titles = vec!["Renew car registration", "Sort the garden shed", "Call the plumber", "Write the quarterly report", "Dentist check-up"];
    for day in [TODAY, "2026-11-03"] {
        a.post("/api/tasks", json!({"title": titles[0], "date": day, "notes": "Reminder letter is in the drawer."}));
        a.post("/api/tasks", json!({"title": titles[1], "date": day, "part_of_day": "afternoon"}));
        a.post("/api/tasks", json!({"title": titles[2], "date": day}));
        a.post("/api/tasks", json!({"title": titles[3], "date": day, "checklist": [{"text": "Gather last quarter's numbers"}, {"text": "Draft the summary"}]}));
        a.post("/api/tasks", json!({"title": titles[4], "date": day, "time": "15:30"}));
    }
    (a, titles)
}

#[test]
fn templates_walk_through_every_task_with_we_phrasing() {
    let (mut a, titles) = five_task_app();
    a.weather = json!({ TODAY: {"summary": "Thunderstorm", "temp_max": 22, "temp_min": 16, "precip_prob": 90, "uv_max": 2.0} });
    for kind in ["evening", "morning"] {
        let t = a.get(&format!("/api/briefing?kind={kind}&date={TODAY}"))["template_text"].as_str().unwrap().to_string();
        println!("{kind}: {t}\n");
        for title in &titles {
            let ph = crate::taskvoice::task_phrase(title).text().to_string();
            let lt = t.to_lowercase();
            assert!(lt.contains(&title.to_lowercase()) || lt.contains(&ph.to_lowercase()), "{kind}: missing {title}\n{t}");
        }
        assert!(!t.contains("plus a few more") && !t.contains("a few more,"), "{t}");
        assert!(t.contains("we've got") || t.contains("you've got") || t.contains("we have") || t.contains("for us") || t.contains("your day"), "{t}");
        assert!(t.contains("says reminder letter is in the drawer"), "notes missing\n{t}");
        assert!(t.contains("the first step is gathering last quarter's numbers"), "checklist step missing\n{t}");
        assert!(t.contains("usually takes") || (t.contains("half hour") && t.contains("ten minutes")), "size hints missing\n{t}");
        assert!(!has_stats(&t));
        assert_spoken(kind, &t);
        if kind == "evening" {
            assert!(t.contains("Alex"), "{t}");
            assert!(t.contains("tomorrow"), "{t}");
            assert!(t.contains("Nothing else") || t.contains("nothing else") || t.contains("Nothing more"), "{t}");
            assert!(t.contains("thunderstorm rolled through") && !t.contains("was thunderstorm"), "{t}");
            let n = words_in(&t);
            assert!((300..=900).contains(&n), "{n} words");
        }
    }
}

#[test]
fn many_tasks_name_the_extras_in_one_sentence() {
    let mut a = App::new(TODAY);
    let titles: Vec<String> = (1..=9).map(|i| format!("Errand number {i}")).collect();
    for t in &titles {
        a.post("/api/tasks", json!({"title": t, "date": "2026-11-03"}));
    }
    let t = a.get("/api/briefing?kind=evening")["template_text"].as_str().unwrap().to_string();
    for title in &titles {
        assert!(t.to_lowercase().contains(&title.to_lowercase()), "missing {title}\n{t}");
    }
    assert!(t.contains("The smaller bits are"), "{t}");
    assert!(!t.contains("plus a few more"), "{t}");
}

#[test]
fn weather_phrases_are_grammatical_in_both_tenses() {
    let mut seen = std::collections::HashSet::new();
    for code in [0, 1, 2, 3, 45, 51, 53, 55, 56, 61, 63, 65, 66, 71, 73, 75, 77, 80, 81, 82, 85, 95, 96, 99, 123] {
        let summary = crate::weather::wmo_summary(code);
        for past in [true, false] {
            let p = crate::weather::natural_sentence(summary, past);
            let low = summary.to_lowercase();
            // the label is a noun phrase for most codes ("thunderstorm"): never "it was thunderstorm"
            if !["foggy", "overcast"].contains(&low.as_str()) {
                let pl = format!("{} ", p.to_lowercase().replace('.', " "));
                assert!(!pl.contains(&format!("it was {low} ")) && !pl.contains(&format!("going to be {low} ")), "{p}");
            }
            assert!(p.ends_with('.') && p.contains("today"), "{p}");
            assert_spoken("weather", &p);
            if past {
                assert!(!p.contains("going to") && !p.contains("There'll"), "{p}");
            }
            seen.insert(p);
        }
    }
    assert!(seen.contains("Pretty wild weather today, wasn't it? A thunderstorm rolled through."));
    assert!(seen.contains("It stayed sunny today."));
}

#[test]
fn gym_line_says_what_the_workout_trains() {
    let base = App::with_sample(TODAY).state;
    let mut seen: Vec<String> = vec![];
    for off in 0..10 {
        let day = fmt_date(d(TODAY) + chrono::Duration::days(off));
        let next = fmt_date(d(&day) + chrono::Duration::days(1));
        let mut a = App { state: base.clone(), today: day.clone(), holidays: json!([]), weather: json!({}), news: json!([]), news_at: String::new() };
        // (briefing kind, the day whose workout it talks about)
        for (kind, target) in [("morning", day.clone()), ("evening", next)] {
            let w = a.get(&format!("/api/days?start={target}&days=1"))[0]["workout"].clone();
            if w.is_null() || w["rest"] == true {
                continue;
            }
            let name = w["name"].as_str().unwrap().to_string();
            let lower = name.to_lowercase();
            let expect = if lower.contains("push") {
                "chest, shoulders and triceps"
            } else if lower.contains("pull") {
                "back, biceps and rear delts"
            } else if lower.contains("leg") {
                "legs, glutes and core"
            } else if lower.contains("cardio") {
                "heart, lungs and endurance"
            } else {
                continue;
            };
            let t = a.get(&format!("/api/briefing?kind={kind}&date={day}"))["template_text"].as_str().unwrap().to_string();
            assert!(t.contains(expect), "{kind} {name}: {t}");
            if w["kind"] != "cardio" {
                let first = w["exercises"][0]["name"].as_str().unwrap();
                assert!(t.contains(first), "{kind} {name}: key lift {first} missing\n{t}");
            }
            seen.push(name);
        }
    }
    seen.sort();
    seen.dedup();
    assert!(seen.len() >= 2, "{seen:?}");
}

#[test]
fn trains_covers_the_usual_workout_names_generically() {
    let plan = |name: &str, kind: &str| crate::gym::WorkoutPlan { rest: false, template_id: None, name: name.into(), kind: kind.into(), note: String::new(), exercises: vec![], options: vec![], minutes_min: None, minutes_max: None };
    let t = |n: &str, k: &str| crate::gym::trains(&plan(n, k));
    assert_eq!(t("Push 1", "strength").unwrap(), "chest, shoulders and triceps");
    assert_eq!(t("Pull 2", "strength").unwrap(), "back, biceps and rear delts");
    assert_eq!(t("Legs + Abs", "strength").unwrap(), "legs, glutes and core");
    assert_eq!(t("Cardio", "cardio").unwrap(), "heart, lungs and endurance");
    assert_eq!(t("Core", "strength").unwrap(), "core");
    assert!(t("Mystery", "strength").is_none());
    let mut rest = plan("Rest", "strength");
    rest.rest = true;
    assert!(crate::gym::trains(&rest).is_none());
}

// ------------------------------------------------------------ template quality, greeting, fillers, style guide

fn days_from(n: i64) -> Vec<String> {
    (0..n).map(|k| fmt_date(d(TODAY) + chrono::Duration::days(k))).collect()
}

fn task_app_for_days(days: &[String], titles: &[&str]) -> App {
    let mut a = App::new(TODAY);
    a.req("PUT", "/api/settings", json!({"user_name": "Sam"}));
    for day in days {
        for t in titles {
            a.post("/api/tasks", json!({"title": t, "date": day}));
        }
    }
    a
}

#[test]
fn morning_greeting_is_a_hello_never_a_well_wish() {
    let days = days_from(14);
    for named in [true, false] {
        let mut a = task_app_for_days(&days, &["Pay the water bill"]);
        if !named {
            a.req("PUT", "/api/settings", json!({"user_name": ""}));
        }
        for day in &days {
            let b = a.get(&format!("/api/briefing?kind=morning&date={day}"));
            let g = b["template_parts"]["greeting"].as_str().unwrap().to_string();
            let l = g.to_lowercase();
            assert!(!l.starts_with("i hope") && !l.starts_with("hope") && !l.starts_with("have a") && !l.contains("lovely"), "{g}");
            assert!(l.contains("morning") || l.contains("rise and shine"), "{g}");
            // the well-wish is in the closing instead
            let body = b["template_parts"]["briefing"].as_str().unwrap();
            assert!(body.trim_end().ends_with('.'), "{body}");
            let last = sentences(body).pop().unwrap().to_lowercase();
            assert!(last.contains("lovely") || last.contains("enjoy") || last.contains("good one") || last.contains("good"), "closing well-wish missing: {last}");
        }
    }
}

#[test]
fn imperative_titles_read_naturally_in_sentences() {
    use crate::taskvoice::{task_phrase, Phrase};
    let cases = [
        ("Try Gemini learning mode for Korean study", "trying Gemini learning mode for Korean study"),
        ("Read about the Australian mining boom", "reading about the Australian mining boom"),
        ("Make book club inquiries", "making some book club inquiries"),
        ("Organise the hall cupboard", "organising the hall cupboard"),
        ("Call the plumber", "calling the plumber"),
        ("Pay the water bill", "paying the water bill"),
    ];
    for (title, want) in cases {
        assert_eq!(task_phrase(title), Phrase::Gerund(want.into()), "{title}");
    }
    // nouns and book-club style titles stay as they are
    assert_eq!(task_phrase("Dentist check-up"), Phrase::Noun("dentist check-up".into()));
    assert_eq!(task_phrase("Book club"), Phrase::Noun("book club".into()));
    assert_eq!(task_phrase("Powerhouse Museum"), Phrase::Noun("Powerhouse Museum".into()));
    assert_eq!(task_phrase("Sam's birthday"), Phrase::Noun("Sam's birthday".into()));
    assert_eq!(task_phrase("Submit weekly report"), Phrase::Gerund("submitting the weekly report".into()));
    assert_eq!(task_phrase("Book dentist appointment"), Phrase::Gerund("booking the dentist appointment".into()));
    assert_eq!(task_phrase("Reply to Sam's email"), Phrase::Gerund("replying to Sam's email".into()));
    assert_eq!(task_phrase("Drink water"), Phrase::Gerund("drinking water".into()));
    // and the template uses the phrasing
    let days = days_from(1);
    let mut a = task_app_for_days(&days, &["Try Gemini learning mode for Korean study", "Read about the Australian mining boom", "Make book club inquiries", "Organise the hall cupboard"]);
    let t = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"))["template_text"].as_str().unwrap().to_string();
    for want in ["trying Gemini learning mode for Korean study", "reading about the Australian mining boom", "making some book club inquiries", "organising the hall cupboard"] {
        assert!(t.contains(want), "{want} missing in\n{t}");
    }
    assert!(!t.contains("there's Try ") && !t.contains("there's Read ") && !t.contains("got Make "), "{t}");
}

#[test]
fn no_stock_sentence_repeats_within_one_summary() {
    let days = days_from(10);
    let titles = ["Sort the garden shed", "Clean the oven", "Tidy the spare room", "Wash the car", "Mop the hallway"];
    let mut a = task_app_for_days(&days, &titles);
    for kind in ["morning", "evening"] {
        for day in &days {
            let t = a.get(&format!("/api/briefing?kind={kind}&date={day}"))["template_text"].as_str().unwrap().to_string();
            let mut seen = std::collections::HashSet::new();
            for s in sentences(&t) {
                if words_in(&s) >= 6 {
                    assert!(seen.insert(s.clone()), "{kind} {day}: repeated sentence {s:?}\n{t}");
                }
            }
        }
    }
}

#[test]
fn goals_link_only_on_meaningful_words_and_at_most_once() {
    use crate::goals::find_link;
    use crate::model::Goal;
    let g = |title: &str, why: &str| Goal { id: 1, title: title.into(), horizon: "medium".into(), deadline: None, why: why.into(), status: "active".into(), achieved_on: None, created: "2026-01-01".into() };
    let generic = [g("Read more and work better", "I want to spend my own time writing and learning more"), g("Be better with time", "more of my own work")];
    for task in ["Read about the Australian mining boom", "Work on the report notes", "Writing time", "Learning mode for something", "Own it", "Make book club inquiries"] {
        assert!(find_link(&generic, &[(task.into(), task.into())]).is_none(), "{task}");
    }
    let korean = [g("Learn Korean", "to talk with family")];
    assert!(find_link(&korean, &[("Try Gemini learning mode for Korean study".into(), "Try Gemini learning mode for Korean study".into())]).is_some());
    assert!(find_link(&korean, &[("Read about the Australian mining boom".into(), "Read about the Australian mining boom".into())]).is_none());
    // in a whole summary the goal is mentioned at most once
    let days = days_from(3);
    let mut a = task_app_for_days(&days, &["Korean vocab practice", "Korean lesson notes", "Do a Korean flashcard set"]);
    a.post("/api/goals", json!({"title": "Learn Korean", "horizon": "medium"}));
    for kind in ["morning", "evening"] {
        for day in &days {
            let t = a.get(&format!("/api/briefing?kind={kind}&date={day}"))["template_text"].as_str().unwrap().to_string();
            assert!(t.matches("Learn Korean").count() <= 1, "{kind} {day}\n{t}");
        }
    }
}

#[test]
fn keyword_categories_for_study_admin_and_reading() {
    use crate::model::Category;
    let names = ["Chores", "Exercise", "Study", "Work", "Errands", "Appointments", "Social", "Self-care", "Admin & Money"];
    let cats: Vec<Category> = names.iter().enumerate().map(|(i, n)| Category { id: i as i64 + 1, name: n.to_string(), color: "#000".into() }).collect();
    let name = |t: &str| crate::tasks::keyword_category(t, &cats).map(|id| names[id as usize - 1]);
    assert_eq!(name("Try Gemini learning mode for Korean study"), Some("Study"));
    assert_eq!(name("Read about the Australian mining boom"), Some("Study"));
    assert!(matches!(name("Make book club inquiries"), Some("Admin & Money") | Some("Social")));
    assert_eq!(name("Research flights to Seoul"), Some("Study"));
    assert_eq!(name("JLPT practice"), Some("Study"));
    assert_eq!(name("Email the landlord about the lease"), Some("Admin & Money"));
    assert_ne!(name("Read"), Some("Self-care"));
    assert_eq!(name("Read a novel before bed"), Some("Self-care"));
}

fn filler_count(p: &str) -> usize {
    let l = p.to_lowercase();
    ["…", " um,", ", um", "hmm", "oh, and", "you know", " uh,", " ah,"].iter().map(|k| l.matches(k).count()).sum()
}

#[test]
fn templates_have_light_natural_pauses_but_not_too_many() {
    let days = days_from(14);
    let mut a = task_app_for_days(&days, &["Pay the water bill", "Sort the garden shed", "Call the plumber"]);
    for kind in ["morning", "evening"] {
        let mut with = 0;
        for day in &days {
            let t = a.get(&format!("/api/briefing?kind={kind}&date={day}"))["template_text"].as_str().unwrap().to_string();
            if filler_count(&t) > 0 {
                with += 1;
            }
            for p in t.split("\n\n") {
                assert!(filler_count(p) <= 3, "{kind} {day}: too many in a paragraph ({}) {p}", filler_count(p));
            }
            assert!(!has_stats(&t));
        }
        assert!(with * 10 >= days.len() * 8, "{kind}: only {with} days had a pause");
    }
}

#[test]
fn every_prompt_carries_the_spoken_style_guide() {
    let mut a = news_app(TODAY);
    for kind in ["morning", "evening", "weekly"] {
        let b = a.get(&format!("/api/briefing?kind={kind}&date={TODAY}"));
        assert!(b["system_prompt"].as_str().unwrap().contains(crate::STYLE_GUIDE), "{kind}");
    }
    let b = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    assert!(b["news"]["system_prompt"].as_str().unwrap().contains(crate::STYLE_GUIDE));
    assert!(crate::STYLE_GUIDE.contains("never put one inside a quoted headline"));
    for k in ["BASE TONE", "NEWS TONE", "EXAMPLE OF THE TONE FOR THE DAY", "EXAMPLE OF THE TONE FOR THE NEWS", "speaking of", "reflective takeaway", "PERSONAL ASSISTANT MANNER", "DAY ARC", "WIND-DOWN VOICE", "NEVER: sell"] {
        assert!(crate::STYLE_GUIDE.contains(k), "{k}");
    }
}

#[test]
fn news_leads_are_complete_sentences() {
    use crate::news::first_sentence;
    for lead in ["Several ferries were suspended on Sunday. Repairs will take days.", "A long lead that runs on and then stops mid-", "The cut off lead ends here. And then a part that was trimmed…", "Fine without a full stop"] {
        let f = first_sentence(lead);
        assert!(f.is_empty() || f.ends_with(['.', '!', '?']), "{f:?}");
    }
    assert_eq!(first_sentence("The cut off lead ends here. And then a part that was trimmed…"), "The cut off lead ends here.");
    assert_eq!(first_sentence("A lead with no end that keeps going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going and going"), "");
}

#[test]
fn knowledge_picks_fit_the_day_and_vary() {
    use crate::knowledge::{self, K};
    let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 4).unwrap();
    let wet = knowledge::tags("evening", Some(16), "Rain showers", true, &["Study".into()], true, true, false, "");
    for t in ["evening", "rain", "cold", "study", "goal", "quiet", "workday"] {
        assert!(wet.contains(&t), "{t}");
    }
    let p = knowledge::picks(knowledge::kinds_for(true, false), &wet, d);
    assert_eq!(p.iter().map(|e| e.kind).collect::<Vec<_>>(), vec![K::Memory, K::Fact, K::WindDown]);
    // a different memory most days over a week, never the same one two days running
    let mut seen = std::collections::HashSet::new();
    let mut last = "";
    for i in 0..7 {
        let q = knowledge::picks(knowledge::kinds_for(false, false), &wet, d + chrono::Duration::days(i));
        assert_ne!(q[0].text, last);
        last = q[0].text;
        seen.insert(q[0].text);
    }
    assert!(seen.len() >= 5, "{}", seen.len());
    // wind-down lines only ever go to the evening
    let m = knowledge::picks(knowledge::kinds_for(false, false), &knowledge::tags("morning", Some(22), "Clear", false, &[], true, false, true, ""), d);
    assert!(m.iter().all(|e| e.kind != K::WindDown));
}

#[test]
fn knowledge_entries_are_clean() {
    for e in crate::knowledge::ENTRIES {
        let t = e.text.to_lowercase();
        let words: Vec<&str> = t.split(|c: char| !c.is_alphanumeric()).collect();
        for bad in ["bro", "dude", "manifest", "manifesting", "astrology", "crystals", "promo", "subscribe", "discount"] {
            assert!(!words.contains(&bad), "{bad}: {}", e.text);
        }
        assert!(!e.tags.is_empty());
        assert!(e.text.ends_with('.') || e.text.ends_with('!'), "{}", e.text);
        assert!(!crate::has_stats(e.text), "{}", e.text);
    }
}

#[test]
fn every_summary_prompt_carries_the_knowledge_picks() {
    let mut a = news_app(TODAY);
    for kind in ["morning", "evening", "weekly"] {
        let b = a.get(&format!("/api/briefing?kind={kind}&date={TODAY}"));
        assert!(b["system_prompt"].as_str().unwrap().contains("LITTLE THINGS TO DRAW ON"), "{kind}");
        assert!(b.to_string().contains("Little things you could draw on today"), "{kind}");
    }
}

#[test]
fn news_template_tells_more_of_the_story_without_cutting_sentences() {
    use crate::news::lead_sentences;
    let lead = "Crews contained the fire on Monday afternoon. No homes were lost. Residents can return from Tuesday. The cause is being investigated…";
    let t = lead_sentences(lead, 90);
    assert_eq!(t, "Crews contained the fire on Monday afternoon. No homes were lost. Residents can return from Tuesday.");
    assert_eq!(lead_sentences("One. Two two. Three three three.", 3), "One. Two two.");
    assert_eq!(lead_sentences("", 90), "");
}

#[test]
fn briefings_carry_the_editor_brief_the_persona_and_full_story_news() {
    let mut a = news_app(TODAY);
    for kind in ["morning", "evening", "weekly"] {
        let b = a.get(&format!("/api/briefing?kind={kind}&date={TODAY}"));
        let r = b["review_prompt"].as_str().unwrap();
        assert!(r.contains("same keys") && r.contains("recent summaries") && r.contains("word for word"), "{kind}");
        assert!(b["system_prompt"].as_str().unwrap().contains("KNOWING THEM AND YOURSELF"), "{kind}");
    }
    let b = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    assert!(b["system_prompt"].as_str().unwrap().contains("Cover BOTH stories"));
    assert!(b["user_prompt"].as_str().unwrap().contains("Article: "));
}

#[test]
fn common_sense_durations() {
    use crate::durations::{range_text, typical_for};
    let hc = typical_for("Haircut").unwrap();
    assert!(hc.min >= 30 && hc.outing, "{hc:?}");
    assert_eq!(range_text(&hc), "about 30 to 45 minutes");
    // booking the haircut is quick; the haircut itself is not
    assert!(typical_for("Book haircut").unwrap().max <= 10);
    assert!(typical_for("Book club").is_some_and(|t| t.max >= 90));
    assert!(typical_for("Dentist check-up").unwrap().max >= 45);
    assert!(typical_for("Call Mum").unwrap().max <= 20);
    assert!(typical_for("Call the bank about my card").unwrap().max >= 30);
    assert!(typical_for("Grocery shopping").unwrap().outing);
    assert!(typical_for("Fix the leaky tap").is_none());
    assert_eq!(range_text(&typical_for("Pay electricity bill").unwrap()), "five or ten minutes");
    // the template never calls a haircut a ten-minute job, and doesn't suggest it for the lunch break
    let days = days_from(1);
    let mut a = task_app_for_days(&days, &["Haircut"]);
    let t = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"))["template_text"].as_str().unwrap().to_string();
    let hair: Vec<&str> = t.split(". ").filter(|s| s.to_lowercase().contains("haircut")).collect();
    assert!(!hair.is_empty(), "{t}");
    for s in &hair {
        assert!(!s.contains("ten minutes") && !s.contains("quick one"), "{s}");
    }
    assert!(t.contains("30 to 45 minutes"), "{t}");
    assert!(!t.contains("middle of the day, haircut") && !t.contains("haircut would fit in nicely"), "{t}");
    // the AI is told too
    let b = a.get(&format!("/api/briefing?kind=morning&date={TODAY}"));
    assert!(b["user_prompt"].as_str().unwrap().contains("usually takes about 30 to 45 minutes, plus getting there"));
    assert!(b["system_prompt"].as_str().unwrap().contains("COMMON SENSE ABOUT TIME"));
}
