# Personal Assistant App — Spec

## Stack
- **Runs entirely on the phone:** installable web app (PWA) on GitHub Pages; no server.
- **Logic:** Rust core compiled to WebAssembly. **UI:** TypeScript + Vite.
- **Data:** stored in the browser (IndexedDB); backup/restore file in Settings.
- **AI:** Google Gemini API free tier, called directly from the phone with the user's key (stored only on the device). A plain built-in summary is used when there's no key or the AI is unavailable. Daily budget of 12 text requests (Settings has "Allow 3 more today"); long replies are streamed; a busy Google (503) or a timeout is retried, then the lite model is tried.
- **Voice:** natural Gemini voices (female voices only, default Sulafat, with favourites and a speed slider in Settings). Each summary is read as three clips played back to back. The phone's built-in speech (Web Speech API) is only the fallback.
- **Weather:** Open-Meteo (free, no key). **Public holidays:** Nager.Date (AU-NSW).
- **Units:** kg. **Timezone:** Australia/Sydney (setting).
- **No notifications/reminders:** the app is opened manually.
- **Single user:** no accounts.

## Assistant
- Tone: warm best friend: chatty and natural, like a real assistant. No raw stats or percentages; specific highlights tied to goals.
- Name: user-configurable in settings (default name provided).
- Spoken style: a shared style guide (relaxed, natural speech with light fillers) and a small built-in knowledge base (little memories, facts, tips and thoughts picked to fit the day) shape everything the AI writes. Recent summaries are passed to the writer so it avoids repeating itself.
- A private profile code (pasted in Settings) can fill in interests, notes, work hours, weather preferences, goals, leisure and tasks without putting personal details in the public repo.

## Tabs

### Home (the assistant's digest, no task list)
- Greeting + date.
- Weather with the assistant's comment, interpreted through your saved preferences (e.g. likes rain, cool is comfortable, dislikes heat, avoid sunburn).
- The assistant's digest of the day: its organisation and interpretation of the day's tasks in prose, tied to goals.
- Progress + next up.
- Morning / Wind-down summary, with "Read it to me". Each is written by up to three small independent AI requests (morning: a short "opening" with greeting, weather comment and affirmation; the day; the news). Each can fail on its own and falls back to the built-in part. There is no editor pass.
- A label at the top of the summary card says "Written by Miranda's AI" or "Built-in summary" with the reason, plus a "Try again" button after a Google hiccup. Failed summaries are retried automatically only a couple of times a day.
- Built-in summary (no AI): deliberately short and plain. Morning: today, the order, tomorrow, closing. Evening: look back, tomorrow, goodnight.
- News: two stories, morning and evening, each told as a short full story. Fetched by a GitHub Action at about 05:45 and 16:45 Sydney time all year round.
- Evening check-in: during the wind-down, asks once about tomorrow's unplaced tasks (Before work / Afternoon / After work). It uses no AI and never blocks a summary.
- The actual task list lives only on the Tasks tab.

### Assistant memory (in Settings)
- Weather preferences: comfortable max (18), hot above (22), too hot above (25), likes rain, avoid sunburn.
- Free-text notes the assistant should remember about you; used as context for everything it writes.

### Tasks
- Week page: Monday to Sunday, top to bottom, each day with its date; navigate to future (and past) weeks.
- A task can have an exact time or none. Untimed tasks can optionally have a part of the day (morning/afternoon/evening/anytime).
- Recurring tasks (daily, weekdays, specific weekdays, etc.).
- Rollover: anything without a clock time carries over to the next day automatically (including untimed appointments). Only tasks with a clock time go to "Missed", where you choose.
- Press and hold the arrow on a one-off task to move it to the next day; the day menu has "Move unfinished to tomorrow".
- Task brain: one table decides a task's category, typical duration, whether it means going out, and how Miranda says it ("go to the doctor" becomes "a doctor's appointment"). A task can have a "How long does it take?" figure; it is remembered for that task title and beats the guess. The task dialog shows "Miranda reads this as: ...".
- Untimed Work tasks sit inside the work block and are spoken as part of the workday.
- Categories with colours.
- Routine checklists (a task with sub-steps, e.g. "Morning routine").

### Gym
- Workout templates with variations (e.g. Push A, Push B, Pull A, Legs A).
- Rolling rotation, default: Push 1 → Pull 1 → Cardio → Rest → Legs + Abs 1 → Push 2 → Pull 2 → Cardio → Rest → Legs + Abs 2 (legs always come off a rest day; cardio never on sore legs). The next workout follows the last one completed and appears in the day and the briefing.
- Workout mode: tick off sets and log weight/reps, with last session's numbers shown beside each exercise.
- Weight history per exercise over time.
- Progression hints (e.g. all reps hit → suggest +2.5 kg).

### Plans (goals and leisure)
- Medium-term and long-term goals, each with an optional deadline and an optional "why".
- Mark achieved / archive.
- Used by the AI as context to connect daily tasks and the week's activity to what you are working towards. No manual task-to-goal linking.
- Leisure list: things to watch, read, listen to, play or try. On free days Miranda may suggest one, softly, in the morning or evening summary.

### Weekly review
- Titled "Overview of the week". Sunday summary of the week (tasks completed, gym sessions, progress), readable and playable as audio.

## Tabs in the app
Home, Tasks, Plans (goals + leisure), Gym, Settings.

## Safety and testing
- A Content-Security-Policy is added to the built page, so the app can only talk to Google (Gemini), Open-Meteo and Nager.Date.
- The app asks the browser to keep its storage (`navigator.storage.persist`) so data isn't cleared.
- Whole-app phone-screen tests (Playwright, `app/e2e/run.cjs`) run against the built app with stand-ins for Google, weather and news.
