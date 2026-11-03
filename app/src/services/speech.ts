// Read-aloud with the Web Speech API (speechSynthesis).
import { speechChunks } from "../util/text";

const PARAGRAPH_PAUSE_MS = 450;

export type SpeechState = "idle" | "playing" | "paused";

let state: SpeechState = "idle";
let runId = 0;
const listeners = new Set<(s: SpeechState) => void>();
const set = (s: SpeechState) => {
  state = s;
  listeners.forEach((l) => l(s));
};

export const speechSupported = () => typeof window !== "undefined" && "speechSynthesis" in window && typeof SpeechSynthesisUtterance !== "undefined";
export const speechState = () => state;
export function onSpeechState(fn: (s: SpeechState) => void): () => void {
  listeners.add(fn);
  return () => void listeners.delete(fn);
}

/** English voices available on this device (waits briefly for the async voice list). */
export async function listVoices(): Promise<SpeechSynthesisVoice[]> {
  if (!speechSupported()) return [];
  const get = () => speechSynthesis.getVoices().filter((v) => /^en([-_]|$)/i.test(v.lang));
  let v = get();
  if (v.length) return v;
  await new Promise<void>((resolve) => {
    const done = () => resolve();
    speechSynthesis.addEventListener("voiceschanged", done, { once: true });
    setTimeout(done, 1200);
  });
  v = get();
  return v;
}

/** Higher is better: prefers en-US, then natural-sounding named voices. */
export function voiceScore(v: { name: string; lang: string; localService?: boolean }): number {
  let s = 0;
  const lang = v.lang.replace("_", "-").toLowerCase();
  if (lang === "en-us") s += 100;
  else if (lang.startsWith("en-")) s += 40;
  if (/google us english/i.test(v.name)) s += 60;
  if (/samantha|aria|jenny|ava|allison|zoe|nicky/i.test(v.name)) s += 55;
  if (/natural|neural|enhanced|premium/i.test(v.name)) s += 35;
  if (/google/i.test(v.name)) s += 20;
  if (/compact|espeak|novelty|zarvox|bells|boing|bad news|cellos|good news|whisper|wobble|trinoids|bubbles|jester|organ|albert|fred|junior|kathy|ralph/i.test(v.name)) s -= 80;
  return s;
}

export function pickVoice(voices: SpeechSynthesisVoice[], saved?: string): SpeechSynthesisVoice | null {
  if (!voices.length) return null;
  const exact = saved ? voices.find((v) => v.name === saved) : undefined;
  if (exact) return exact;
  return [...voices].sort((a, b) => voiceScore(b) - voiceScore(a))[0];
}

export async function speak(text: string, voiceName?: string): Promise<void> {
  if (!speechSupported()) throw new Error("This browser can't read aloud");
  stop();
  const id = ++runId;
  const voice = pickVoice(await listVoices(), voiceName);
  // blank lines separate the spoken parts: a short natural pause after the last chunk of each paragraph
  const chunks: { text: string; pause: number }[] = [];
  for (const para of text.split(/\n\s*\n/)) {
    const cs = speechChunks(para);
    cs.forEach((c, k) => chunks.push({ text: c, pause: k === cs.length - 1 ? PARAGRAPH_PAUSE_MS : 0 }));
  }
  if (!chunks.length || id !== runId) return;
  set("playing");
  let i = 0;
  const next = () => {
    if (id !== runId) return;
    if (i >= chunks.length) return set("idle");
    const chunk = chunks[i++];
    const u = new SpeechSynthesisUtterance(chunk.text);
    if (voice) {
      u.voice = voice;
      u.lang = voice.lang;
    } else u.lang = "en-US";
    u.rate = 0.95;
    u.pitch = 1;
    u.onend = () => (chunk.pause && i < chunks.length ? void setTimeout(next, chunk.pause) : next());
    u.onerror = (e) => {
      if (id !== runId || e.error === "canceled" || e.error === "interrupted") return;
      set("idle");
    };
    speechSynthesis.speak(u);
  };
  next();
}

export function pause() {
  if (state !== "playing") return;
  speechSynthesis.pause();
  set("paused");
}
export function resume() {
  if (state !== "paused") return;
  speechSynthesis.resume();
  set("playing");
}
export function stop() {
  runId++;
  if (speechSupported()) speechSynthesis.cancel();
  if (state !== "idle") set("idle");
}
