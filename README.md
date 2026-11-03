# Miranda: Personal Assistant

An installable web app (PWA) that runs entirely on your phone: a spoken daily summary, a weekly task planner, goals, and a gym tracker.

**Live app:** https://annoyingtrash123-design.github.io/PA/

- `core/`: all app logic in Rust (tasks, recurrence, gym rotation, briefing templates, AI prompts).
- `wasm/`: the core compiled to WebAssembly for the browser.
- `app/`: TypeScript + Vite UI, installable PWA. See `app/README.md`.
- `.github/workflows/pages.yml`: tests, builds and publishes to GitHub Pages on every push.
- `SPEC.md`: what the app does. `docs/WEB_ARCH.md`: how it fits together. `docs/API.md`: the core's API.

No server: data is stored in the browser (IndexedDB) on your phone. Use Settings → Download backup to keep a copy.
The Gemini API key (optional, free tier) is stored only on your device.

## Develop

```
cd app && npm ci
npm run build:web   # wasm-pack build of the Rust core + vite build
npm run dev         # http://localhost:5173
npm test            # UI unit tests
cargo test -p pa-core
```
