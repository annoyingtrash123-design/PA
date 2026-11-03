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
    assert_eq!(st.version, 1);
    assert_eq!(st.next_id, 100);
    assert_eq!(st.categories.len(), 10);
    assert_eq!(st.categories[0].name, "Chores");
    assert_eq!(st.gym.templates.len(), 6);
    assert_eq!(st.gym.rotation.items.len(), 8);
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

#[test]
fn gym_projection_and_progression_hint() {
    let mut a = App::with_sample(TODAY);
    let days = a.get("/api/days?start=2026-11-01&days=4");
    assert!(days[0]["workout"].is_null(), "past days have no workout");
    assert_eq!(days[1]["workout"]["name"], "Push B");
    assert_eq!(days[2]["workout"]["name"], "Pull B");
    assert_eq!(days[3]["workout"]["name"], "Legs B");
    // Incline DB Press: last time 22.5 kg x 12,12,12 and rep_max is 12 -> go up by 2.5
    let inc = days[1]["workout"]["exercises"].as_array().unwrap().iter().find(|e| e["name"] == "Incline DB Press").unwrap().clone();
    assert_eq!(inc["suggested_weight"], 25.0);
    assert_eq!(inc["hint"], "You hit 12 reps on every set last time, try 25 kg");
    assert_eq!(inc["last"]["sets"].as_array().unwrap().len(), 3);
    // Dips: no history -> null
    let dips = days[1]["workout"]["exercises"].as_array().unwrap().iter().find(|e| e["name"] == "Dips").unwrap().clone();
    assert!(dips["suggested_weight"].is_null() && dips["hint"].is_null() && dips["last"].is_null());
    // next
    assert_eq!(a.get("/api/gym/next")["name"], "Push B");
    // Pull A deadlift: reps 8,7,6 vs rep_max 8 -> stay at the weight
    let (tid, ex) = {
        let st = a.st();
        (st.gym.templates.iter().find(|t| t.name == "Pull A").unwrap().id, st.gym.exercises.iter().find(|e| e.name == "Deadlift").unwrap().id)
    };
    let _ = tid;
    let hist = a.get(&format!("/api/exercises/{ex}/history"));
    assert_eq!(hist[0]["top_weight"], 100.0);
    assert_eq!(hist[0]["best_reps"], 8);
    assert_eq!(hist[0]["est_1rm"], 126.7);
}

#[test]
fn gym_session_flow_advances_rotation() {
    let mut a = App::with_sample(TODAY);
    let push_b = a.st().gym.templates.iter().find(|t| t.name == "Push B").unwrap().id;
    let s = a.post("/api/sessions", json!({"template_id": push_b}));
    assert_eq!(s["completed"], false);
    assert_eq!(s["date"], TODAY);
    let sets = s["sets"].as_array().unwrap().clone();
    assert_eq!(sets.len(), 15);
    // pre-filled with the suggested weight (25 kg) and rep_min reps for the progressed exercise
    let inc = sets.iter().find(|x| x["weight"] == 25.0).unwrap();
    assert_eq!(inc["reps"], 8);
    // asking again returns the same session
    assert_eq!(a.post("/api/sessions", json!({"template_id": push_b}))["id"], s["id"]);
    let mut upd = sets.clone();
    upd[0]["done"] = json!(true);
    let s2 = a.req("PUT", &format!("/api/sessions/{}/sets", s["id"]), json!({"sets": upd}));
    assert_eq!(s2.0, 200);
    assert_eq!(s2.1["sets"][0]["done"], true);
    let c = a.post(&format!("/api/sessions/{}/complete", s["id"]), json!({}));
    assert_eq!(c["completed"], true);
    assert_eq!(a.get("/api/rotation")["next_index"], 5);
    // today now shows the completed session's template and tomorrow continues
    let days = a.get("/api/days?start=2026-11-02&days=2");
    assert_eq!(days[0]["workout"]["name"], "Push B");
    assert_eq!(days[1]["workout"]["name"], "Pull B");
    // rest handling: rotation next is Pull B, not a rest
    assert_eq!(a.req("POST", "/api/gym/rest", json!({})).0, 400);
    assert_eq!(a.get("/api/sessions?limit=2").as_array().unwrap().len(), 2);
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
    assert_eq!(a.st().gym.sessions.len(), 3);
    assert_eq!(a.st().gym.sessions[0].date, "2027-03-02");
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
