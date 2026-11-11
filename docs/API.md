# Core API contract

There is no server. These are the routes of the Rust core (`core/src/router.rs`), reached through the wasm `call(state, method, path, body, ctx)` (see `docs/WEB_ARCH.md`). All bodies JSON. Dates `YYYY-MM-DD`, times `HH:MM` (24h). IDs are integers.
Errors: non-2xx with `{"error": "message"}`.

## Settings
`GET /api/settings` → `Settings`
`PUT /api/settings` body: partial `Settings` → `Settings`
```json
Settings = {
  "assistant_name": "Miranda",
  "user_name": "",                // what the assistant calls you
  "voice": "",                    // phone-voice name for the fallback speech; the natural voice, speed and favourites live in the UI (localStorage `pa-voice-prefs`)
  "location_name": "Sydney",
  "latitude": -33.8688, "longitude": 151.2093,
  "timezone": "Australia/Sydney", // IANA name; the UI uses it to compute "today"/"now"; PUT validates it
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
  "minutes": 30,                  // optional: how long it takes, set by the user; null = Miranda guesses. Remembered per title (State.learned_minutes) and beats the guess
  "checklist": [{"id": 5, "text": "Make bed"}]
}
```
`GET /api/tasks/guess?title=Go%20to%20the%20doctor` → `{"says":"a doctor's appointment","category":"Appointments","takes":"...","learned":null|30}` (what Miranda reads from a title; all null for an empty title; `learned` is the remembered figure for that exact title)
`PUT /api/tasks/{id}/part` body `{"part_of_day":"morning"|"afternoon"|"evening"|"anytime"}` → Task (just the part of the day; used by the evening check-in)
`POST /api/tasks` body: Task without id (checklist items without id) → `Task`
`PUT /api/tasks/{id}` body: full Task (checklist replaced) → `Task`. Sending `"minutes": null` also forgets the remembered figure for that title.
`DELETE /api/tasks/{id}`

### Auto-categorisation
`POST /api/tasks` and `PUT /api/tasks/{id}` accept `"category_id": "auto"`. The core then picks one category from the existing list (never creates new ones) using the task brain (`taskbrain.rs`) and then the keyword table, so it works offline; no match → "Other". Gemini is not used for this. The returned Task has the chosen numeric `category_id` and `"category_auto": true`. Tasks store `category_auto` so the UI can show "Auto" as the selection on edit; choosing a category manually sets it false.
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
  "minutes": 30,                  // as on Task (null when not set)
  "rolled_over_from": null,       // "2026-11-01" if this one-off was carried over
  "done": false,
  "checklist": [{"id":5,"text":"Make bed","done":true}]
}
```
`POST /api/occurrences/toggle` body `{"task_id":1,"date":"2026-11-02","done":true}` → Occurrence
`POST /api/occurrences/checklist` body `{"task_id":1,"date":"2026-11-02","item_id":5,"done":true}` → Occurrence

`POST /api/occurrences/move` body `{"task_id":1,"date":"2026-11-02"}` → Occurrence on the next day: moves an unfinished one-off task to the day after its date (400 for recurring or done tasks)
`POST /api/days/{date}/move-unfinished` → `{"moved":3,"to":"2026-11-03"}`: moves every unfinished one-off task on that day to the next day

Rollover rule: when today is requested, any one-off task with `date < today` and not done is handled by kind:
- no clock time (only a part of the day, or anytime), including untimed appointments: moved to today automatically (its `date` becomes today; `rolled_over_from` keeps the original date).
- with a clock time (`time` set): NOT moved. Its `date` stays and it is recorded in `State.missed` (`{"<task_id>": "<original date>"}`).
Untimed Work tasks show inside the work band and are spoken as part of the workday.
- recurring tasks are unaffected.

Missed items:
`GET /api/missed` -> `[Occurrence]` (clock-time tasks only) on each item's original date (oldest first; done items and deleted tasks are not listed). Also runs the rollover.
`POST /api/missed/{id}/today` -> Occurrence: moves the task to today keeping its time; clears the missed mark (404 if not missed).
`POST /api/missed/{id}/dismiss` -> `{"ok":true}`: marks it done on its original date and clears the mark (404 if not missed).
`PUT /api/tasks/{id}` clears the mark when the date becomes today or later (or the task becomes recurring); `DELETE /api/tasks/{id}` clears it too.
The morning briefing mentions missed items in one gentle sentence (template only when there are some; AI context carries the same guidance, no counts).

## Gym
```json
Exercise = {"id":1,"name":"Counterweighted dip","assisted":true}   // assisted: a HIGHER weight is EASIER (counterweight machines)
Template = {"id":1,"name":"Push 1","kind":"strength","note":"Chest + Triceps + Front Deltoid",
  "exercises":[{"exercise_id":1,"name":"Incline dumbbell chest press","sets":4,"rep_min":10,"rep_max":10,
                "start_weight":15,"note":""}],            // start_weight: kg or null (bodyweight / unknown); note: shown under the name
  "options":[], "minutes_min":null, "minutes_max":null}
