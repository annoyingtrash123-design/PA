//! The serde State model (the whole app state is one JSON document) and request context.
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashMap};

pub const STATE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub assistant_name: String,
    pub user_name: String,
    pub voice: String,
    pub location_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            assistant_name: "Sunny".into(),
            user_name: String::new(),
            voice: String::new(),
            location_name: "Sydney".into(),
            latitude: -33.8688,
            longitude: 151.2093,
            timezone: "Australia/Sydney".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Recurrence {
    Daily,
    Weekdays,
    Weekly { days: Vec<u8> },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChecklistItem {
    pub id: i64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default = "anytime")]
    pub part_of_day: String,
    #[serde(default)]
    pub category_id: Option<i64>,
    #[serde(default)]
    pub category_auto: bool,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub recurrence: Option<Recurrence>,
    #[serde(default)]
    pub checklist: Vec<ChecklistItem>,
}

fn anytime() -> String {
    "anytime".into()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Goal {
    pub id: i64,
    pub title: String,
    pub horizon: String,
    #[serde(default)]
    pub deadline: Option<String>,
    #[serde(default)]
    pub why: String,
    #[serde(default = "active")]
    pub status: String,
    #[serde(default)]
    pub achieved_on: Option<String>,
    #[serde(default)]
    pub created: String,
}

fn active() -> String {
    "active".into()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WeatherPrefs {
    pub comfortable_max: f64,
    pub hot_above: f64,
    pub too_hot_above: f64,
    pub likes_rain: bool,
    pub avoid_sunburn: bool,
}

impl Default for WeatherPrefs {
    fn default() -> Self {
        WeatherPrefs { comfortable_max: 18.0, hot_above: 22.0, too_hot_above: 25.0, likes_rain: true, avoid_sunburn: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WorkPrefs {
    pub enabled: bool,
    /// 0 = Monday ... 6 = Sunday
    pub days: Vec<u32>,
    pub start: String,
    pub end: String,
    pub holiday_region: String,
    #[serde(default = "default_lunch")]
    pub lunch_minutes: u32,
}

fn default_lunch() -> u32 {
    60
}

impl Default for WorkPrefs {
    fn default() -> Self {
        WorkPrefs { enabled: true, days: vec![0, 1, 2, 3, 4], start: "10:00".into(), end: "17:00".into(), holiday_region: "AU-NSW".into(), lunch_minutes: 60 }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Rhythm {
    pub wake_from: String,
    pub wake_to: String,
    pub sleep_from: String,
    pub sleep_to: String,
}

impl Default for Rhythm {
    fn default() -> Self {
        Rhythm { wake_from: "06:00".into(), wake_to: "07:00".into(), sleep_from: "23:00".into(), sleep_to: "00:00".into() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Note {
    pub id: i64,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Memory {
    pub weather: WeatherPrefs,
    pub work: WorkPrefs,
    pub rhythm: Rhythm,
    pub notes: Vec<Note>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Exercise {
    pub id: i64,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TemplateExercise {
    pub exercise_id: i64,
    pub name: String,
    pub sets: i64,
    pub rep_min: i64,
    pub rep_max: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Template {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub exercises: Vec<TemplateExercise>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RotItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_id: Option<i64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Rotation {
    pub items: Vec<RotItem>,
    pub next_index: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SetRec {
    pub exercise_id: i64,
    pub set_no: i64,
    #[serde(default)]
    pub weight: f64,
    #[serde(default)]
    pub reps: i64,
    #[serde(default)]
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub id: i64,
    #[serde(default)]
    pub template_id: Option<i64>,
    pub name: String,
    pub date: String,
    #[serde(default)]
    pub completed: bool,
    #[serde(default)]
    pub sets: Vec<SetRec>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Gym {
    pub exercises: Vec<Exercise>,
    pub templates: Vec<Template>,
    pub rotation: Rotation,
    pub sessions: Vec<Session>,
    /// Internal: the date a rest day was last taken (so the projection can show "rest" for today).
    pub rest_taken_date: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct State {
    pub version: u32,
    pub next_id: i64,
    pub settings: Settings,
    pub categories: Vec<Category>,
    pub tasks: Vec<Task>,
    /// "<task_id>|<date>" -> true (one-off tasks use an empty date)
    pub completions: BTreeMap<String, bool>,
    /// "<task_id>|<date>|<item_id>" -> true
    pub checklist_done: BTreeMap<String, bool>,
    /// "<task_id>" -> original date
    pub rolled_over: BTreeMap<String, String>,
    pub days_off: Vec<String>,
    pub goals: Vec<Goal>,
    pub memory: Memory,
    pub gym: Gym,
}

impl Default for State {
    fn default() -> Self {
        State {
            version: STATE_VERSION,
            next_id: 100,
            settings: Settings::default(),
            categories: vec![],
            tasks: vec![],
            completions: BTreeMap::new(),
            checklist_done: BTreeMap::new(),
            rolled_over: BTreeMap::new(),
            days_off: vec![],
            goals: vec![],
            memory: Memory::default(),
            gym: Gym::default(),
        }
    }
}

impl State {
    pub fn new_id(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Largest id used anywhere, so imports can repair `next_id`.
    pub fn max_id(&self) -> i64 {
        let mut m = 0;
        let mut up = |v: i64| m = m.max(v);
        self.categories.iter().for_each(|c| up(c.id));
        self.tasks.iter().for_each(|t| {
            up(t.id);
            t.checklist.iter().for_each(|c| up(c.id));
        });
        self.goals.iter().for_each(|g| up(g.id));
        self.memory.notes.iter().for_each(|n| up(n.id));
        self.gym.exercises.iter().for_each(|e| up(e.id));
        self.gym.templates.iter().for_each(|t| up(t.id));
        self.gym.sessions.iter().for_each(|s| up(s.id));
        m
    }
}

// ------------------------------------------------------------ request context

fn de_round<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(f64::deserialize(d)?.round() as i64)
}

fn de_round_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    Ok(Option::<f64>::deserialize(d)?.map(|x| x.round() as i64))
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Weather {
    pub summary: String,
    #[serde(deserialize_with = "de_round")]
    pub temp_max: i64,
    #[serde(deserialize_with = "de_round")]
    pub temp_min: i64,
    /// Max precipitation probability for the day, percent.
    #[serde(default, deserialize_with = "de_round_opt")]
    pub precip_prob: Option<i64>,
    #[serde(default)]
    pub uv_max: Option<f64>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Holiday {
    pub date: String,
    pub name: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Ctx {
    pub today: String,
    pub now: String,
    pub has_key: bool,
    pub weather: HashMap<String, Weather>,
    pub holidays: Vec<Holiday>,
}
