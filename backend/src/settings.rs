use crate::error::{AppJson, Res, AppError};
use crate::{db, AppState};
use axum::extract::State;
use axum::Json;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct Settings {
    pub assistant_name: String,
    pub user_name: String,
    pub voice: String,
    pub location_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
    pub gemini_api_key_set: bool,
}

#[derive(Deserialize)]
pub struct SettingsIn {
    assistant_name: Option<String>,
    user_name: Option<String>,
    voice: Option<String>,
    location_name: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
    gemini_api_key: Option<String>,
}

fn env_key() -> Option<String> {
    std::env::var("GEMINI_API_KEY").ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// DB value wins over the environment variable.
pub fn gemini_key(conn: &Connection) -> Option<String> {
    db::get_setting(conn, "gemini_api_key")
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(env_key)
}

pub fn load(conn: &Connection) -> rusqlite::Result<Settings> {
    let get = |k: &str, d: &str| db::get_setting(conn, k).map(|v| v.unwrap_or_else(|| d.to_string()));
    let num = |k: &str, d: f64| -> rusqlite::Result<f64> {
        Ok(db::get_setting(conn, k)?.and_then(|v| v.parse().ok()).unwrap_or(d))
    };
    Ok(Settings {
        assistant_name: get("assistant_name", "Sunny")?,
        user_name: get("user_name", "")?,
        voice: get("voice", "en-US-JennyNeural")?,
        location_name: get("location_name", "Sydney")?,
        latitude: num("latitude", -33.8688)?,
        longitude: num("longitude", 151.2093)?,
        timezone: get("timezone", db::DEFAULT_TZ)?,
        gemini_api_key_set: gemini_key(conn).is_some(),
    })
}

pub async fn get(State(st): State<AppState>) -> Res<Json<Settings>> {
    let conn = st.db.lock().unwrap();
    Ok(Json(load(&conn)?))
}

pub async fn put(State(st): State<AppState>, AppJson(b): AppJson<SettingsIn>) -> Res<Json<Settings>> {
    let conn = st.db.lock().unwrap();
    if let Some(lat) = b.latitude {
        if !(-90.0..=90.0).contains(&lat) {
            return Err(AppError::bad("latitude must be between -90 and 90"));
        }
    }
    if let Some(lon) = b.longitude {
        if !(-180.0..=180.0).contains(&lon) {
            return Err(AppError::bad("longitude must be between -180 and 180"));
        }
    }
    if let Some(tz) = &b.timezone {
        if tz.trim().parse::<chrono_tz::Tz>().is_err() {
            return Err(AppError::bad("timezone must be an IANA name such as Australia/Sydney"));
        }
    }
    let strs = [
        ("assistant_name", &b.assistant_name),
        ("user_name", &b.user_name),
        ("voice", &b.voice),
        ("location_name", &b.location_name),
        ("gemini_api_key", &b.gemini_api_key),
    ];
    for (k, v) in strs {
        if let Some(v) = v {
            if k == "assistant_name" && v.trim().is_empty() {
                return Err(AppError::bad("assistant_name cannot be empty"));
            }
            if k == "voice" && v.trim().is_empty() {
                return Err(AppError::bad("voice cannot be empty"));
            }
            db::set_setting(&conn, k, v.trim())?;
        }
    }
    if let Some(tz) = &b.timezone {
        db::set_setting(&conn, "timezone", tz.trim())?;
        db::set_timezone(tz.trim());
    }
    if let Some(v) = b.latitude {
        db::set_setting(&conn, "latitude", &v.to_string())?;
    }
    if let Some(v) = b.longitude {
        db::set_setting(&conn, "longitude", &v.to_string())?;
    }
    Ok(Json(load(&conn)?))
}
