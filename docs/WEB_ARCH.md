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
  "version": 8,
  "next_id": 100,
  "settings": {"assistant_name":"Miranda","user_name":"","voice":"", "location_name":"Sydney",
               "latitude":-33.8688,"longitude":151.2093,"timezone":"Australia/Sydney"},
  "categories": [Category],
  "tasks": [Task],                          // incl. "category_auto"
  "completions": {"<task_id>|<date>": true},       // recurring: per date; one-off: date ""
  "checklist_done": {"<task_id>|<date>|<item_id>": true},
  "rolled_over": {"<task_id>": "<original date>"},
  "missed": {"<task_id>": "<original date>"},     // one-offs with a clock time that were not done in time (date unchanged)
  "learned_minutes": {"<task title, lower case>": 30},  // durations the user set, remembered per title
  "news_used": {"<date>|<kind>": [NewsItem]},           // stories already used in summaries
  "days_off": ["YYYY-MM-DD"],
  "goals": [Goal],
  "leisure": [Leisure],                      // things to watch/read/listen/play/try (API.md "Leisure")
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
  `{"kind","date","weather","stats","affirmation","template_text","template_body","template_parts","news","cache_key","plan_fingerprint","system_prompt","user_prompt","opening","max_words"}`.
  `system_prompt`/`user_prompt` are for the plain-text "day" request. For mornings, `opening` = `{system_prompt, user_prompt}` is a separate small
  request for the greeting, weather comment and affirmation (null for evening and weekly); `news` carries its own prompts (see News).
  UI (`services/ai-text.ts`): if a key is saved and the cache has no entry for `cache_key`, it sends up to THREE small independent Gemini requests in
  parallel: the opening (JSON, morning only), the day (plain text, streamed) and the news (JSON, streamed). Each can fail on its own and falls back to the
  built-in version of just that part. There is no editor or review pass. The day reply is accepted if `!has_stats(reply)` (one retry with a stricter reminder
  while the budget is comfortable), else the built-in `template_text` is shown. Recent summaries (`services/ai-review.ts`: for a morning, last night's,
  yesterday morning's and the night before's; for an evening, this morning's, last night's and yesterday morning's) are appended to the prompts so the writer
  doesn't repeat itself. Accepted replies are cached. Shown text = cached/Gemini text or the built-in text.
- `GET /api/digest?date=` → `{"date","weather","cache_key","system_prompt","user_prompt","template":{"weather_comment","overview"}}`.
  Gemini reply is JSON `{"weather_comment","overview"}` (parse tolerantly; guard only the overview).
- `GET /api/day-note?date=` → `{"date","cache_key","system_prompt","user_prompt","template_text"}`.
- Auto-categorisation: `POST/PUT /api/tasks` with `"category_id":"auto"` categorises immediately with the
  keyword table (so it works offline), sets `category_auto: true`. Gemini is not used for this (the free tier is tiny).
- Gemini budget (`app/src/services/ai-budget.ts`, `gemini.ts`): at most 12 text requests per local date (localStorage
  `pa-ai-usage`), plus "Allow 3 more today" in Settings (`pa-ai-extra`, that day only). A summary that failed is retried automatically at most
  twice a day (`pa-ai-auto-tries`, `MAX_AUTO_TRIES`), then waits for the "Try again" button, so reopening the app doesn't use up requests. Long replies
  are streamed (no fixed time limit while words keep arriving, with a cap). A 503 and similar "busy" errors don't count: wait a moment and retry once, then
  try the lite model; timeouts also fall back to the lite model. A 429/RESOURCE_EXHAUSTED stores `pa-ai-paused-until` (from "Please retry in XhYmZs", else next US Pacific
  midnight) and blocks all calls; the main model's own pause (`pa-ai-main-paused-until`) falls back to `gemini-3.8-flash-lite`.
  Texts are written once per (kind, date) (weekly: per week start), stored in `pa-ai-cache` as `morning:DATE` / `evening:DATE` /
  `week:MONDAY` with the briefing's `plan_fingerprint` (core: items' titles/times/parts of day + workout name; evening also
  done states; weekly done titles + sessions; never weather). Only the first text is automatic; weather refreshes, edits and
  done-states never regenerate. If the fingerprint differs, a "Plans changed — update summary" button regenerates on tap
  (budget/pause respected, toast if blocked). Weekly is automatic for the current week only; other weeks show a cached text or
  the template with a "Write overview" button. Rules live in `services/ai-policy.ts` (unit tested).
