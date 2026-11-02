# pa-backend

Rust (axum + SQLite) backend for the personal assistant app. The API contract is `../docs/API.md`.

## Run

```sh
cd backend
cargo run --release            # listens on 0.0.0.0:8080, creates ./pa.db
PORT=9000 DB_PATH=/data/pa.db GEMINI_API_KEY=... cargo run --release
```

On the first run with an empty database it is seeded with demo data (categories, recurring and one-off tasks,
gym templates, a rotation and a few past sessions).

## Environment

| Variable | Default | Purpose |
| --- | --- | --- |
| `PORT` | `8080` | HTTP port |
| `DB_PATH` | `./pa.db` | SQLite file |
| `GEMINI_API_KEY` | none | Gemini key; a key saved via `PUT /api/settings` (`gemini_api_key`) wins. Never returned by the API. |
| `GEMINI_MODEL` | `gemini-2.5-flash` | Model for briefings. Any failure or no key falls back to template text. |
| `PIPER_MODEL` | none | Path to a Piper `.onnx` voice, enables the offline TTS fallback (`piper` must be on PATH) |
| `PA_CA_BUNDLE` | none | Optional extra CA bundle (PEM) for outbound HTTPS (`SSL_CERT_FILE` is also honoured) |

`HTTPS_PROXY` and friends are honoured for outbound requests (weather, Gemini).

## Text to speech

`POST /api/tts` shells out to `edge-tts` (or `python3 -m edge_tts`), so install it with `pip install edge-tts`.
If that fails and `piper` plus `PIPER_MODEL` are available it returns WAV; otherwise 503 with `{"error": ...}`.
Audio is cached in `$TMPDIR/pa-tts` by hash of voice and text.

## Notes

- "Today" is the server's local date (`TZ` env applies).
- Weather comes from Open-Meteo (no key); on failure the briefing just has `"weather": null`.
- Extra endpoints beyond API.md: `GET /api/tasks`, `GET /api/tasks/{id}`, `GET /api/sessions`, `GET /api/sessions/{id}`, `GET /health`.
