//! Per-call environment: the (mutable) state, the UI context and a "state changed" flag.
use crate::model::{Ctx, State};
use chrono::NaiveDate;

pub struct Env<'a> {
    pub st: &'a mut State,
    pub ctx: &'a Ctx,
    pub today: NaiveDate,
    pub changed: bool,
}

impl Env<'_> {
    /// Current local time "HH:MM" from the context.
    pub fn now(&self) -> String {
        let n = self.ctx.now.trim();
        if n.len() >= 5 && n.get(2..3) == Some(":") {
            n[..5].to_string()
        } else {
            "12:00".to_string()
        }
    }
}
