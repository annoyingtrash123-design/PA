use crate::AppState;
use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Serialize, Clone, Debug)]
pub struct Weather {
    pub summary: String,
    pub temp_max: i64,
    pub temp_min: i64,
}

pub fn wmo_summary(code: i64) -> &'static str {
    match code {
        0 => "Clear sky",
        1 => "Mostly clear",
        2 => "Partly cloudy",
        3 => "Overcast",
        45 | 48 => "Foggy",
        51 => "Light drizzle",
        53 => "Drizzle",
        55 => "Heavy drizzle",
        56 | 57 => "Freezing drizzle",
        61 => "Light rain",
        63 => "Rain",
        65 => "Heavy rain",
        66 | 67 => "Freezing rain",
        71 => "Light snow",
        73 => "Snow",
        75 => "Heavy snow",
        77 => "Snow grains",
        80 => "Light rain showers",
        81 => "Rain showers",
        82 => "Heavy rain showers",
        85 | 86 => "Snow showers",
        95 => "Thunderstorm",
        96 | 99 => "Thunderstorm with hail",
        _ => "Mixed weather",
    }
}

async fn fetch(st: &AppState, lat: f64, lon: f64, date: &str) -> Option<Weather> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}&daily=weather_code,temperature_2m_max,temperature_2m_min&timezone=auto&start_date={date}&end_date={date}"
    );
    let resp = st.http.get(url).timeout(Duration::from_secs(8)).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().await.ok()?;
    let daily = v.get("daily")?;
    let code = daily.get("weather_code")?.get(0)?.as_i64()?;
    let max = daily.get("temperature_2m_max")?.get(0)?.as_f64()?;
    let min = daily.get("temperature_2m_min")?.get(0)?.as_f64()?;
    Some(Weather { summary: wmo_summary(code).to_string(), temp_max: max.round() as i64, temp_min: min.round() as i64 })
}

/// Weather for a date; `None` on any failure. Successful lookups are cached per date and location.
pub async fn for_date(st: &AppState, lat: f64, lon: f64, date: &str) -> Option<Weather> {
    let key = format!("{date}|{lat:.2}|{lon:.2}");
    if let Some(w) = st.weather_cache.lock().unwrap().get(&key) {
        return Some(w.clone());
    }
    let w = fetch(st, lat, lon, date).await;
    if let Some(w) = &w {
        st.weather_cache.lock().unwrap().insert(key, w.clone());
    } else {
        tracing::warn!("weather lookup failed for {date}");
    }
    w
}
