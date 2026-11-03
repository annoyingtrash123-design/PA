// Read-aloud controller: the natural voice (Gemini audio in an HTMLAudioElement) with an automatic fall back to the
// phone voice (Web Speech). One thing plays at a time. UI code subscribes with onPlayState / onProgress.
import { assistantName, getState } from "../core";
import { clockText, voiceRemaining, ttsBlockedUntil } from "./ai-budget";
import { hasKey } from "./cache";
import { budgetDate } from "./gemini";
import { PREVIEW_TEXT, chunkScript, chunkSections, pcmToWav, planVoice, restingReason, synthesize } from "./gemini-tts";
import * as phone from "./speech";
import { getGeminiVoice, getSpeed, getStyle, getVoiceType } from "./voice-prefs";

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
// ---- one read-aloud = several clips played back-to-back; progress and seeking span all of them
interface Session {
  id: number;
  chunks: string[];
  /** each clip: the promise of its audio (started ahead of time, one at a time) and, once there, the Blob */
  fetches: (Promise<Blob> | null)[];
  blobs: (Blob | null)[];
  /** real duration (seconds) once a clip's metadata is known */
  durs: number[];
  idx: number;
  phoneVoice: string;
}
let sess: Session | null = null;
/** Pause between spoken sections, like a breath between paragraphs. */
export const SECTION_GAP_MS = 1000;
const words = (t: string) => t.split(/\s+/).filter(Boolean).length;

/** Seconds a clip lasts: the real duration when known, else an estimate from its words. */
export function clipSeconds(chunks: string[], durs: number[], i: number): number {
  if (durs[i] > 0) return durs[i];
  let sec = 0;
  let w = 0;
  durs.forEach((d, k) => {
    if (d > 0) {
      sec += d;
      w += words(chunks[k]);
    }
  });
  return words(chunks[i]) * (w ? sec / w : 0.42);
}

/** Overall position across clips from the clip index and the time inside it. */
export function overallProgress(chunks: string[], durs: number[], idx: number, inClip: number): Progress {
  let before = 0;
  for (let k = 0; k < idx; k++) before += clipSeconds(chunks, durs, k);
  let total = before;
  for (let k = idx; k < chunks.length; k++) total += clipSeconds(chunks, durs, k);
  return { cur: before + inClip, dur: total };
}

export const progress = (): Progress => {
  const t = audio?.currentTime ?? 0;
  if (sess) return overallProgress(sess.chunks, sess.durs, sess.idx, t);
  return { cur: t, dur: audio && Number.isFinite(audio.duration) ? audio.duration : 0 };
};
export const seek = (t: number) => {
  if (!audio) return;
  if (!sess) {
    audio.currentTime = Math.max(0, t);
    return;
  }
  let start = 0;
  for (let k = 0; k < sess.chunks.length; k++) {
    const len = clipSeconds(sess.chunks, sess.durs, k);
    if (t < start + len || k === sess.chunks.length - 1) {
      if (k === sess.idx) audio.currentTime = Math.max(0, t - start);
      else if (sess.blobs[k]) void startChunk(sess, k, Math.max(0, t - start));
      return;
    }
    start += len;
  }
};

export const formatClock = (sec: number): string => {
  const s = Math.max(0, Math.floor(Number.isFinite(sec) ? sec : 0));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** Playback speed without changing pitch; needs no new synthesis. */
function applySpeed(el: HTMLAudioElement) {
  const a = el as HTMLAudioElement & { webkitPreservesPitch?: boolean };
  a.preservesPitch = true;
  a.webkitPreservesPitch = true;
  a.playbackRate = getSpeed();
}
/** Settings slider moved: apply to whatever is playing right now. */
export function refreshSpeed(): void {
  if (audio) applySpeed(audio);
}

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
  sess = null;
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
    applySpeed(el);
    el.play().then(() => resolve(), reject);
  });
}

/** Start the audio element inside the user's tap; the real clip replaces the silent one when ready. */
function openAudio(): HTMLAudioElement {
  const el = new Audio();
  el.preload = "auto";
  el.src = silentClip();
  void el.play().catch(() => {});
  applySpeed(el);
  audio = el;
  return el;
}

const tz = () => getState().settings.timezone || undefined;

