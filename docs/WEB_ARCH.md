# Web app architecture (GitHub Pages, no server)

Same pattern as the owner's other apps (e.g. Chengyu-App): a Rust core compiled to WebAssembly,
a vanilla TypeScript + Vite UI, installable PWA, published to GitHub Pages by a GitHub Actions workflow.
Everything runs on the phone. No server.

```
Cargo.toml          workspace: members = ["core", "wasm"]
core/               pa-core: pure Rust logic, no I/O (no rusqlite/axum/reqwest/tokio). Unit tested.
wasm/               pa-wasm: wasm-bindgen wrapper exposing `call` (below). crate-type cdylib+rlib.
app/                Vite + TypeScript UI, vite-plugin-pwa. `npm run build:web` = wasm-pack build + vite build.
.github/workflows/pages.yml   build + deploy to Pages on push to the default branch.
```
The old `backend/` (axum server) and `frontend/` (Flet) are the reference implementation being
ported; they will be deleted once the port is complete.

## Data
The whole app state is ONE JSON document (`State`), owned by the UI layer and persisted in
IndexedDB (key `pa-state`). The Rust core is stateless: every call takes the current state and
returns the new state if it changed. The UI saves it after every mutating call.
The Gemini API key is NOT in State (kept in localStorage `pa-gemini-key`), so backup files never contain it.
AI-generated texts are cached by the UI in IndexedDB (`pa-ai-cache`, keyed by the `cache_key` the core returns).

```json
State = {
  "version": 6,
  "next_id": 100,
  "settings": {"assistant_name":"Miranda","user_name":"","voice":"", "location_name":"Sydney",
               "latitude":-33.8688,"longitude":151.2093,"timezone":"Australia/Sydney"},
  "categories": [Category],
  "tasks": [Task],                          // incl. "category_auto"
  "completions": {"<task_id>|<date>": true},       // recurring: per date; one-off: date ""
  "checklist_done": {"<task_id>|<date>|<item_id>": true},
  "rolled_over": {"<task_id>": "<original date>"},
  "days_off": ["YYYY-MM-DD"],
  "goals": [Goal],
  "memory": Memory,                         // weather, work (incl lunch_minutes, holiday_region), rhythm, notes
  "gym": {"exercises":[Exercise],"templates":[Template],"rotation":Rotation,"sessions":[Session]}
}
```
Shapes of Category, Task, Occurrence, Day, Goal, Memory, Exercise, Template, Rotation, Session,
WorkoutPlan are exactly as in `docs/API.md`. The core may add internal fields but must keep those.

## The one wasm entry point
```rust
#[wasm_bindgen] pub fn init_state(today: &str) -> String            // fresh State: defaults only (see below)
#[wasm_bindgen] pub fn call(state: &str, method: &str, path: &str, body: &str, ctx: &str) -> String
#[wasm_bindgen] pub fn has_stats(text: &str) -> bool                 // the "N of M"/percent guard
```
`call` mirrors the REST API in `docs/API.md` — same methods, paths, query strings, bodies and
response bodies — so the UI can be ported 1:1 from `frontend/api.py`.
Returns `{"status": 200, "body": <response JSON>, "state": <new State> | null}`
(`state` null when unchanged; status 400/404 with body `{"error": "..."}` on errors).

`ctx` (JSON, built by the UI on every call):
```json
{"today":"2026-11-03", "now":"07:42",                     // in the user's timezone (UI computes via Intl)
 "has_key": true,                                           // a Gemini key is saved (for settings.gemini_api_key_set)
 "weather": {"2026-11-03": Weather, ...},                   // whatever the UI has cached; may be {}
 "holidays": [{"date":"2026-12-25","name":"Christmas Day"}] // already filtered to the user's region
}
```
Weather = `{"summary","temp_max","temp_min","precip_prob","uv_max"}`; the UI fetches Open-Meteo and
maps WMO codes using `GET /api/weather-summary?code=61` (core owns the WMO table).

### Endpoints that need the network — replaced by "prepare" responses
The core cannot call the network, so these return everything the UI needs to do it:

