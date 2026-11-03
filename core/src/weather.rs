//! WMO weather-code table (the UI fetches Open-Meteo and asks the core to name the code).

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

/// A coarse icon name for the UI.
pub fn wmo_icon(code: i64) -> &'static str {
    match code {
        0 | 1 => "clear",
        2 => "partly-cloudy",
        3 => "cloudy",
        45 | 48 => "fog",
        51..=57 => "drizzle",
        61..=67 | 80..=82 => "rain",
        71..=77 | 85 | 86 => "snow",
        95..=99 => "thunder",
        _ => "cloudy",
    }
}

/// A summary label ("Thunderstorm", "Light rain") as a natural, grammatical sentence about the day.
/// `past` for the evening recap ("It was rainy today."), otherwise the forecast ("It's going to be rainy today.").
pub fn natural_sentence(summary: &str, past: bool) -> String {
    let s = summary.to_lowercase();
    let has = |k: &str| s.contains(k);
    // (past, future)
    let (p, f): (&str, &str) = if has("thunder") {
        if has("hail") {
            ("Pretty wild weather today, wasn't it? A thunderstorm rolled through, with some hail.", "There's a thunderstorm on the cards today, with a chance of hail.")
        } else {
            ("Pretty wild weather today, wasn't it? A thunderstorm rolled through.", "There's a thunderstorm on the cards today.")
        }
    } else if has("snow") {
        if has("light") {
            ("It snowed lightly today.", "There's a light dusting of snow on the way today.")
        } else {
            ("It snowed today.", "There's snow on the way today.")
        }
    } else if has("freezing") {
        ("There was some freezing rain about today.", "There's some freezing rain about today.")
    } else if has("shower") {
        if has("heavy") {
            ("There were heavy rain showers on and off today.", "There'll be heavy rain showers on and off today.")
        } else {
            ("There were rain showers on and off today.", "There'll be rain showers on and off today.")
        }
    } else if has("drizzle") {
        ("There was a bit of drizzle about today.", "There'll be a bit of drizzle about today.")
    } else if has("rain") {
        if has("heavy") {
            ("It poured with rain today.", "It's going to pour with rain today.")
        } else if has("light") {
            ("It was a little rainy today.", "It's going to be a little rainy today.")
        } else {
            ("It was rainy today.", "It's going to be rainy today.")
        }
    } else if has("fog") {
        ("It was a foggy one today.", "It's going to be foggy today.")
    } else if has("clear") || has("sunny") {
        ("It stayed sunny today.", "It's going to be sunny today.")
    } else if has("partly") {
        ("It was a mix of sun and cloud today.", "It's going to be a mix of sun and cloud today.")
    } else if has("overcast") || has("cloud") {
        ("It was grey and overcast today.", "It's going to be grey and overcast today.")
    } else {
        ("The weather was a bit of a mix today.", "The weather's going to be a bit of a mix today.")
    };
    (if past { p } else { f }).to_string()
}
