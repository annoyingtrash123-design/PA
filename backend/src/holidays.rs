//! Public holidays from Nager.Date, cached per (year, region) in the DB.
//! Any failure means "no holidays"; requests never fail because of this.
use crate::AppState;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const CREATE_TABLE: &str = "CREATE TABLE IF NOT EXISTS holidays (year INTEGER NOT NULL, region TEXT NOT NULL, data TEXT NOT NULL, PRIMARY KEY (year, region));";

#[derive(Serialize, Deserialize, Clone)]
struct H {
    date: String,
    name: String,
}

static FAILED: Mutex<Option<HashMap<(i32, String), Instant>>> = Mutex::new(None);

/// `PA_FAKE_HOLIDAYS="2026-12-25:Christmas Day,2026-12-28:Boxing Day (observed)"`
fn fake() -> Option<Vec<H>> {
    let raw = std::env::var("PA_FAKE_HOLIDAYS").ok()?;
    Some(
        raw.split(',')
            .filter_map(|p| p.split_once(':'))
            .map(|(d, n)| H { date: d.trim().to_string(), name: n.trim().to_string() })
            .collect(),
    )
}

fn country(region: &str) -> &str {
    region.split('-').next().unwrap_or("")
}

async fn fetch(st: &AppState, year: i32, region: &str) -> Option<Vec<H>> {
    let url = format!("https://date.nager.at/api/v3/PublicHolidays/{year}/{}", country(region));
    let resp = st.http.get(url).timeout(Duration::from_secs(8)).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    let mut out = vec![];
    for h in v.as_array()? {
        let global = h["global"].as_bool().unwrap_or(false);
        let in_region = h["counties"].as_array().is_some_and(|c| c.iter().any(|x| x.as_str() == Some(region)));
        if global || in_region {
            if let (Some(d), Some(n)) = (h["date"].as_str(), h["name"].as_str()) {
                out.push(H { date: d.to_string(), name: n.to_string() });
            }
        }
    }
    Some(out)
}

/// Make sure the holidays for these years are cached (best effort).
pub async fn ensure(st: &AppState, years: &[i32]) {
    if fake().is_some() {
        return;
    }
    let region = {
        let conn = st.db.lock().unwrap();
        let w = crate::memory::load(&conn).map(|m| m.work).unwrap_or_default();
        if !w.enabled || w.holiday_region.is_empty() {
            return;
        }
        w.holiday_region
    };
    for &year in years {
        {
            let conn = st.db.lock().unwrap();
            let have: bool = conn.query_row("SELECT 1 FROM holidays WHERE year=?1 AND region=?2", params![year, region], |_| Ok(())).optional().ok().flatten().is_some();
            if have {
                continue;
            }
        }
        {
            let f = FAILED.lock().unwrap();
            if f.as_ref().and_then(|m| m.get(&(year, region.clone()))).is_some_and(|t| t.elapsed() < Duration::from_secs(600)) {
                continue;
            }
        }
        match fetch(st, year, &region).await {
            Some(list) => {
                let conn = st.db.lock().unwrap();
                let _ = conn.execute(
                    "INSERT OR REPLACE INTO holidays(year,region,data) VALUES(?1,?2,?3)",
                    params![year, region, serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())],
                );
            }
            None => {
                tracing::warn!("public holiday lookup failed for {year} {region}; treating as none");
                FAILED.lock().unwrap().get_or_insert_with(HashMap::new).insert((year, region.clone()), Instant::now());
            }
        }
    }
}

/// Name of the public holiday on `date` (YYYY-MM-DD), from the cache. Sync; never fails.
pub fn lookup(conn: &Connection, region: &str, date: &str) -> Option<String> {
    if let Some(list) = fake() {
        return list.into_iter().find(|h| h.date == date).map(|h| h.name);
    }
    let year: i32 = date.get(0..4)?.parse().ok()?;
    let data: String = conn.query_row("SELECT data FROM holidays WHERE year=?1 AND region=?2", params![year, region], |r| r.get(0)).optional().ok()??;
    serde_json::from_str::<Vec<H>>(&data).ok()?.into_iter().find(|h| h.date == date).map(|h| h.name)
}
