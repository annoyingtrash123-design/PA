# Personal Assistant App — Spec

## Stack
- **Backend:** Rust (Axum) REST API, Google Firestore storage behind a storage interface (SQLite for local development).
- **Frontend:** Python (Flet), served as a web app; used on Android via Chrome "Add to home screen". APK later, optional.
- **Hosting:** Google Cloud Run (free tier, scales to zero). Backend and frontend each run as a container.
- **AI:** Google Gemini API free tier, behind a swappable provider interface. Template-based fallback if the AI fails or there is no key.
- **Voice:** Gemini text-to-speech (same API key, American English voice), with edge-tts as a free fallback.
- **Backup (optional):** periodic JSON export to the user's Google Drive.
- **Weather:** Open-Meteo (free, no key). Location set in settings.
- **Units:** kg.
- **No notifications/reminders:** the app is opened manually; briefings play on demand.

## Assistant
- Tone: warm best friend (chatty, upbeat, casual).
- Name: user-configurable in settings (default name provided).

## Tabs

### Home
- Overview of today: timed tasks in order, untimed tasks grouped by part of day, today's workout.
- "Brief me" button: AI-written, chatty spoken briefing.
- Evening wind-down briefing: what got done, gentle preview of tomorrow.
- Weather included in the briefing.
- Daily affirmation/quote.
- Progress ring (tasks done today) and a "next up" card.

### Tasks
- Week page: Monday to Sunday, top to bottom, each day with its date; navigate to future (and past) weeks.
- A task can have an exact time or none. Untimed tasks can optionally have a part of the day (morning/afternoon/evening/anytime).
- Recurring tasks (daily, weekdays, specific weekdays, etc.).
- Unfinished tasks roll over to the next day.
- Categories with colours.
- Routine checklists (a task with sub-steps, e.g. "Morning routine").

### Gym
- Workout templates with variations (e.g. Push A, Push B, Pull A, Legs A).
- Rolling rotation (e.g. Push → Pull → Legs → Rest): the next workout follows the last one completed. It appears automatically in the day and the briefing.
- Workout mode: tick off sets and log weight/reps, with last session's numbers shown beside each exercise.
- Weight history per exercise over time.
- Progression hints (e.g. all reps hit → suggest +2.5 kg).

### Weekly review
- Sunday summary of the week (tasks completed, gym sessions, progress), readable and playable as audio.
