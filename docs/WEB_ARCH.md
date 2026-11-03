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
  "version": 7,
  "next_id": 100,
  "settings": {"assistant_name":"Miranda","user_name":"","voice":"", "location_name":"Sydney",
               "latitude":-33.8688,"longitude":151.2093,"timezone":"Australia/Sydney"},
  "categories": [Category],
  "tasks": [Task],                          // incl. "category_auto"
  "completions": {"<task_id>|<date>": true},       // recurring: per date; one-off: date ""
  "checklist_done": {"<task_id>|<date>|<item_id>": true},
  "rolled_over": {"<task_id>": "<original date>"},
  "missed": {"<task_id>": "<original date>"},     // timed / Appointments one-offs that were not done in time (date unchanged)
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
  Texts are written once per (kind, date) (weekly: per week start), stored in `pa-ai-cache` as `morning:DATE` / `evening:DATE` /
  `week:MONDAY` with the briefing's `plan_fingerprint` (core: items' titles/times/parts of day + workout name; evening also
  done states; weekly done titles + sessions; never weather). Only the first text is automatic; weather refreshes, edits and
  done-states never regenerate. If the fingerprint differs, a "Plans changed — update summary" button regenerates on tap
  (budget/pause respected, toast if blocked). Weekly is automatic for the current week only; other weeks show a cached text or
  the template with a "Write overview" button. Rules live in `services/ai-policy.ts` (unit tested).
- Holidays: UI fetches `https://date.nager.at/api/v3/PublicHolidays/{year}/AU` (country from the region prefix),
  then `POST /api/holidays/filter {"region":"AU-NSW","raw":[...]}` → `[{"date","name"}]`; caches per year+region; passes in ctx.
- TTS: UI only. Phone voice = Web Speech API; Natural voice = Gemini TTS (see "Natural voice" below).

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
`State.version` is 7. Every `call` first upgrades an older state (`seed::migrate`) and, when it did, returns the upgraded state
in `state` even for a GET, so the UI persists it; `startCore` additionally calls `GET /api/state/migrate` (body `{"version":7}`) once
for a stored v1 state. v1 -> v2: if the gym templates are exactly the six old generic ones (Push A, Push B, Pull A, Pull B, Legs A,
Legs B; never customised) the whole gym section is replaced by the new defaults (sessions are dropped when all belong to those
templates; otherwise they are kept with the exercises they reference). Any other state only gains the new fields (serde defaults).
`POST /api/import` migrates the imported backup the same way.

v7 adds `missed` (empty after migration). v5 adds `gym.next_since` (date the current next rotation item became next); the migration sets it to today. After the
migration, every call (except the date-free ones) runs `gym::roll_missed_days`: a rest item that has been next for a passed day
counts as taken and the rotation advances (state returned); workouts never advance on their own. See API.md (`/api/gym/skip-rest`, `/api/gym/skip`).

## Natural voice (Gemini TTS)
UI only (`services/gemini-tts.ts`, `readaloud.ts`, `voice-prefs.ts`); no core changes. Core settings have a fixed field set, so prefs live in
localStorage `pa-voice-prefs` `{type:"gemini"|"phone", voice, style}` (default type: Natural if a key is saved, else Phone).

**Research caveat:** `ai.google.dev` was unreachable from the build sandbox, so this comes from search snippets/mirrors of the docs
and prior knowledge. Re-verify against https://ai.google.dev/gemini-api/docs/speech-generation before relying on it.
- Models: `gemini-3.1-flash-tts-preview` (newest flash TTS; model page: 8,192-token input, 16,384 output) and
  `gemini-2.5-flash-preview-tts` (older; 32k context). `gemini-2.5-pro-preview-tts` exists but is not free-tier. All are previews.
  `TTS_MODELS` in `gemini-tts.ts` is the ordered list; a 404/410/"not found" moves to the next model.
- Request: `POST /v1beta/models/{model}:generateContent`, `contents:[{parts:[{text}]}]`,
  `generationConfig:{responseModalities:["AUDIO"], speechConfig:{voiceConfig:{prebuiltVoiceConfig:{voiceName}}}}`. Text input only.
- Response: `candidates[0].content.parts[0].inlineData{mimeType:"audio/L16;codec=pcm;rate=24000", data:<base64>}` = raw 16-bit
  mono little-endian PCM at 24 kHz. We read the rate from the mimeType (default 24000) and wrap it in a WAV header.
- Voices (30): Zephyr Bright, Puck Upbeat, Charon Informative, Kore Firm, Fenrir Excitable, Leda Youthful, Orus Firm, Aoede Breezy,
  Callirrhoe Easy-going, Autonoe Bright, Enceladus Breathy, Iapetus Clear, Umbriel Easy-going, Algieba Smooth, Despina Smooth,
  Erinome Clear, Algenib Gravelly, Rasalgethi Informative, Laomedeia Upbeat, Achernar Soft, Alnilam Firm, Schedar Even,
  Gacrux Mature, Pulcherrima Forward, Achird Friendly, Zubenelgenubi Casual, Vindemiatrix Gentle, Sadachbia Lively,
  Sadaltager Knowledgeable, Sulafat Warm. Default for Miranda: **Sulafat** (Warm).
- Style: natural-language instruction in the text, e.g. `Say warmly and chattily: <text>` (presets: Warm & chatty, Calm & soothing,
  Upbeat, Matter-of-fact). One request per script (a 2-3 minute script is ~700 tokens, far under the limit); text over 12,000 chars is cut at a paragraph/sentence.
- Free tier: TTS preview models have small per-minute/per-day request limits (not confirmed here; historically ~3 RPM / ~10 RPD for 2.5 flash TTS).
  So each request counts toward the 12/day app budget (`recordUse`), a 429 stores `pa-tts-paused-until` (TTS only; text keeps working
  unless the global pause is set), the timeout is 90 s, and results are cached.
- Cache: IndexedDB `pa-tts-cache`, key = hash(first model id + voice + style + text); newest 10 clips kept, clips older than 3 days pruned;
  voice previews (fixed sentence) are kept forever. Replays are instant and free.
- Playback: `HTMLAudioElement` (a silent clip is played inside the tap to unlock iOS audio). Any failure (no key, paused, over budget,
  error, autoplay refusal) falls back to the phone voice with a toast.
