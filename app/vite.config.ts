import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

// The Rust core runs in the browser as WebAssembly (src/core-wasm/, built by `npm run build:core`).
// VITE_MOCK_CORE=1 swaps in a plain TypeScript stand-in so the UI can be developed without the wasm.
const mock = process.env.VITE_MOCK_CORE === "1";
const base = process.env.BASE_PATH ?? "/";
const src = (p: string) => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
  base,
  clearScreen: false,
  resolve: { alias: { "pa-wasm": mock ? src("./src/dev/mock-core.ts") : src("./src/core-wasm/pa_wasm.js") } },
  plugins: [
    // Built pages only: scripts may come from the app itself and nowhere else, and the app may only talk to the
    // services it uses (Gemini, the weather, the public-holiday list). This protects the saved Gemini key.
    {
      name: "pa-csp",
      apply: "build",
      transformIndexHtml(html: string) {
        const csp = [
          "default-src 'self'",
          "script-src 'self' 'wasm-unsafe-eval'",
          "style-src 'self' 'unsafe-inline'",
          "img-src 'self' data: blob:",
          "media-src 'self' blob: data:",
          "font-src 'self' data:",
          "connect-src 'self' https://generativelanguage.googleapis.com https://api.open-meteo.com https://date.nager.at",
          "worker-src 'self'",
          "manifest-src 'self'",
          "object-src 'none'",
          "base-uri 'self'",
          "form-action 'none'",
        ].join("; ");
        return html.replace("<meta charset=\"UTF-8\" />", `<meta charset="UTF-8" />\n    <meta http-equiv="Content-Security-Policy" content="${csp}" />`);
      },
    },
    VitePWA({
      registerType: "autoUpdate",
      injectRegister: false, // registered by src/updates.ts, which also handles reloading
      includeAssets: ["apple-touch-icon.png", "icon.png"],
      manifest: {
        name: "Miranda — Personal Assistant",
        short_name: "Miranda",
        description: "Your personal assistant: tasks, goals, gym and a daily briefing. Works offline.",
        lang: "en",
        start_url: base,
        scope: base,
        display: "standalone",
        orientation: "portrait",
        background_color: "#2A2725",
        theme_color: "#2A2725",
        icons: [
          { src: "icon-192.png", sizes: "192x192", type: "image/png" },
          { src: "icon-512.png", sizes: "512x512", type: "image/png" },
          { src: "icon-maskable-512.png", sizes: "512x512", type: "image/png", purpose: "maskable" },
        ],
      },
      workbox: {
        globPatterns: ["**/*.{js,css,html,wasm,png,svg,webmanifest}"],
        maximumFileSizeToCacheInBytes: 30 * 1024 * 1024,
        cleanupOutdatedCaches: true,
        skipWaiting: true,
        clientsClaim: true,
        navigateFallback: `${base}index.html`,
        // news.json is refreshed twice a day by a scheduled workflow: never precached, never served as the app shell
        navigateFallbackDenylist: [/news\.json$/],
      },
    }),
  ],
  server: { port: 5173, host: false },
  build: { target: "es2021", outDir: "dist" },
});