CardioTemplate = {"id":7,"name":"Cardio","kind":"cardio","note":"Do one of the following for 20\u201340 mins","exercises":[],
  "options":["Swimming","Rowing machine","Stair machine","Intense walking/hiking"],"minutes_min":20,"minutes_max":40}
Rotation = {"items":[{"template_id":1}, {"template_id":2}, {"template_id":3}, {"rest":true}], "next_index": 0}
WorkoutPlan = {"rest": false, "template_id":1, "name":"Push 1", "kind":"strength"|"cardio", "note":"...",
  "exercises":[{"exercise_id":1,"name":"Incline dumbbell chest press","sets":4,"rep_min":10,"rep_max":10,
                "last":{"date":"2026-10-30","sets":[{"weight":15,"reps":10}]} | null,
                "suggested_weight": 17.5, "hint": "You hit 10 reps on every set last time, try 17.5 kg",
                "assisted": false, "start_weight": 15, "note": ""}],
  "options":[], "minutes_min":null, "minutes_max":null}      // options/minutes_*: cardio only
Session = {"id":1,"template_id":1,"name":"Push 1","date":"2026-11-02","completed":false,
  "sets":[{"exercise_id":1,"set_no":1,"weight":15,"reps":10,"done":true}],
  "cardio": null | {"activity":"Swimming","minutes":30}}      // cardio sessions have no sets
```
`GET/POST /api/exercises` (body `{name, assisted?}`), `PUT/DELETE /api/exercises/{id}` (`assisted` optional, unchanged when omitted)
`GET/POST /api/templates`, `PUT/DELETE /api/templates/{id}`: body `{name, kind?:"strength"|"cardio", note?, exercises:[{exercise_id | name, sets, rep_min, rep_max, start_weight?, note?, assisted?}], options?, minutes_min?, minutes_max?}`.
A template exercise's `assisted`, when given, sets the flag on the (shared) exercise. Cardio templates need at least one option (names are de-duplicated; minutes default 20-40) and ignore `exercises`.
Exercises are shared by name across templates (the same `exercise_id`), so history carries over: "Seated cable machine row" in Pull 1 and Pull 2 is ONE exercise.
`GET /api/rotation` -> Rotation; `PUT /api/rotation` body `{"items":[...]}`
`GET /api/gym/next` -> WorkoutPlan (the next item in the rotation)
`POST /api/gym/rest` -> marks the current rest item as taken, advancing the rotation -> Rotation
`POST /api/gym/skip-rest` -> "Train today instead": advances past the next item when it is a rest, WITHOUT recording a rest taken today, so today's plan (`/api/gym/next`, `/api/days`) becomes the next workout -> Rotation (400 if the next item is not a rest)
`POST /api/gym/skip` -> escape hatch: advances past the next item (workout or rest) without logging anything -> Rotation. Missed workouts are never skipped automatically.
Missed days: `state.gym.next_since` is the date the current next item became next (set whenever `next_index` changes; set to today by the v5 migration, or on the first call for a state without it). On every call, if the next item is a rest and `next_since` < today, the rest counts as taken: the rotation advances past it (one rest item per elapsed day), `rest_taken_date` = today so today's projection shows the rest, `next_since` = today, and the new state is returned. Workouts always wait.
`POST /api/sessions` body `{"template_id":1}` -> Session (today, pre-filled with suggested weights, sets not done; the start weight when the exercise has no history). If an uncompleted session for today exists, return it. For a cardio template: no sets, `cardio` = `{"activity":"","minutes":30}` (30 clamped into the template's range).
`PUT /api/sessions/{id}/sets` body `{"sets":[...]}` -> Session
`PUT /api/sessions/{id}/cardio` body `{"activity":"Swimming","minutes":35}` -> Session (cardio sessions only; minutes 1..600)
`POST /api/sessions/{id}/complete` -> Session; advances rotation past this template. A cardio session needs an activity first (400 otherwise).
`GET /api/exercises/{id}/history` -> `[{"date":"...","top_weight":62.5,"best_reps":8,"est_1rm":79.2}]`. For assisted exercises `top_weight` is the LOWEST counterweight of the session (the best one) and `est_1rm` is null.

Rotation projection for `/api/days`: today gets the next rotation item, each following day gets the following item (wrapping), unless a session was already completed today (then today shows that session, and projection continues from next). The default rotation is Push 1, Pull 1, Legs + Abs 1, Cardio, Push 2, Pull 2, Legs + Abs 2, Cardio.
Progression: if every set of an exercise last time was done with `reps >= rep_max` at the same weight W, suggest W + 2.5; otherwise suggest W. No history -> the template exercise's `start_weight` with hint `"Starting weight: 15 kg"`, or null when it has none.
Assisted exercises go the other way: every set hit -> suggest W - 2.5 (not below 0) with hint "You hit every rep last time, try 2.5 kg less counterweight"; otherwise stay at W. Weekly progress (PRs) counts a lower counterweight as better.
Briefings and the weekly overview mention cardio sessions naturally ("a swim", "40 minutes on the rowing machine").

## Goals
```json
Goal = {"id":1,"title":"Pass my driving test","horizon":"medium"|"long","deadline":"2027-03-01"|null,
        "why":"Freedom to visit family without relying on lifts","status":"active"|"achieved"|"archived",
        "achieved_on":null|"2026-11-20","created":"2026-11-02"}