- Holidays: UI fetches `https://date.nager.at/api/v3/PublicHolidays/{year}/AU` (country from the region prefix),
  then `POST /api/holidays/filter {"region":"AU-NSW","raw":[...]}` → `[{"date","name"}]`; caches per year+region; passes in ctx.
- TTS: UI only. Natural voice = Gemini TTS (the default when a key is saved, see "Natural voice" below); the phone's Web Speech API is only the fallback.

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
`State.version` is 8. Every `call` first upgrades an older state (`seed::migrate`) and, when it did, returns the upgraded state
in `state` even for a GET, so the UI persists it; `startCore` additionally calls `GET /api/state/migrate` (body `{"version":8}`) once
for a stored v1 state. v1 -> v2: if the gym templates are exactly the six old generic ones (Push A, Push B, Pull A, Pull B, Legs A,
Legs B; never customised) the whole gym section is replaced by the new defaults (sessions are dropped when all belong to those
templates; otherwise they are kept with the exercises they reference). Any other state only gains the new fields (serde defaults).
`POST /api/import` migrates the imported backup the same way.

v8 adds `leisure` (empty) and `memory.interests` (filled with the default interests text when empty). v7 adds `missed` (empty after migration). v5 adds `gym.next_since` (date the current next rotation item became next); the migration sets it to today. After the
migration, every call (except the date-free ones) runs `gym::roll_missed_days`: a rest item that has been next for a passed day
counts as taken and the rotation advances (state returned); workouts never advance on their own. See API.md (`/api/gym/skip-rest`, `/api/gym/skip`).

## Natural voice (Gemini TTS)
UI only (`services/gemini-tts.ts`, `readaloud.ts`, `voice-prefs.ts`); no core changes. Core settings have a fixed field set, so prefs live in
localStorage `pa-voice-prefs` `{type:"gemini"|"phone", voice, style, speed, favs}` (default type: Natural if a key is saved, else Phone).

**Research caveat:** `ai.google.dev` was unreachable from the build sandbox, so this comes from search snippets/mirrors of the docs
and prior knowledge. Re-verify against https://ai.google.dev/gemini-api/docs/speech-generation before relying on it.
- Models: `gemini-3.1-flash-tts-preview` (newest flash TTS; model page: 8,192-token input, 16,384 output) and
  `gemini-2.5-flash-preview-tts` (older; 32k context). `gemini-2.5-pro-preview-tts` exists but is not free-tier. All are previews.
  `TTS_MODELS` in `gemini-tts.ts` is the ordered list; a 404/410/"not found" moves to the next model.
- Request: `POST /v1beta/models/{model}:generateContent`, `contents:[{parts:[{text}]}]`,
  `generationConfig:{responseModalities:["AUDIO"], speechConfig:{voiceConfig:{prebuiltVoiceConfig:{voiceName}}}}`. Text input only.
- Response: `candidates[0].content.parts[0].inlineData{mimeType:"audio/L16;codec=pcm;rate=24000", data:<base64>}` = raw 16-bit
  mono little-endian PCM at 24 kHz. We read the rate from the mimeType (default 24000) and wrap it in a WAV header.
- Voices: only female voices are offered (`VOICES` in `gemini-tts.ts`, 14 of them): Sulafat Warm, Achernar Soft, Aoede Breezy, Vindemiatrix Gentle,
  Callirrhoe Easy-going, Despina Smooth, Laomedeia Upbeat, Leda Youthful, Zephyr Bright, Autonoe Bright, Erinome Clear, Kore Firm, Gacrux Mature,
  Pulcherrima Forward. Default for Miranda: **Sulafat** (Warm). Settings also has a speed slider (0.7 to 1.15, default 0.88; applied at playback with
  `playbackRate`, so it needs no new synthesis) and favourite voices (stars; `pa-voice-prefs.favs`, listed first).
