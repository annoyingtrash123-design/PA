//! pa-wasm: the thin wasm-bindgen wrapper around pa-core. See docs/WEB_ARCH.md.
use wasm_bindgen::prelude::*;

/// A fresh State (JSON): defaults only, no tasks, goals or sessions.
#[wasm_bindgen]
pub fn init_state(today: &str) -> String {
    pa_core::init_state(today)
}

/// `call(state, method, path, body, ctx)` -> `{"status", "body", "state": <new State> | null}` (JSON).
#[wasm_bindgen]
pub fn call(state: &str, method: &str, path: &str, body: &str, ctx: &str) -> String {
    pa_core::call(state, method, path, body, ctx)
}

/// True when the text reads out a count, fraction or percentage ("3 of 5", "7 out of 10", "percent", "%").
#[wasm_bindgen]
pub fn has_stats(text: &str) -> bool {
    pa_core::has_stats(text)
}