```
`GET /api/goals` (all, active first) -> `[Goal]`; `POST /api/goals` body Goal without id/created -> Goal; `PUT /api/goals/{id}` -> Goal; `DELETE /api/goals/{id}`.
Goals are context for the AI only; tasks are not linked manually. Active goals (and goals achieved in the relevant period) are included in the Gemini context for day notes, daily briefings and the weekly overview, so the assistant can connect what was done to what it is working towards. The AI must only claim a task relates to a goal when it plausibly does.

## Leisure
```json
Leisure = {"id":1,"kind":"watch"|"read"|"listen"|"play"|"try","title":"Piranesi","note":"On chapter twelve",
           "status":"someday"|"current"|"finished","created":"2026-11-02","finished_on":null|"2026-11-20",
           "last_suggested":null|"2026-11-07","blurb":""}
```
`GET /api/leisure` (current, someday, then finished) -> `[Leisure]`; `POST /api/leisure` body `{kind,title,note?,status?,blurb?}` -> Leisure;
`PUT /api/leisure/{id}` (a missing `blurb` keeps the stored one; `last_suggested` is never taken from the body; `finished_on` is set to today when the status becomes finished); `DELETE /api/leisure/{id}`.
Validation: kind in the five values, title required (max 120), note max 300, blurb max 700, status one of the three.
`GET /api/leisure/{id}/blurb-prompt` -> `{id,title,system_prompt,user_prompt}` for ONE text request: 2-3 spoken sentences, spoiler-free (what it is, then why this person might enjoy it, using their interests silently); the model must reply exactly `UNKNOWN` when it does not know the work, and the UI then stores the blurb "I don't know much about this one yet." (blurbs are written only when an item is added or the person taps Refresh).

When Miranda may suggest something (morning and evening briefings only, never the weekly one): only with genuinely free room - an empty day, a day off / holiday, a weekend day with few items (at most one timed), or for the evening nothing timed from 18:00 and at most two items left. Then ONE item: a `current` one (phrased as carrying on: "another episode of X", "a few more chapters of Y"), else a `someday` one not suggested in the last 5 days; interests that share words with the title/note/blurb raise priority, ties rotate by date; rainy, cold or wet days avoid "try" items and evenings never suggest them. `last_suggested` is set (state returned) when the briefing for TODAY is prepared; repeated calls the same day keep the same pick. The template adds one gentle optional sentence (plus the first sentence of the blurb for something new) and the AI context lists the leisure items (current first, finished never), the computed suggestion and whether there is room, with the rule: at most one, only with free time, softly, never as a goal. Finished items appear only as a light highlight in the weekly overview.

## Assistant memory
```json
Memory = {
  "weather": {"comfortable_max": 18, "hot_above": 22, "too_hot_above": 25, "likes_rain": true, "avoid_sunburn": true},
  "work": {"enabled": true, "days": [0,1,2,3,4], "start": "10:00", "end": "17:00", "holiday_region": "AU-NSW", "lunch_minutes": 60},
  "rhythm": {"wake_from": "06:00", "wake_to": "07:00", "sleep_from": "23:00", "sleep_to": "00:00"},
  "notes": [{"id": 1, "text": "I work from an office most weekdays"}],
  "interests": "History and politics, ..."   // free text; default set by init_state and the v8 migration
}
```
`GET /api/memory` -> Memory; `PUT /api/memory/weather` body weather object (partial allowed; thresholds must satisfy comfortable_max <= hot_above <= too_hot_above, each -30..60) -> Memory; `PUT /api/memory/work` body partial work object (`days` are 0=Monday..6=Sunday; `start` < `end`, HH:MM; `lunch_minutes` 0..240; empty `holiday_region` = no holidays; optional `weekday_hours` `{"1":{"start":"10:00","end":"20:00"}}` (keys are weekday indexes as strings, "0"=Mon.."6"=Sun; only used on work days) and `monthly_rules` `[{"nth":1,"weekday":2,"start":"10:00","end":"18:00"}]` (nth 1-5 or -1=last occurrence of that weekday in the month); precedence: holiday/day off = no work, else first matching monthly rule > weekday_hours > default start/end; all validated HH:MM with start < end; both replace the whole value when sent; also accepted in profile codes' `work`) -> Memory; `PUT /api/memory/interests` body `{"text"}` (free text, max 1000 chars; silent AI context and it steers which leisure item is suggested; empty is allowed). `PUT /api/memory/rhythm` body partial rhythm object (HH:MM) -> Memory; `POST /api/memory/notes` `{"text"}` -> Memory; `PUT /api/memory/notes/{id}` `{"text"}` -> Memory; `DELETE /api/memory/notes/{id}` -> Memory (404 for unknown id; note text 1..500 chars).
Seed defaults are the values above with one example note (also returned for existing databases that have no stored memory). Memory is included in all Gemini context (digest, day note, briefings).

Work is a time block, not a task. Lunch is a flexible-time break of `lunch_minutes` (good for a quick call, bill, booking or shop). "Before work" means the window between waking and `work.start`; evening suggestions finish comfortably before `rhythm.sleep_from`.

Days off: `GET /api/days-off` -> `["2026-12-24", ...]`; `POST /api/days-off` `{"date"}` -> list; `DELETE /api/days-off/{date}` -> list.
Public holidays: fetched from Nager.Date (`https://date.nager.at/api/v3/PublicHolidays/{year}/AU`, no key), keeping entries where `global` is true or `counties` contains `holiday_region`; fetched by the UI and passed in `ctx.holidays` (cached per year and region); any failure means no holidays.
All text (briefings, digest, day note) knows the work hours: on work days the day is framed as before work / during work / after work; holidays and days off are mentioned warmly ("no work today").

