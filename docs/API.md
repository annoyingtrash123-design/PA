# Backend API contract (v0 draft)

Base URL: `http://localhost:8080` (env `BACKEND_URL` in the frontend). All bodies JSON. Dates `YYYY-MM-DD`, times `HH:MM` (24h). IDs are integers.
Errors: non-2xx with `{"error": "message"}`.

## Settings
`GET /api/settings` → `Settings`
`PUT /api/settings` body: partial `Settings` → `Settings`
```json
Settings = {
  "assistant_name": "Sunny",
  "user_name": "",                // what the assistant calls you
  "voice": "en-US-JennyNeural",   // or "en-US-AriaNeural"
  "location_name": "Sydney",
  "latitude": -33.8688, "longitude": 151.2093,
  "timezone": "Australia/Sydney", // IANA name; defines "today"/"now" on the (UTC) server; PUT validates it
  "gemini_api_key_set": false     // read-only; key is written via "gemini_api_key" in PUT body, never returned
}
```

## Categories
`GET /api/categories` → `[Category]`; `POST /api/categories` body `{name,color}` → `Category`; `PUT /api/categories/{id}`; `DELETE /api/categories/{id}`
`Category = {"id":1,"name":"Health","color":"#4CAF50"}`

## Tasks
Two kinds share one shape: one-off tasks (have `date`) and recurring tasks (have `recurrence`).
```json
Task = {
  "id": 1,
  "title": "Morning routine",
  "date": "2026-11-02",           // one-off only; null for recurring
  "time": "07:30",                // null if untimed
  "part_of_day": "morning",       // "morning" | "afternoon" | "evening" | "anytime"; used when time is null (if time set, derived from it)
  "category_id": 2,               // nullable
  "notes": "",
  "recurrence": null,             // null, or {"kind":"daily"} | {"kind":"weekdays"} | {"kind":"weekly","days":[0,2,4]}  (0=Mon..6=Sun)
  "checklist": [{"id": 5, "text": "Make bed"}]
}
```
`POST /api/tasks` body: Task without id (checklist items without id) → `Task`
`PUT /api/tasks/{id}` body: full Task (checklist replaced) → `Task`
`DELETE /api/tasks/{id}`

### Auto-categorisation
`POST /api/tasks` and `PUT /api/tasks/{id}` accept `"category_id": "auto"`. The backend then picks one category from the existing list (never creates new ones): Gemini if a key is set (prompt with the title, notes and the category names; answer must be one exact name), else keyword matching; no match → "Other". The returned Task has the chosen numeric `category_id` and `"category_auto": true`. Tasks store `category_auto` so the UI can show "Auto" as the selection on edit; choosing a category manually sets it false.
Default categories (seeded on a fresh DB): Chores, Exercise, Study, Work, Errands, Appointments, Social, Self-care, Admin & Money, Other — muted colours that read well on a dark warm-grey background.

### Day / week views (resolved occurrences)
`GET /api/days?start=2026-11-02&days=7` → `[Day]`
```json
Day = {
  "date": "2026-11-02",
  "weekday": "Monday",
  "items": [Occurrence],          // sorted: timed by time, then untimed by part_of_day order morning, afternoon, evening, anytime
  "workout": WorkoutPlan | null,  // only for today and future days, from the gym rotation projection
  "work": {"start":"10:00","end":"17:00"} | null,  // work hours (memory.work); null on non-work days, days off and public holidays. NOT a task: not tickable, not counted in progress
  "holiday": "Christmas Day" | null,               // public holiday for memory.work.holiday_region (Nager.Date; null if lookup fails)
  "day_off": false                                 // true when the date is in /api/days-off
}
Occurrence = {
  "task_id": 1, "date": "2026-11-02",
  "title": "...", "time": "07:30", "part_of_day": "morning",
  "category": Category | null, "notes": "",
  "recurring": true,
  "rolled_over_from": null,       // "2026-11-01" if this one-off was carried over
  "done": false,
  "checklist": [{"id":5,"text":"Make bed","done":true}]
}
```
`POST /api/occurrences/toggle` body `{"task_id":1,"date":"2026-11-02","done":true}` → Occurrence
`POST /api/occurrences/checklist` body `{"task_id":1,"date":"2026-11-02","item_id":5,"done":true}` → Occurrence

Rollover rule: when today is requested, any one-off task with `date < today` and not done is moved to today (its `date` becomes today; `rolled_over_from` keeps the original date).

