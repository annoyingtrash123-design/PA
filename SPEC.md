# Personal Assistant App — Spec

## Stack
- **Backend:** Rust (Axum) REST API, SQLite storage.
- **Frontend:** Python (Flet), served as a web app; used on Android via Chrome "Add to home screen". APK later, optional.
- **Hosting:** free/cheap cloud host, backend + frontend in one Docker deployment.
- **AI:** Google Gemini API free tier, behind a swappable provider interface. Template-based fallback if the AI fails or there is no key.
- **Voice:** edge-tts (free Microsoft neural voices, American English, e.g. "Jenny"/"Aria"), with Piper as an offline fallback.
- **Weather:** Open-Meteo (free, no key). Location set in settings.
- **Units:** kg.
- **No notifications/reminders:** the app is opened manually; briefings play on demand.
- **Single user:** no accounts. A single passcode (env `APP_PASSCODE`) protects the hosted app; the phone stays signed in after entering it once.

## Assistant
- Tone: warm best friend: chatty and natural, like a real assistant. No raw stats or percentages; specific highlights tied to goals.
- Name: user-configurable in settings (default name provided).

## Tabs

### Home (the assistant's digest, no task list)
- Greeting + date.
- Weather with the assistant's comment, interpreted through your saved preferences (e.g. likes rain, cool is comfortable, dislikes heat, avoid sunburn).
- The assistant's digest of the day: its organisation and interpretation of the day's tasks in prose, tied to goals.
- Progress + next up.
- "Brief me" / "Wind-down" button: AI-written, chatty spoken briefing.
- The actual task list lives only on the Tasks tab.

### Assistant memory (in Settings)
- Weather preferences: comfortable max (18), hot above (22), too hot above (25), likes rain, avoid sunburn.
- Free-text notes the assistant should remember about you; used as context for everything it writes.

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

### Goals
- Medium-term and long-term goals, each with an optional deadline and an optional "why".
- Mark achieved / archive.
- Used by the AI as context to connect daily tasks and the week's activity to what you are working towards. No manual task-to-goal linking.

### Weekly review
- Titled "Overview of the week". Sunday summary of the week (tasks completed, gym sessions, progress), readable and playable as audio.
