# Personal Assistant

A personal assistant app: a daily briefing read aloud, a weekly task planner, and a gym tracker.

- `backend/`: Rust (Axum + SQLite) API. See `backend/README.md`.
- `frontend/`: Python (Flet) web app. See `frontend/README.md`.
- `SPEC.md`: what the app does. `docs/API.md`: the backend API.

## Run locally

```
# terminal 1
cd backend && cargo run --release            # http://localhost:8080 (seeds demo data on first run)

# terminal 2
cd frontend && pip install -r requirements.txt && pip install edge-tts
BACKEND_URL=http://localhost:8080 python main.py   # open http://localhost:8550
```

Optional: set your Gemini API key in the app's Settings (or `GEMINI_API_KEY` for the backend) for AI-written briefings; otherwise friendly template briefings are used.
