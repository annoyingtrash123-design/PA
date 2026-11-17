/** Differences of the Android app build (Capacitor, `VITE_NATIVE=1`) from the website. Everything here is a no-op
 *  on the web, and the Capacitor plugins are only loaded in the app. */

export const IS_NATIVE = import.meta.env.VITE_NATIVE === "1";

/** Where the news list comes from: the website (in the app, everything else is bundled). */
export const SITE_BASE: string = IS_NATIVE
  ? ((import.meta.env.VITE_SITE_BASE as string | undefined) ?? "https://annoyingtrash123-design.github.io/PA/")
  : import.meta.env.BASE_URL;

/** Read aloud with Android's text-to-speech (the WebView has no speechSynthesis). Resolves when it finishes. */
export async function nativeSpeak(text: string, rate: number): Promise<void> {
  const { TextToSpeech } = await import("@capacitor-community/text-to-speech");
  await TextToSpeech.stop().catch(() => undefined); // nothing playing is fine
  await TextToSpeech.speak({ text, lang: "en-AU", rate, category: "playback" });
}
export async function nativeStopSpeaking(): Promise<void> {
  const { TextToSpeech } = await import("@capacitor-community/text-to-speech");
  await TextToSpeech.stop().catch(() => undefined);
}

/** Save a file and offer it through the share sheet (downloads don't work in a WebView). */
export async function nativeSaveFile(name: string, text: string): Promise<void> {
  const { Filesystem, Directory, Encoding } = await import("@capacitor/filesystem");
  const { Share } = await import("@capacitor/share");
  const r = await Filesystem.writeFile({ path: name, data: text, directory: Directory.Cache, encoding: Encoding.UTF8 });
  await Share.share({ title: name, files: [r.uri] });
}

/** Android back button: goes through the app's own back handling (src/util/back.ts), which listens to history. */
export async function installBackButton(): Promise<void> {
  if (!IS_NATIVE) return;
  const { App } = await import("@capacitor/app");
  await App.addListener("backButton", ({ canGoBack }) => {
    if (canGoBack) history.back();
    else void App.exitApp();
  });
}

/** Leave the app (the back button on Home with nothing left to undo). */
export async function nativeExit(): Promise<void> {
  const { App } = await import("@capacitor/app");
  await App.exitApp();
}

/** Open a link in the phone's browser (an APK download, AI Studio). */
export function openExternal(url: string): void {
  window.open(url, "_blank");
}