## Gym
```json
Exercise = {"id":1,"name":"Bench Press"}
Template = {"id":1,"name":"Push A","exercises":[{"exercise_id":1,"name":"Bench Press","sets":3,"rep_min":6,"rep_max":10}]}
Rotation = {"items":[{"template_id":1}, {"template_id":2}, {"template_id":3}, {"rest":true}], "next_index": 0}
WorkoutPlan = {"rest": false, "template_id":1, "name":"Push A",
  "exercises":[{"exercise_id":1,"name":"Bench Press","sets":3,"rep_min":6,"rep_max":10,
                "last":{"date":"2026-10-30","sets":[{"weight":60,"reps":10}]} | null,
                "suggested_weight": 62.5, "hint": "You hit 10 reps on every set last time, try 62.5 kg"}]}
Session = {"id":1,"template_id":1,"name":"Push A","date":"2026-11-02","completed":false,
  "sets":[{"exercise_id":1,"set_no":1,"weight":60,"reps":10,"done":true}]}
```
`GET/POST /api/exercises`, `PUT/DELETE /api/exercises/{id}`
`GET/POST /api/templates`, `PUT/DELETE /api/templates/{id}`
`GET /api/rotation` → Rotation; `PUT /api/rotation` body `{"items":[...]}`
`GET /api/gym/next` → WorkoutPlan (the next item in the rotation)
`POST /api/gym/rest` → marks the current rest item as taken, advancing the rotation → Rotation
`POST /api/sessions` body `{"template_id":1}` → Session (today, pre-filled with suggested weights, sets not done). If an uncompleted session for today exists, return it.
`PUT /api/sessions/{id}/sets` body `{"sets":[...]}` → Session
`POST /api/sessions/{id}/complete` → Session; advances rotation past this template.
`GET /api/exercises/{id}/history` → `[{"date":"...","top_weight":62.5,"best_reps":8,"est_1rm":79.2}]`

Rotation projection for `/api/days`: today gets the next rotation item, each following day gets the following item (wrapping), unless a session was already completed today (then today shows that session, and projection continues from next).
Progression: if every set of an exercise last time was done with `reps >= rep_max` at the same weight W, suggest W + 2.5; otherwise suggest W. No history → null.

## Goals
```json
Goal = {"id":1,"title":"Pass my driving test","horizon":"medium"|"long","deadline":"2027-03-01"|null,
        "why":"Freedom to visit family without relying on lifts","status":"active"|"achieved"|"archived",
        "achieved_on":null|"2026-11-20","created":"2026-11-02"}
```
`GET /api/goals` (all, active first) -> `[Goal]`; `POST /api/goals` body Goal without id/created -> Goal; `PUT /api/goals/{id}` -> Goal; `DELETE /api/goals/{id}`.
Goals are context for the AI only; tasks are not linked manually. Active goals (and goals achieved in the relevant period) are included in the Gemini context for day notes, daily briefings and the weekly overview, so the assistant can connect what was done to what it is working towards. The AI must only claim a task relates to a goal when it plausibly does.

## Assistant memory
```json
Memory = {
  "weather": {"comfortable_max": 18, "hot_above": 22, "too_hot_above": 25, "likes_rain": true, "avoid_sunburn": true},
  "work": {"enabled": true, "days": [0,1,2,3,4], "start": "10:00", "end": "17:00", "holiday_region": "AU-NSW", "lunch_minutes": 60},
  "rhythm": {"wake_from": "06:00", "wake_to": "07:00", "sleep_from": "23:00", "sleep_to": "00:00"},
  "notes": [{"id": 1, "text": "I work from an office most weekdays"}]
}
```
`GET /api/memory` -> Memory; `PUT /api/memory/weather` body weather object (partial allowed; thresholds must satisfy comfortable_max <= hot_above <= too_hot_above, each -30..60) -> Memory; `PUT /api/memory/work` body partial work object (`days` are 0=Monday..6=Sunday; `start` < `end`, HH:MM; `lunch_minutes` 0..240; empty `holiday_region` = no holidays) -> Memory; `PUT /api/memory/rhythm` body partial rhythm object (HH:MM) -> Memory; `POST /api/memory/notes` `{"text"}` -> Memory; `PUT /api/memory/notes/{id}` `{"text"}` -> Memory; `DELETE /api/memory/notes/{id}` -> Memory (404 for unknown id; note text 1..500 chars).
Seed defaults are the values above with one example note (also returned for existing databases that have no stored memory). Memory is included in all Gemini context (digest, day note, briefings).

Work is a time block, not a task. Lunch is a flexible-time break of `lunch_minutes` (good for a quick call, bill, booking or shop). "Before work" means the window between waking and `work.start`; evening suggestions finish comfortably before `rhythm.sleep_from`.

Days off: `GET /api/days-off` -> `["2026-12-24", ...]`; `POST /api/days-off` `{"date"}` -> list; `DELETE /api/days-off/{date}` -> list.
Public holidays: fetched from Nager.Date (`https://date.nager.at/api/v3/PublicHolidays/{year}/AU`, no key), keeping entries where `global` is true or `counties` contains `holiday_region`; cached per (year, region) in the DB; any failure means no holidays (requests never fail). Env `PA_FAKE_HOLIDAYS="2026-12-25:Christmas Day,2026-12-28:Boxing Day (observed)"` overrides for testing.
All text (briefings, digest, day note) knows the work hours: on work days the day is framed as before work / during work / after work; holidays and days off are mentioned warmly ("no work today").