## Digest (home screen)
`GET /api/digest?date=2026-11-02` ->
```json
{"date":"2026-11-02",
 "weather": {"summary":"Light rain","temp_max":14,"temp_min":8,"precip_prob":80,"uv_max":1.5} | null,
 "weather_comment": "A proper rainy one, 14 at most. Cosy if you're home, and nice to listen to from your desk.",
 "overview": "...",
 "cache_key": "digest:2026-11-02:<hash>", "system_prompt": "...", "user_prompt": "...",   // for the UI's Gemini request (JSON reply {weather_comment, overview})
 "template": {"weather_comment": "...", "overview": "..."},
 "source": "template"}      // weather_comment / overview at the top level are the built-in text
```
- `weather_comment`: 1-2 sentences, interprets the forecast against the memory weather prefs (cool is comfortable, warn kindly when hot/too hot, suggest sun cream / shade when UV >= 3 and avoid_sunburn, enjoy rain if likes_rain), and relates it to the day's plans where natural (e.g. office day vs home day, outdoor tasks, gym). Null if no weather.
- `overview`: the assistant's digest and organisation of the day in prose, 2-4 short sentences, chatty and natural; groups the day into its shape (e.g. what the morning is about, the afternoon, the evening), names the key things, points out a breather, ties to goals where plausible. No raw counts/percentages. Never a bare list of every task. May mention the workout briefly.
- The UI asks Gemini when a key is saved, cached by (date, hash of items + goals + memory + weather); built-in text otherwise. The day-note endpoint remains for the briefing's `affirmation` field.
- Digest cache key also includes work/holiday/day-off info and the workout name.
- Weather comes from Open-Meteo (including `precipitation_probability_max` and `uv_index_max`), fetched by the UI and passed in `ctx.weather`.
 (home screen "affirmation")