/** Play clip `i` of the session on the shared audio element (from `offset` seconds), then line up what follows. */
async function startChunk(s: Session, i: number, offset = 0): Promise<void> {
  const el = audio;
  if (!el || s.id !== runId) return;
  const blob = s.blobs[i] ?? (await clip(s, i));
  if (s.id !== runId || !audio) return;
  if (url) URL.revokeObjectURL(url);
  url = URL.createObjectURL(blob);
  s.idx = i;
  el.onended = () => void onClipEnd(s, i);
  el.ontimeupdate = el.onloadedmetadata = () => {
    if (Number.isFinite(el.duration) && el.duration > 0) s.durs[i] = el.duration;
    const p = progress();
    progressListeners.forEach((l) => l(p));
  };
  el.onerror = () => {
    if (s.id === runId) void fallbackRest(s, i, new Error("The audio couldn't be played"));
  };
  el.src = url;
  applySpeed(el);
  if (offset > 0) el.currentTime = offset;
  await el.play();
  if (s.id !== runId) return;
  if (audioState !== "paused") setAudio("playing");
  // prefetch the next clip while this one plays
  if (i + 1 < s.chunks.length) void clip(s, i + 1).catch(() => {});
}

/** The audio for clip `i` (requested once; the result is kept so rewinding is free). */
function clip(s: Session, i: number): Promise<Blob> {
  if (!s.fetches[i]) {
    s.fetches[i] = synthesize(s.chunks[i], { voice: getGeminiVoice(), style: getStyle() }).then((b) => {
      s.blobs[i] = b;
      return b;
    });
    s.fetches[i]!.catch(() => {});
  }
  return s.fetches[i]!;
}

async function onClipEnd(s: Session, i: number): Promise<void> {
  if (s.id !== runId) return;
  if (i + 1 >= s.chunks.length) {
    release();
    sess = null;
    setAudio("idle");
    return;
  }
  // a short breath between sections, then the next clip (waiting for it if it is not ready yet)
  await new Promise((r) => setTimeout(r, SECTION_GAP_MS));
  if (s.id !== runId) return;
  try {
    if (!s.blobs[i + 1]) setAudio("loading");
    await clip(s, i + 1);
    if (s.id !== runId) return;
    await startChunk(s, i + 1);
  } catch (e) {
    if (s.id === runId) await fallbackRest(s, i + 1, e);
  }
}

const failReason = (e: unknown, partial: boolean): string => {
  const name = assistantName();
  const until = ttsBlockedUntil(Date.now());
  if (until) return partial ? `Switching to the phone voice for the rest — ${name}'s natural voice is resting until ${clockText(until, tz())}` : restingReason(name, until, tz());
  return `${partial ? "Switching to the phone voice for the rest" : "Using the phone voice"} — ${name}'s natural voice isn't available right now (${(e as Error).message})`;
};

/** The natural voice failed part-way: carry on with the phone voice from the clip that failed. */
async function fallbackRest(s: Session, from: number, e: unknown): Promise<void> {
  if (s.id !== runId) return;
  runId++; // this session is over
  sess = null;
  release();
  setAudio("idle");
  notify(failReason(e, from > 0));
  await phone.speak(s.chunks.slice(from).join("\n\n"), s.phoneVoice);
}

/**
 * Read `text` aloud. Uses the natural voice when chosen and available, otherwise (or on any failure) the phone voice,
 * with a toast saying why. The natural voice reads clips of about 1.5 minutes (`opts.sections`, if given, are the clips)
 * back-to-back; a later clip that fails continues with the phone voice.
 */
export async function playAloud(text: string, phoneVoice: string, opts: { forcePhone?: boolean; sections?: string[] } = {}): Promise<void> {
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

  const chunks = opts.sections && opts.sections.length ? chunkSections(opts.sections) : chunkScript(text);
  if (!chunks.length) return fallback(null);
  const s: Session = { id, chunks, fetches: [], blobs: [], durs: [], idx: 0, phoneVoice };
  sess = s;
  openAudio();
  setAudio("loading");
  try {
    await clip(s, 0);
    if (id !== runId) return;
    await startChunk(s, 0);
  } catch (e) {
    if (id !== runId) return;
    sess = null;
    release();
    setAudio("idle");
    return fallback(failReason(e, false));
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
