# Personal Assistant App — Spec

## Stack
- **Runs entirely on the phone:** installable web app (PWA) on GitHub Pages; no server.
- **Logic:** Rust core compiled to WebAssembly. **UI:** TypeScript + Vite.
- **Data:** stored in the browser (IndexedDB); backup/restore file in Settings.
- **AI:** Google Gemini API free tier, called directly from the phone with the user's key (stored only on the device). Template fallback when there's no key or the AI is unavailable.
- **Voice:** the phone's built-in text-to-speech (Web Speech API), voice selectable in Settings.
- **Weather:** Open-Meteo (free, no key). **Public holidays:** Nager.Date (AU-NSW).
- **Units:** kg. **Timezone:** Australia/Sydney (setting).
- **No notifications/reminders:** the app is opened manually.
- **Single user:** no accounts.

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
