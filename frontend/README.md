# Personal assistant frontend (Flet)

Mobile-first Flet 0.28.3 web app (Home, Tasks, Gym, Review, Settings). All backend calls
happen server-side in Python via `httpx`, so there are no CORS concerns.

## Run

```bash
cd frontend
pip install -r requirements.txt
BACKEND_URL=http://localhost:8080 PORT=8550 python main.py   # opens on http://localhost:8550
```

- `BACKEND_URL` (default `http://localhost:8080`), `PORT` (default `8550`).
- Open it in Chrome on the phone and use "Add to home screen".
- `requirements.txt` pins `flet==0.28.3`, `flet-web==0.28.3` and an older FastAPI, because flet-web 0.28.3
  crashes on startup with current FastAPI releases (`openapi_prefix` was removed).

## Develop against the mock backend

```bash
python dev/mock_backend.py 8080        # stdlib only, in-memory, follows docs/API.md
MOCK_TTS=off python dev/mock_backend.py   # make /api/tts return 503 to test the "Voice unavailable" path
```

`dev/screenshots/` holds 390x844 screenshots of each tab taken against the mock.
Note: Flet web renders on a canvas and loads CanvasKit and fonts from gstatic.com at runtime; in a sandbox
without internet, serve those locally (e.g. Playwright route interception) when taking screenshots.

## Layout

- `main.py` app shell: AppBar (settings gear), bottom NavigationBar, shared `Ctx`.
- `api.py` httpx client; `ApiError` for everything that goes wrong.
- `theme.py` warm Material 3 theme, palette, constants.
- `views/` `home.py`, `tasks.py`, `gym.py`, `review.py`, `settings.py`.
- `widgets/` `common.py` (cards, occurrence tile, error card), `player.py` (TTS + `ft.Audio`),
  `task_dialog.py` (add/edit task), `base.py` (view base class with loading/retry handling).

## Contract notes / assumptions

- Editing a task calls `GET /api/tasks/{id}` (not in API.md) to load recurrence and date. If it 404s the dialog
  falls back to the occurrence data and warns that repeat settings could not be loaded.
- The weekly review shows a "Gym sessions" tile only if `stats.gym_sessions` is present in the weekly briefing.
- Home loads `/api/briefing?kind=morning` on open just to get weather + affirmation.
