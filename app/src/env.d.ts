// Build-time values (vite.config.ts `define`, and VITE_* environment variables).
declare const __APP_VERSION__: string;
interface ImportMetaEnv {
  /** "1" in the Android app build (Capacitor) */
  readonly VITE_NATIVE?: string;
  readonly VITE_SITE_BASE?: string;
}