- Style: natural-language instruction in the text, e.g. `Say warmly and chattily: <text>` (presets: Warm & chatty, Calm & soothing,
  Upbeat, Matter-of-fact). Each summary is read as three clips (see "Spoken sections" below), so a request is short; text over 12,000 chars is cut at a paragraph/sentence.
- Free tier: TTS preview models have small per-minute/per-day request limits (not confirmed here; historically ~3 RPM / ~10 RPD for 2.5 flash TTS).
  So voice has its own separate budget of 12 clips a day (`VOICE_BUDGET`, `pa-voice-usage`), a 429 stores `pa-tts-paused-until` (TTS only; text keeps working
  unless the global pause is set), the timeout is 90 s, and results are cached.
- Cache: IndexedDB `pa-tts-cache`, key = hash(first model id + voice + style + text); newest 16 clips kept, clips older than 3 days pruned;
  voice previews (fixed sentence) are kept forever. Replays are instant and free.
- Playback: `HTMLAudioElement` (a silent clip is played inside the tap to unlock iOS audio). Any failure (no key, paused, over budget,
  error, autoplay refusal) falls back to the phone voice with a toast.

## News in the summaries
- `scripts/news/fetch-news.mjs` (+ `sources.json`) is run by the scheduled workflow (`pages.yml`) at about 05:45 and 16:45 Sydney time all year round. GitHub
  crons are UTC and Sydney moves between UTC+11 and UTC+10, so four crons are listed (`45 18`, `45 19`, `45 5`, `45 6`) and a "gate" job lets through only the
  run that lands in the right Sydney window (pushes and manual runs always go ahead). It writes
  `app/public/news.json` = `{generated_at, items:[{id, source, title, summary (the feed's lead, <=700 chars), link, published, topics}]}`
  (last 36 h, near-duplicate titles merged, <=80 items). Topics: australia, world, east-asia, science-tech, life; the script and the core add
  `industry-tech` by keywords. Feeds that fail are skipped; article pages are only read (<=25/run) when a feed gives a lead under 120 chars.
- App: `services/news.ts` fetches `./news.json` (network first, last copy in IndexedDB `pa-news`) into `ctxCache.news`; `core.ts` passes it as
  `ctx.news` / `ctx.news_generated_at` to `/api/briefing` and `/api/news/*` calls only. news.json is never precached by the service worker.
- Core (`news.rs`): `GET /api/briefing` (morning/evening, today only) adds `news` (items, sources, position 0/1/2, recorded, template_text, ...) and
  `template_body`, and puts the NEWS instructions + story block (title + lead) into the prompts. Two stories in the morning and two in the evening ("since this morning":
  published after `news_morning_cutoff[date]`): an Australian lead, then a second that rotates by day (mostly a lighter "life" story, else world and so on);
  one story per event, spread over sources. Each is told as a short, complete story (about 110 to 160 words), not a headline. `POST /api/news/select {kind,date,refresh?,items?}` returns the same plus a
  news-only prompt; `POST /api/news/record {kind,date,items,generated_at?}` stores the stories a summary used in `State.news_used["date|kind"]`
  (and the morning cutoff) when its news is first generated (AI or template); nothing used in the last ~48 h is picked again, and re-viewing
  returns the recorded stories.
- The news sits inside the summary: morning position varies by date hash (0 after greeting/weather/affirmation, 1 after the first paragraph of the
  briefing, 2 before the last paragraph); the evening is always 2. The news is its own small JSON request (`news` in the briefing prep has its own prompts), so it can fail without
  losing the day text; the AI news is kept only if it quotes at least half of the headlines word for word, else the template segment is used. News text is
  stored under `<kind:DATE>:news` in `pa-ai-cache`. "Get the latest news" = one news-only request.