`GET /api/day-note?date=2026-11-02` → `{"date":"2026-11-02","cache_key","system_prompt","user_prompt","template_text","skip_ai","max_words"}` (prepare response; `skip_ai` is true for an empty or finished day)
A short (1–2 sentences, max ~40 words), grounded, specific note about *this* day — not generic or soppy. It references the actual shape of the day (how busy, when the busy stretch is, the one thing that matters most, a breather) and offers one practical, encouraging angle. Examples of the style: "Three things land before noon, then the afternoon opens up. Get the stand-up and Sam's email done early and the rest of the day is yours." / "A light day with just two tasks, both flexible. Good day to get the dentist booked and have a proper evening."
The UI caches the AI reply by `cache_key` (date, hash of that day's items) so it is generated at most once per distinct day plan. Built-in fallback builds the same kind of note from the day's data. Never mentions the gym/workout.
The briefing's `affirmation` field returns the same day note text.

Profile codes (private data kept out of the repo): `POST /api/profile/apply` body `{v, updated, interests?, notes?, work?, weather?, add_goals?, add_leisure?, add_tasks?}` (all-or-nothing); `POST /api/profile/clear` resets interests, notes, work hours, weather preferences and the profile date.

## Briefings
The core never calls Gemini. Each AI endpoint returns a "prepare" response: the built-in text, a cache key and the prompts for the UI to send.

`GET /api/briefing?kind=morning|evening|weekly&date=2026-11-02` →
```json
{"kind":"morning","date":"2026-11-02",
 "weather":{"summary":"Light rain","temp_max":14,"temp_min":8,"precip_prob":80,"uv_max":1.5} | null,
 "stats":{"total":6,"done":2,"next_up":Occurrence|null},
 "affirmation":"...",            // the built-in day note
 "template_text":"...",          // the whole built-in summary
 "template_body":{...}, "template_parts":{...},   // the built-in summary in parts, so AI and built-in parts can be mixed
 "news": {...} | null,           // the stories picked, their sources, position, template_text and the news request's own system_prompt/user_prompt
 "cache_key":"briefing:morning:2026-11-02:<hash>",
 "plan_fingerprint":"...",       // changes when the plan changes (titles, times, parts of day, workout; evening also done states), never for weather
 "system_prompt":"...", "user_prompt":"...",       // the plain-text "day" request
 "opening": {"system_prompt":"...","user_prompt":"..."} | null,   // mornings only: the small JSON request for greeting, weather_comment and affirmation
 "max_words": 450}
```
A summary is written by up to three small independent requests: `opening` (mornings; JSON `{greeting, weather_comment, affirmation}`), the day (`system_prompt`/`user_prompt`, plain text) and the news (JSON). Each can fail on its own and the UI uses the matching built-in part instead. There is no review or editor pass (`review_prompt` no longer exists). The UI appends the recent summaries to the prompts so the writer avoids repeating them.

Morning is the main daily summary: the opening, then four short paragraphs (look back and today by theme, the suggested order, tomorrow, a closing line ending in a well-wish), about 2-3 minutes spoken, flowing prose with no markdown or lists. It is read aloud as three sections: greeting/weather/affirmation, the day, then tomorrow/news/closing. Evening is three paragraphs (look back, tomorrow, a closing that winds down); the news sits before the closing. Weekly looks back over the Mon-Sun week of `date` and ahead to next week. Mornings and evenings may include two news stories, each told as a short full story.

The built-in summary (`template_text`) is deliberately short and plain. Morning: today, the order, tomorrow, closing. Evening: look back, tomorrow, goodnight. It is not an imitation of the AI voice.

Tone for all AI text: warm best friend, chatty and natural, written for speaking (shared STYLE_GUIDE in `core/src/lib.rs`, plus a few picks from the knowledge base in `core/src/knowledge.rs`). Never read out raw counts, fractions or percentages (`has_stats` guards the plan text; quoted headlines are exempt); talk about specific things done or coming up and tie them to goals where they genuinely relate. Kind and low-key about quiet days.

News endpoints: `POST /api/news/select` body `{kind, date?, items?, generated_at?, morning_generated_at?, busy?, refresh?}` → the same `news` object as in the briefing (null when there is nothing to say); `POST /api/news/record` body `{kind, date, items, generated_at?}` → `{"ok":true}` (stories used are not picked again for about 48 hours).

Other helpers: `POST /api/holidays/filter` body `{"region":"AU-NSW","raw":[...]}` → `[{"date","name"}]`; `GET /api/weather-summary?code=61` → `{"summary","icon"}`; `GET /api/state/migrate` → `{"version":8}`; `POST /api/sample-data`; `GET /api/export`; `POST /api/import`.
