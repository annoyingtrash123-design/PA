//! pa-core: the personal assistant's logic as a pure, stateless library.
//!
//! The whole app state is one JSON document (`model::State`). Every call takes the current state and
//! returns the new state if it changed; nothing here reads the clock or does I/O. The REST semantics
//! of `docs/API.md` are exposed through [`call`]; endpoints that would need the network return
//! "prepare" responses (prompts, cache keys, template text) for the UI to use. See `docs/WEB_ARCH.md`.
/// The shared "everything is read aloud" rule for every Gemini system prompt (a literal, so it can sit in `concat!`).
macro_rules! spoken_rule {
    () => {
        "Everything you write will be read aloud by a voice. Write every part in complete, natural spoken sentences, the way a warm person talks to a friend: a subject and a verb in every sentence, with flowing connectives (so, and, which means, before, after). Never use clipped headline fragments or lists (not 'Hot and muggy today, 27 degrees.' but 'It's going to be hot and muggy today, getting up to 27 degrees.'; not 'Light clothes, water close.' but 'I'd wear something light and keep some water with you.'). Write numbers and times the way they would be said aloud ('10:15 AM' is fine, 'ten fifteen in the morning' is fine); avoid symbols like °, %, / and &. "
    };
}
pub mod briefing;
pub mod daynote;
pub mod digest;
pub mod env;
pub mod error;
pub mod goals;
pub mod gym;
pub mod holidays;
pub mod leisure;
pub mod memory;
pub mod model;
pub mod news;
pub mod router;
pub mod seed;
pub mod tasks;
pub mod taskvoice;
pub mod util;
pub mod weather;

pub use briefing::has_stats;
pub use model::{Ctx, State};
pub use router::{call_typed, Response};

/// A fresh State with defaults only (categories, Push/Pull/Legs/Cardio routine and rotation; no tasks/goals/sessions).
pub fn init_state(today: &str) -> String {
    serde_json::to_string(&seed::init_state(today)).unwrap_or_else(|_| "{}".into())
}

/// `{"status": 200, "body": ..., "state": <new State> | null}` as a JSON string.
pub fn call(state: &str, method: &str, path: &str, body: &str, ctx: &str) -> String {
    let r = call_typed(state, method, path, body, ctx);
    serde_json::to_string(&r).unwrap_or_else(|_| r#"{"status":500,"body":{"error":"serialisation failed"},"state":null}"#.into())
}

#[cfg(test)]
mod tests;
