// Read-aloud controller: the natural voice (Gemini audio in an HTMLAudioElement) with an automatic fall back to the
// phone voice (Web Speech). One thing plays at a time. UI code subscribes with onPlayState / onProgress.
import { assistantName, getState } from "../core";
import { voiceRemaining, ttsBlockedUntil } from "./ai-budget";
import { hasKey } from "./cache";
import { budgetDate } from "./gemini";
import { PREVIEW_TEXT, pcmToWav, planVoice, restingReason, synthesize } from "./gemini-tts";
import * as phone from "./speech";
import { getGeminiVoice, getStyle, getVoiceType } from "./voice-prefs";

export type PlayState = "idle" | "loading" | "playing" | "paused";
export interface Progress {
  cur: number;
  dur: number;
}

let audio: HTMLAudioElement | null = null;
let url = "";
let audioState: PlayState = "idle";
let runId = 0;
let notify: (msg: string) => void = () => {};
const stateListeners = new Set<(s: PlayState) => void>();
const progressListeners = new Set<(p: Progress) => void>();

/** The UI registers its toast function here (keeps this module free of DOM widgets). */
export const setNotifier = (fn: (msg: string) => void) => void (notify = fn);

const emit = () => stateListeners.forEach((l) => l(playState()));
const setAudio = (s: PlayState) => {
  audioState = s;
  emit();
};
phone.onSpeechState(() => emit());

/** The state of whatever is playing (natural voice first, else the phone voice). */
export const playState = (): PlayState => (audioState !== "idle" ? audioState : phone.speechState());
/** True when the natural voice owns the current playback (so the UI shows a progress bar). */
export const isNatural = () => audioState !== "idle";
export function onPlayState(fn: (s: PlayState) => void): () => void {
  stateListeners.add(fn);
  return () => void stateListeners.delete(fn);
}
export function onProgress(fn: (p: Progress) => void): () => void {
  progressListeners.add(fn);
  return () => void progressListeners.delete(fn);
}
export const progress = (): Progress => ({ cur: audio?.currentTime ?? 0, dur: audio && Number.isFinite(audio.duration) ? audio.duration : 0 });
export const seek = (t: number) => {
  if (audio) audio.currentTime = Math.max(0, t);
};

export const formatClock = (sec: number): string => {
  const s = Math.max(0, Math.floor(Number.isFinite(sec) ? sec : 0));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

function release() {
  if (audio) {
    audio.onended = audio.ontimeupdate = audio.onloadedmetadata = audio.onerror = null;
    audio.pause();
    audio.removeAttribute("src");
  }
  if (url) URL.revokeObjectURL(url);
  audio = null;
  url = "";
}

export function stopAll(): void {
  runId++;
  release();
  if (audioState !== "idle") setAudio("idle");
  phone.stop();
}
export function pauseAny(): void {
  if (audioState === "playing") {
    audio?.pause();
    setAudio("paused");
  } else phone.pause();
}
export function resumeAny(): void {
  if (audioState === "paused") {
    void audio?.play();
    setAudio("playing");
  } else phone.resume();
}

let silentUrl = "";
/** A 0.1 s silent clip: playing it inside the tap unlocks audio on iOS for the real clip that arrives later. */
function silentClip(): string {
  if (!silentUrl) silentUrl = URL.createObjectURL(new Blob([pcmToWav(new Uint8Array(4800)) as BlobPart], { type: "audio/wav" }));
  return silentUrl;
}

function startPlayback(blob: Blob, el: HTMLAudioElement, id: number): Promise<void> {
  return new Promise((resolve, reject) => {
    url = URL.createObjectURL(blob);
    el.onended = () => {
      if (id !== runId) return;
      release();
      setAudio("idle");
    };
    el.ontimeupdate = el.onloadedmetadata = () => {
      const p = progress();
      progressListeners.forEach((l) => l(p));
    };
    el.onerror = () => {
      if (id === runId) reject(new Error("The audio couldn't be played"));
    };
    el.src = url;
    el.play().then(() => resolve(), reject);
  });
}

/** Start the audio element inside the user's tap; the real clip replaces the silent one when ready. */
function openAudio(): HTMLAudioElement {
  const el = new Audio();
  el.preload = "auto";
  el.src = silentClip();
  void el.play().catch(() => {});
  audio = el;
  return el;
}

const tz = () => getState().settings.timezone || undefined;

/**
 * Read `text` aloud. Uses the natural voice when chosen in Settings and available, otherwise (or on any failure) the
 * phone voice, with a toast saying why.
 */
export async function playAloud(text: string, phoneVoice: string, opts: { forcePhone?: boolean } = {}): Promise<void> {
  stopAll();
  const id = runId;
  const now = Date.now();
  const plan = planVoice({
    wantNatural: !opts.forcePhone && getVoiceType() === "gemini",
    hasKey: hasKey(),
    blockedUntil: ttsBlockedUntil(now),
    remaining: voiceRemaining(budgetDate()),
    now,
    name: assistantName(),
    timeZone: tz(),
  });
  const fallback = (reason: string | null) => {
    if (reason) notify(reason);
    return phone.speak(text, phoneVoice);
  };
  if (plan.mode === "phone") return fallback(plan.reason);

  const el = openAudio();
  setAudio("loading");
  try {
    const blob = await synthesize(text, { voice: getGeminiVoice(), style: getStyle() });
    if (id !== runId) return;
    await startPlayback(blob, el, id);
    if (id !== runId) return;
    setAudio("playing");
  } catch (e) {
    if (id !== runId) return;
    release();
    setAudio("idle");
    const name = assistantName();
    const until = ttsBlockedUntil(Date.now());
    const reason = until ? restingReason(name, until, tz()) : `Using the phone voice — ${name}'s natural voice isn't available right now (${(e as Error).message})`;
    return fallback(reason);
  }
}

/** Play the short fixed sample for one voice + style (cached forever). Throws on failure. */
export async function previewNatural(voice: string, style: string): Promise<void> {
  stopAll();
  const id = runId;
  const el = openAudio();
  setAudio("loading");
  try {
    const blob = await synthesize(PREVIEW_TEXT, { voice, style, kind: "preview" });
    if (id !== runId) return;
    await startPlayback(blob, el, id);
    if (id === runId) setAudio("playing");
  } catch (e) {
    if (id === runId) {
      release();
      setAudio("idle");
    }
    throw e;
  }
}
