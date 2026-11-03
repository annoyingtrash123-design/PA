//! Public holidays: the UI fetches Nager.Date and passes the raw JSON to `filter`;
//! the filtered list comes back in `ctx.holidays` and is looked up by date.
use crate::model::{Ctx, Holiday};
use serde_json::Value;

/// Keep entries where `global` is true or `counties` contains the region.
pub fn filter(region: &str, raw: &Value) -> Vec<(String, String)> {
    let mut out = vec![];
    let Some(list) = raw.as_array() else { return out };
    if region.trim().is_empty() {
        return out;
    }
    for h in list {
        let global = h["global"].as_bool().unwrap_or(false);
        let in_region = h["counties"].as_array().is_some_and(|c| c.iter().any(|x| x.as_str() == Some(region)));
        if global || in_region {
            if let (Some(d), Some(n)) = (h["date"].as_str(), h["name"].as_str()) {
                out.push((d.to_string(), n.to_string()));
            }
        }
    }
    out
}

/// Name of the public holiday on `date`, from the context list.
pub fn lookup(ctx: &Ctx, date: &str) -> Option<String> {
    ctx.holidays.iter().find(|h: &&Holiday| h.date == date).map(|h| h.name.clone())
}
