import type { CapacitorConfig } from "@capacitor/cli";

// Android app: the same web app, run in the app's own WebView. Its storage (tasks, plans, gym, settings, the
// Gemini key) lives in the app's private folder, so clearing Chrome cannot touch it.
// Build: BASE_PATH=/ VITE_NATIVE=1 npm run build:web && npx cap sync android  (CI: .github/workflows/android.yml)
const config: CapacitorConfig = {
  appId: "app.miranda.assistant",
  appName: "Miranda",
  webDir: "dist",
  android: { backgroundColor: "#2A2725" },
  server: { androidScheme: "https" },
};

export default config;