## Reliability, sections and voice (update)
- AI replies: thinking stays low. `parseMorningJson` strips fences and salvages the complete fields of a truncated
  reply (a usable `briefing` is enough; missing parts use the template). `has_stats` (core) ignores text inside quotation marks and the app guards only the
  non-news parts. The news must quote at least half of the headlines after normalising quotes, case, accents and punctuation (`normHeadline`).
  Why the built-in text was shown is classified in `services/ai-reason.ts` and shown as a label at the top of the summary card ("Written by Miranda's AI" /
  "Built-in summary — <reason>"), with a "Try again" button after a Google hiccup.
- Spoken sections: each briefing is three sections (`morningSections` / `eveningSections`; the news sits first or last, never in the middle: core `news::position`
  is 0 or 2). The screen shows one continuous text; "Read it to me" synthesises one clip per section (a section over ~300 words is split at paragraph/sentence
  ends; without sections `chunkScript` splits at ~240 words), prefetches the next clip while one plays, pauses ~1 s between sections, shows progress across clips
  and continues with the phone voice if a later clip fails. Each clip is cached by its text (newest 16 kept). Voice budget: 12 clips per day.
- Prompts carry the shared SPOKEN STYLE GUIDE (`lib.rs` `style_guide!`/`STYLE_GUIDE`): light natural fillers and pauses; headlines stay verbatim.
- Built-in summary (no AI): deliberately short and plain, not an imitation of the AI voice. Morning: today, the order, tomorrow, closing. Evening: look back,
  tomorrow, goodnight (news, when there is any, before the goodnight). Imperative titles become gerund phrases (`taskvoice::task_phrase`), stock lines never
  repeat within a summary, at most one goal mention (`goals::find_link` ignores generic words), a varied closing well-wish, greetings are hellos.
- Knowledge base (`knowledge.rs`): Miranda's own small memories, facts, tips and thoughts, written in our own words. A few are picked per summary to fit the
  day (weather, tasks, time of day, goals) and stepped through by date so they don't repeat; the model may use one or two, or none.

## Task brain and task moves
- `taskbrain.rs` is the single table deciding a task's category, typical duration, whether it is an outing (travel on top) and the natural wording
  ("go to the doctor" becomes "a doctor's appointment"). Category guessing, duration hints in the prompts and `taskvoice` all use it.
- Tasks and Occurrences have an optional `minutes` (how long the user says it takes). Setting it is remembered per task title in `State.learned_minutes`
  and beats the table's guess; clearing it (null) on update also forgets the remembered figure. `GET /api/tasks/guess?title=` returns what Miranda would
  say about a title (`says`, `category`, `takes`, `learned`); the task dialog shows it as "Miranda reads this as: ..." next to the "How long does it take?" field.
- Rollover: any unfinished one-off with no clock time (including untimed appointments) moves to today automatically; only clock-time tasks go to `missed`.
  Untimed Work tasks are shown inside the work band and spoken as part of the workday.
- Moving: press-and-hold the arrow on a one-off task (`POST /api/occurrences/move`), or the day menu "Move unfinished to tomorrow"
  (`POST /api/days/{date}/move-unfinished`). Recurring and done tasks can't be moved.
- Evening check-in (`util/checkin.ts`, Home wind-down): asks once about tomorrow's unplaced tasks (no time, no part of day, not Work on a work day) with
  Before work / Afternoon / After work buttons (`PUT /api/tasks/{id}/part`). It uses no AI, never holds a summary back, and is remembered per day (`pa-checkin`).

## Safety and testing
- `vite.config.ts` adds a Content-Security-Policy meta tag to built pages: scripts from the app only (plus wasm), and network only to Gemini, Open-Meteo and
  Nager.Date. This protects the saved Gemini key.
- `core.ts` asks the browser to keep storage (`navigator.storage.persist()`).
- Whole-app phone-screen tests: `app/e2e/run.cjs` (Playwright, `npm run test:app`) opens the BUILT app in a phone-sized browser, with stand-ins for Google
  (one for each of the three small requests), weather and news. `pages.yml` runs it after the build and before publishing (skipped for the scheduled news refresh).
- Tabs: Home, Tasks, Plans (goals + leisure), Gym, Settings. A private profile code (`POST /api/profile/apply`) can fill in interests, notes, work hours,
  weather preferences, goals, leisure and tasks; `/api/profile/clear` resets them.
