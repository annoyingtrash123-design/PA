//! Small shared helpers: dates, spoken times, list joining, deterministic variety, hashing.
use chrono::{Datelike, NaiveDate};

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

pub fn weekday_name(d: NaiveDate) -> String {
    d.format("%A").to_string()
}

/// "07:30" -> "7:30 AM", "10:00" -> "10 AM".
pub fn say_time(t: &str) -> String {
    let h: u32 = t.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(0);
    let m: u32 = t.get(3..5).and_then(|x| x.parse().ok()).unwrap_or(0);
    let (h12, ap) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    if m == 0 {
        format!("{h12} {ap}")
    } else {
        format!("{h12}:{m:02} {ap}")
    }
}

/// Minutes since midnight for "HH:MM" (12:00 when unparseable).
pub fn minutes(t: &str) -> u32 {
    let h: u32 = t.get(0..2).and_then(|x| x.parse().ok()).unwrap_or(12);
    let m: u32 = t.get(3..5).and_then(|x| x.parse().ok()).unwrap_or(0);
    h * 60 + m
}

pub fn join(v: &[String]) -> String {
    match v.len() {
        0 => String::new(),
        1 => v[0].clone(),
        2 => format!("{} and {}", v[0], v[1]),
        _ => format!("{} and {}", v[..v.len() - 1].join(", "), v[v.len() - 1]),
    }
}

/// Join at most `max` entries, then "plus a few more".
pub fn limited(v: Vec<String>, max: usize) -> String {
    if v.len() <= max {
        join(&v)
    } else {
        let extra = match v.len() - max {
            1 => "one more",
            2 => "a couple more",
            _ => "a few more",
        };
        format!("{}, plus {extra}", v[..max].join(", "))
    }
}

pub fn ordinal(n: u32) -> String {
    let suf = match (n % 100, n % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suf}")
}

/// Deterministic pseudo-random index for a date + salt (stable text per day).
pub fn idx(d: NaiveDate, salt: u64, len: usize) -> usize {
    let h = (d.num_days_from_ce() as u64)
        .wrapping_add(salt.wrapping_mul(7919))
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((h >> 33) as usize) % len
}

pub fn pick<'a>(opts: &[&'a str], d: NaiveDate, salt: u64) -> &'a str {
    opts[idx(d, salt, opts.len())]
}

pub fn cap(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

pub fn words_in(s: &str) -> usize {
    s.split_whitespace().count()
}

/// FNV-1a 64-bit, as 16 hex digits. Stable across platforms (used for cache keys).
pub fn fnv(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

pub fn name_suffix(user: &str) -> String {
    if user.is_empty() {
        String::new()
    } else {
        format!(", {user}")
    }
}

/// Percent-decode a query component ('+' is a space).
pub fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(v) => {
                        out.push(v);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
