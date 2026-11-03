# Miranda: web app

Vanilla TypeScript + Vite + vite-plugin-pwa. The Rust core (`../core`, wrapped by `../wasm`) runs in the browser
as WebAssembly; the whole state is one JSON document kept in IndexedDB (`pa-state`). No server.
Contract: `../docs/WEB_ARCH.md`.

## Run
```sh
npm ci
npm run dev:mock     # UI with the TypeScript mock core (src/dev/mock-core.ts), http://localhost:5173
npm run build:web    # wasm-pack build into src/core-wasm/, then typecheck + vite build
npm run dev          # vite against the real wasm (run `npm run build:core` once first)
npm test             # vitest (pure helpers)
BASE_PATH=/PA/ npm run build:web   # as GitHub Pages serves it
```
`VITE_MOCK_CORE=1` aliases the `pa-wasm` import to the mock instead of `src/core-wasm/pa_wasm.js`.

## Layout
- `src/core.ts` wasm bridge + persistence; `src/services/` weather (Open-Meteo), holidays (Nager.Date), Gemini, AI text orchestration (`pa-ai-cache`), speech.
- `src/views/` Home, Tasks, Goals, Gym, Review, Settings; `src/util/` pure helpers (tested).
- The Gemini key lives in localStorage `pa-gemini-key` (never in State or backups).
- `scripts/gen-icons.cjs` renders `scripts/icon.svg` to the PNG icons; `scripts/screenshots.cjs` takes the review screenshots.