## Digest (home screen)
`GET /api/digest?date=2026-11-02` ->
```json
{"date":"2026-11-02",
 "weather": {"summary":"Light rain","temp_max":14,"temp_min":8,"precip_prob":80,"uv_max":1.5} | null,
 "weather_comment": "A proper rainy one, 14 at most. Cosy if you're home, and nice to listen to from your desk.",
 "overview": "...",
 "source": "gemini" | "template"}
```
- `weather_comment`: 1-2 sentences, interprets the forecast against the memory weather prefs (cool is comfortable, warn kindly when hot/too hot, suggest sun cream / shade when UV >= 3 and avoid_sunburn, enjoy rain if likes_rain), and relates it to the day's plans where natural (e.g. office day vs home day, outdoor tasks, gym). Null if no weather.
- `overview`: the assistant's digest and organisation of the day in prose, 2-4 short sentences, chatty and natural; groups the day into its shape (e.g. what the morning is about, the afternoon, the evening), names the key things, points out a breather, ties to goals where plausible. No raw counts/percentages. Never a bare list of every task. May mention the workout briefly.
- Gemini when a key is set, cached by (date, hash of items + goals + memory + weather); template fallback otherwise. The day-note endpoint remains for the briefing's `affirmation` field.
- Digest cache key also includes work/holiday/day-off info and the workout name.
- Weather now also fetched with `precipitation_probability_max` and `uv_index_max` from Open-Meteo. Env `PA_FAKE_WEATHER` (e.g. `Light rain,14,8,80,1.5`) overrides the lookup for testing.
 (home screen "affirmation")
`GET /api/day-note?date=2026-11-02` → `{"date":"2026-11-02","text":"...","source":"gemini"|"template"}`
A short (1–2 sentences, max ~40 words), grounded, specific note about *this* day — not generic or soppy. It references the actual shape of the day (how busy, when the busy stretch is, the one thing that matters most, a breather) and offers one practical, encouraging angle. Examples of the style: "Three things land before noon, then the afternoon opens up. Get the stand-up and Sam's email done early and the rest of the day is yours." / "A light day with just two tasks, both flexible. Good day to get the dentist booked and have a proper evening."
Gemini when a key is set; cached in the DB keyed by (date, hash of that day's items) so it is generated at most once per distinct day plan. Template fallback builds the same kind of note from the day's data. Never mentions the gym/workout.
The briefing's `affirmation` field returns the same day note text.

## Briefings
`GET /api/briefing?kind=morning|evening|weekly&date=2026-11-02[&ai=false]` → (`ai=false` skips Gemini; template text only)
```json
{"kind":"morning","date":"2026-11-02","text":"Good morning! ...","source":"gemini"|"template",
 "weather":{"summary":"Light rain","temp_max":14,"temp_min":8,"precip_prob":80,"uv_max":1.5} | null,
 "affirmation":"You don't have to do it all today, just the next thing.",
 "stats":{"total":6,"done":2,"next_up":Occurrence|null}}
```
Morning is the main daily summary (shown as text on Home and read aloud): about 2-3 minutes spoken, roughly 320-450 words, flowing spoken prose (no markdown or lists): (1) warm hello + the weather interpreted through the memory preferences (and a holiday/day-off/work-hours note); (2) a kind look back at yesterday (what got done by name, what rolled over); (3) today broken down by task category/theme (life admin, work, chores, errands, study, exercise/gym with goal tie-ins, social, appointments with times); (4) a suggested order (quick small tasks first for momentum, or the big one first while fresh, with why; size is inferred from title/notes/checklist; respects fixed-time items and the work block, uses the window between waking and work start, puts a short errand in the flexible lunch hour, keeps the evening ending before bedtime); (5) a peek at tomorrow's notable things; (6) a grounded close. Evening stays about 150-250 words and nudges toward winding down for sleep. Weekly is unchanged.
Briefing text from Gemini is cached in the DB by (kind, date, hash of the full prompt context incl. done states, goals, memory, work info and weather), so reloading Home does not call Gemini again; changing done states may regenerate.

Tone: warm best friend: chatty, natural and human, like a real personal assistant who knows you. Never read out raw counts, fractions or percentages ("0 of 24 tasks", "50 percent"); talk about specific things done or coming up instead, and tie them to goals where they genuinely relate. Kind and low-key about quiet days/weeks (no guilt, no stats). Morning: weather, the day in order, workout, an affirmation. Evening: what got done (kindly), what's left, a gentle look at tomorrow. Weekly: tasks done over the Mon–Sun week of `date`, gym sessions, progress highlights, a look at next week.
Gemini model via env `GEMINI_MODEL` (default `gemini-2.5-flash`); on any failure or no key, fall back to template text.

`POST /api/tts` body `{"text":"...","voice":"en-US-JennyNeural"}` → `audio/mpeg` bytes. Uses `edge-tts` CLI; falls back to `piper` if installed; else 503.