- `GET /api/briefing?kind=morning|evening|weekly&date=` → body
  `{"kind","date","weather","stats", "affirmation", "template_text", "cache_key", "system_prompt", "user_prompt", "max_words"}`.
  UI: if a key is saved and `pa-ai-cache[cache_key]` is missing, call Gemini with the prompts; accept the reply if
  `!has_stats(reply)` (retry once with a stricter reminder appended, as the old backend did), else use `template_text`.
  Cache accepted Gemini replies. Shown text = cached/Gemini text or template_text.
- `GET /api/digest?date=` → `{"date","weather","cache_key","system_prompt","user_prompt","template":{"weather_comment","overview"}}`.
  Gemini reply is JSON `{"weather_comment","overview"}` (parse tolerantly; guard only the overview).
- `GET /api/day-note?date=` → `{"date","cache_key","system_prompt","user_prompt","template_text"}`.
- Auto-categorisation: `POST/PUT /api/tasks` with `"category_id":"auto"` categorises immediately with the
  keyword table (so it works offline), sets `category_auto: true`. Gemini is not used for this (the free tier is tiny).
- Gemini budget (`app/src/services/ai-budget.ts`, `gemini.ts`): at most 12 requests per local date (localStorage
  `pa-ai-usage`); a 429/RESOURCE_EXHAUSTED stores `pa-ai-paused-until` (from "Please retry in XhYmZs", else next US Pacific
  midnight) and blocks all calls; the main model's own pause (`pa-ai-main-paused-until`) falls back to `gemini-3.8-flash-lite`.
  Home makes one request per kind per day (the morning one returns `{"weather_comment","briefing"}`); the morning key ignores
  done states; evening/weekly texts are reused for 3 hours per date.
- Holidays: UI fetches `https://date.nager.at/api/v3/PublicHolidays/{year}/AU` (country from the region prefix),
  then `POST /api/holidays/filter {"region":"AU-NSW","raw":[...]}` → `[{"date","name"}]`; caches per year+region; passes in ctx.
- TTS: entirely UI (Web Speech API).

### Extra endpoints
- `POST /api/sample-data` → loads the old seed's demo tasks/goals/sessions into the state (for trying the app).
- `GET /api/export` → State (UI downloads it as a backup file); `POST /api/import` body State → validates, returns it.
- `GET /api/weather-summary?code=N` → `{"summary": "Light rain", "icon": "rain"}`.

### init_state defaults (a real fresh start, no demo tasks)
Settings as above (Sydney); the 10 default categories (API.md); Memory defaults (weather 18/22/25,
likes_rain, avoid_sunburn; work Mon–Fri 10:00–17:00, lunch 60, region AU-NSW; rhythm 06:00–07:00 / 23:00–00:00;
no notes); gym: the owner's routine (Push 1/2, Pull 1/2, Legs + Abs 1/2, Cardio; 4 x 10 for strength moves, start weights, assisted
counterweight machines) and rotation Push 1, Pull 1, Legs + Abs 1, Cardio, Push 2, Pull 2, Legs + Abs 2, Cardio; no sessions, tasks or goals.

### State versions and migration
`State.version` is 6. Every `call` first upgrades an older state (`seed::migrate`) and, when it did, returns the upgraded state
in `state` even for a GET, so the UI persists it; `startCore` additionally calls `GET /api/state/migrate` (body `{"version":6}`) once
for a stored v1 state. v1 -> v2: if the gym templates are exactly the six old generic ones (Push A, Push B, Pull A, Pull B, Legs A,
Legs B; never customised) the whole gym section is replaced by the new defaults (sessions are dropped when all belong to those
templates; otherwise they are kept with the exercises they reference). Any other state only gains the new fields (serde defaults).
`POST /api/import` migrates the imported backup the same way.

v5 adds `gym.next_since` (date the current next rotation item became next); the migration sets it to today. After the
migration, every call (except the date-free ones) runs `gym::roll_missed_days`: a rest item that has been next for a passed day
counts as taken and the rotation advances (state returned); workouts never advance on their own. See API.md (`/api/gym/skip-rest`, `/api/gym/skip`).
