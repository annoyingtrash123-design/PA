use chrono::{Local, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};

pub fn today() -> NaiveDate {
    Local::now().date_naive()
}

pub fn open(path: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL").ok();
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch(crate::daynote::CREATE_TABLE)?;
    Ok(conn)
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS categories (
  id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, color TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  date TEXT,
  time TEXT,
  part_of_day TEXT,
  category_id INTEGER,
  notes TEXT NOT NULL DEFAULT '',
  recurrence TEXT,
  rolled_from TEXT);
CREATE TABLE IF NOT EXISTS checklist_items (
  id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, text TEXT NOT NULL, pos INTEGER NOT NULL DEFAULT 0);
-- date is '' for one-off tasks
CREATE TABLE IF NOT EXISTS completions (
  task_id INTEGER NOT NULL, date TEXT NOT NULL, PRIMARY KEY (task_id, date));
CREATE TABLE IF NOT EXISTS checklist_done (
  task_id INTEGER NOT NULL, date TEXT NOT NULL, item_id INTEGER NOT NULL, PRIMARY KEY (task_id, date, item_id));

CREATE TABLE IF NOT EXISTS exercises (
  id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE COLLATE NOCASE);
CREATE TABLE IF NOT EXISTS templates (
  id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS template_exercises (
  id INTEGER PRIMARY KEY AUTOINCREMENT, template_id INTEGER NOT NULL, exercise_id INTEGER NOT NULL,
  pos INTEGER NOT NULL, sets INTEGER NOT NULL, rep_min INTEGER NOT NULL, rep_max INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS rotation_items (
  pos INTEGER PRIMARY KEY, template_id INTEGER, rest INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT, template_id INTEGER, name TEXT NOT NULL,
  date TEXT NOT NULL, completed INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS session_sets (
  session_id INTEGER NOT NULL, exercise_id INTEGER NOT NULL, set_no INTEGER NOT NULL,
  weight REAL NOT NULL DEFAULT 0, reps INTEGER NOT NULL DEFAULT 0, done INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (session_id, exercise_id, set_no));
"#;

pub fn get_setting(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0))
        .optional()
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}
