// Gemini text-to-speech ("Natural voice"): generateContent with responseModalities ["AUDIO"], raw PCM wrapped into WAV,
// counted against the same daily budget / quota pauses as the text calls, and cached in IndexedDB (`pa-tts-cache`).
import { VOICE_BUDGET, clockText, pauseUntilFor, quotaMessage, recordVoiceUse, setTtsPausedUntil, ttsBlockedUntil, voiceRemaining } from "./ai-budget";
import { getKey } from "./cache";
import { GeminiError, budgetDate } from "./gemini";
import { kvDelete, kvGet, kvSet } from "./idb";

/** Newest first; a model that 404s (or is gone) is skipped for the next one. Verify against ai.google.dev/gemini-api/docs/speech-generation. */
export const TTS_MODELS = ["gemini-3.1-flash-tts-preview", "gemini-2.5-flash-preview-tts"];
/** One chunk (about 1.5 minutes of speech) can still take a while to synthesise. */
export const TTS_TIMEOUT_MS = 90_000;
/** Far below the documented 8,192-token input limit; longer texts are cut at a sentence/paragraph boundary. */
export const MAX_TTS_CHARS = 12_000;
export const PREVIEW_TEXT = "Hi, I'm Miranda. Here's how I'd sound reading your day.";

export interface TtsVoice {
  name: string;
  trait: string;
}
/** Google's female prebuilt voices and their one-word characters from the docs (the male-sounding ones are left out). */
export const VOICES: TtsVoice[] = [
  ["Sulafat", "Warm"], ["Achernar", "Soft"], ["Aoede", "Breezy"], ["Vindemiatrix", "Gentle"], ["Callirrhoe", "Easy-going"],
  ["Despina", "Smooth"], ["Laomedeia", "Upbeat"], ["Leda", "Youthful"], ["Zephyr", "Bright"], ["Autonoe", "Bright"],
  ["Erinome", "Clear"], ["Kore", "Firm"], ["Gacrux", "Mature"], ["Pulcherrima", "Forward"],
].map(([name, trait]) => ({ name, trait }));
/** Warm, soft, female-sounding and listed as "Warm" in the docs: suits a chatty, caring assistant. */
export const DEFAULT_VOICE = "Sulafat";

export interface TtsStyle {
  id: string;
  label: string;
  instruction: string;
}
export const STYLES: TtsStyle[] = [
  { id: "warm", label: "Warm & chatty", instruction: "Say warmly and chattily, like a caring friend talking to you over coffee" },
  { id: "calm", label: "Calm & soothing", instruction: "Say calmly and soothingly, in a slow, gentle, relaxed way" },
  { id: "upbeat", label: "Upbeat", instruction: "Say in an upbeat, cheerful and encouraging way" },
  { id: "plain", label: "Matter-of-fact", instruction: "Say in a clear, matter-of-fact, neutral way" },
];
export const DEFAULT_STYLE = "warm";

export function buildPrompt(text: string, styleId: string): string {
  const s = STYLES.find((x) => x.id === styleId) ?? STYLES[0];
  // constant wording (cached audio is keyed by the style id, not the speed: the speed is applied with playbackRate only)
  return `${s.instruction}, at a relaxed, unhurried pace: ${text.trim()}`;
}

/** Cut over-long text at the last paragraph (else sentence) boundary before the limit. */
export function clampText(text: string, max = MAX_TTS_CHARS): string {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const para = cut.lastIndexOf("\n\n");
  if (para > max * 0.5) return cut.slice(0, para).trim();
  const sent = Math.max(cut.lastIndexOf(". "), cut.lastIndexOf("! "), cut.lastIndexOf("? "));
  return (sent > max * 0.5 ? cut.slice(0, sent + 1) : cut).trim();
}

// ---------------- chunks ----------------
// Gemini TTS drifts (the voice degrades, deepens, slurs) on long single recordings, so a script is read as several
// clips of about 1.5 minutes each (about 220-260 words), split at section / paragraph boundaries and never mid-sentence.
export const CHUNK_TARGET_WORDS = 240;
export const CHUNK_SOFT_MAX_WORDS = 260;
export const CHUNK_HARD_MAX_WORDS = 300;
/** A section (a spoken part of a briefing) stays one clip unless it runs past about two minutes. */
export const SECTION_MAX_WORDS = 300;

const wordCount = (s: string) => s.split(/\s+/).filter(Boolean).length;

/** Sentences of a paragraph. An ellipsis ("…" or "...") and abbreviations ("U.S.", "Mr.") are not sentence ends. */
export function splitSentences(text: string): string[] {
  const t = text.replace(/\s+/g, " ").trim();
  const out: string[] = [];
  let start = 0;
  for (let i = 0; i < t.length; i++) {
    const c = t[i];
    if (c !== "." && c !== "!" && c !== "?") continue;
    if (t[i + 1] === "." || t[i - 1] === ".") continue; // "..." or "U.S."
    let j = i + 1;
    while (j < t.length && /["'”’)]/.test(t[j])) j++;
    if (j < t.length && t[j] !== " ") continue;
    const word = t.slice(start, i).split(" ").pop() ?? "";
    if (c === "." && (/^[A-Z]$/.test(word) || word.includes(".") || /^(Mr|Mrs|Ms|Dr|St|No|Jr|Sr)$/.test(word))) continue;
    out.push(t.slice(start, j).trim());
    start = j;
  }
  if (start < t.length) out.push(t.slice(start).trim());
  return out.filter(Boolean);
}

/** Pieces of one over-long paragraph, each at most `max` words and ending at a sentence end. */
function splitParagraph(p: string, max: number): string[] {
  const pieces: string[] = [];
  let cur: string[] = [];
  let n = 0;
  for (const s of splitSentences(p)) {
    const w = wordCount(s);
    if (cur.length && n + w > max) {
      pieces.push(cur.join(" "));
      cur = [];
      n = 0;
    }
    cur.push(s);
    n += w;
  }
  if (cur.length) pieces.push(cur.join(" "));
  return pieces;
}

/** Split a script into chunks of about 1.5 minutes at paragraph boundaries (long paragraphs at sentence ends). */
export function chunkScript(text: string): string[] {
  const paras = text.split(/\n\s*\n/).map((p) => p.trim()).filter(Boolean);
  const units = paras.flatMap((p) => (wordCount(p) > CHUNK_SOFT_MAX_WORDS ? splitParagraph(p, CHUNK_SOFT_MAX_WORDS) : [p]));
  const chunks: string[][] = [];
  let cur: string[] = [];
  let n = 0;
  for (const u of units) {
    const w = wordCount(u);
    const fits = n + w <= CHUNK_SOFT_MAX_WORDS || (n < 180 && n + w <= CHUNK_HARD_MAX_WORDS);
    if (cur.length && !fits) {
      chunks.push(cur);
      cur = [];
      n = 0;
    }
    cur.push(u);
    n += w;
  }
  if (cur.length) chunks.push(cur);
  // a tiny last chunk joins the one before it
  if (chunks.length > 1) {
    const last = chunks[chunks.length - 1];
    const prev = chunks[chunks.length - 2];
    if (wordCount(last.join(" ")) < 80 && wordCount(prev.join(" ")) + wordCount(last.join(" ")) <= 340) {
      prev.push(...last);
      chunks.pop();
    }
  }
  return chunks.map((c) => c.join("\n\n"));
}

/** Briefing sections are the clips (one each); a section over about two minutes is split further at paragraph/sentence ends. */
export function chunkSections(sections: string[]): string[] {
  return sections.map((s) => s.trim()).filter(Boolean).flatMap((s) => (wordCount(s) > SECTION_MAX_WORDS ? chunkScript(s) : [s]));
}

// ---------------- audio ----------------
/** Sample rate from a mimeType like "audio/L16;codec=pcm;rate=24000" (default 24000). */
export function parseSampleRate(mime: string | undefined | null): number {
  const m = /rate=(\d{4,6})/i.exec(mime ?? "");
  const r = m ? Number(m[1]) : 24000;
  return r >= 8000 && r <= 192000 ? r : 24000;
}

/** Wrap raw little-endian PCM in a 44-byte RIFF/WAVE header. */
export function pcmToWav(pcm: Uint8Array, sampleRate = 24000, channels = 1, bits = 16): Uint8Array {
  const out = new Uint8Array(44 + pcm.length);
  const v = new DataView(out.buffer);
  const str = (o: number, s: string) => [...s].forEach((c, i) => v.setUint8(o + i, c.charCodeAt(0)));
  str(0, "RIFF");
  v.setUint32(4, 36 + pcm.length, true);
  str(8, "WAVE");
  str(12, "fmt ");
  v.setUint32(16, 16, true);
  v.setUint16(20, 1, true); // PCM
  v.setUint16(22, channels, true);
  v.setUint32(24, sampleRate, true);
  v.setUint32(28, (sampleRate * channels * bits) / 8, true);
  v.setUint16(32, (channels * bits) / 8, true);
  v.setUint16(34, bits, true);
  str(36, "data");
  v.setUint32(40, pcm.length, true);
  out.set(pcm, 44);
  return out;
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Turn an inlineData part into a playable Blob: PCM -> WAV; anything already a container is passed through. */
export function audioBlobFrom(base64: string, mime: string | undefined): Blob {
  const bytes = base64ToBytes(base64);
  const type = (mime ?? "").toLowerCase();
  if (type.startsWith("audio/") && !/l16|pcm|raw/.test(type)) return new Blob([bytes as BlobPart], { type: type.split(";")[0] });
  return new Blob([pcmToWav(bytes, parseSampleRate(mime)) as BlobPart], { type: "audio/wav" });
}

// ---------------- cache ----------------
export const TTS_DB = "pa-tts-cache";
export const INDEX_KEY = "__index";
export const MAX_CLIPS = 16;
export const MAX_AGE_MS = 3 * 24 * 3600 * 1000;

function fnv(s: string, seed: number): string {
  let h = seed >>> 0;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}
/** Stable key for model+voice+style+text (two 32-bit FNV hashes + length). */
export function ttsCacheKey(model: string, voice: string, style: string, text: string): string {
  const s = `${model}\u0001${voice}\u0001${style}\u0001${text}`;
  return `tts-${fnv(s, 0x811c9dc5)}${fnv(s, 0x01000193)}-${s.length}`;
}

export interface IndexEntry {
  key: string;
  at: number;
  kind: "clip" | "preview";
}
/** Which cached entries to drop: clips older than 3 days, then all but the newest 10. Previews are kept forever. */
export function pruneIndex(index: IndexEntry[], now: number, max = MAX_CLIPS, maxAge = MAX_AGE_MS): { keep: IndexEntry[]; drop: IndexEntry[] } {
  const previews = index.filter((e) => e.kind === "preview");
  const clips = index.filter((e) => e.kind !== "preview").sort((a, b) => b.at - a.at);
  const fresh = clips.filter((e) => now - e.at <= maxAge).slice(0, max);
  const keepSet = new Set(fresh);
  return { keep: [...previews, ...fresh], drop: clips.filter((e) => !keepSet.has(e)) };
}

interface Stored {
  buf: ArrayBuffer;
  type: string;
}
async function cacheGet(key: string): Promise<Blob | null> {
  const s = await kvGet<Stored>(TTS_DB, key);
  return s?.buf ? new Blob([s.buf], { type: s.type }) : null;
}
async function cachePut(key: string, blob: Blob, kind: IndexEntry["kind"], now = Date.now()): Promise<void> {
  await kvSet(TTS_DB, key, { buf: await blob.arrayBuffer(), type: blob.type } satisfies Stored);
  const index = ((await kvGet<IndexEntry[]>(TTS_DB, INDEX_KEY)) ?? []).filter((e) => e.key !== key);
  index.push({ key, at: now, kind });
  const { keep, drop } = pruneIndex(index, now);
  for (const e of drop) await kvDelete(TTS_DB, e.key);
  await kvSet(TTS_DB, INDEX_KEY, keep);
}

// ---------------- availability / fallback decision ----------------
export type VoicePlan = { mode: "gemini" } | { mode: "phone"; reason: string | null };
export interface PlanInput {
  wantNatural: boolean;
  hasKey: boolean;
  /** epoch ms until which the natural voice is paused (0 = not paused) */
  blockedUntil: number;
  remaining: number;
  now: number;
  name?: string;
  timeZone?: string;
}
export const restingReason = (name: string, until: number, timeZone?: string) => `Using the phone voice — ${name}'s natural voice is resting until ${clockText(until, timeZone)}`;
/** Natural voice when chosen and usable; otherwise the phone voice with a short reason (null when phone was the choice). */
export function planVoice(i: PlanInput): VoicePlan {
  if (!i.wantNatural) return { mode: "phone", reason: null };
  const name = i.name || "Miranda";
  if (!i.hasKey) return { mode: "phone", reason: `Using the phone voice — add a Gemini key in Settings to hear ${name}'s natural voice` };
  if (i.blockedUntil > i.now) return { mode: "phone", reason: restingReason(name, i.blockedUntil, i.timeZone) };
  if (i.remaining <= 0) return { mode: "phone", reason: `Using the phone voice — today's voice allowance is used up (it resets at midnight)` };
  return { mode: "gemini" };
}

// ---------------- request ----------------
export interface SynthOptions {
  voice: string;
  style: string;
  /** previews are cached forever; clips are pruned */
  kind?: IndexEntry["kind"];
  signal?: AbortSignal;
  key?: string;
}

const blockedErr = (msg: string, until?: number) => {
  const e = new GeminiError(msg);
  e.blocked = true;
  e.quota = until !== undefined;
  e.pausedUntil = until;
  return e;
};

async function requestAudio(model: string, key: string, text: string, o: SynthOptions, signal: AbortSignal): Promise<Blob> {
  const body = {
    contents: [{ role: "user", parts: [{ text: buildPrompt(text, o.style) }] }],
    generationConfig: { responseModalities: ["AUDIO"], speechConfig: { voiceConfig: { prebuiltVoiceConfig: { voiceName: o.voice } } } },
  };
  recordVoiceUse(budgetDate());
  const res = await fetch(`https://generativelanguage.googleapis.com/v1beta/models/${encodeURIComponent(model)}:generateContent`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-goog-api-key": key },
    body: JSON.stringify(body),
    signal,
  });
  if (!res.ok) {
    let message = `Gemini said ${res.status}`;
    let exhausted = false;
    try {
      const j = await res.json();
      if (j?.error?.message) message = String(j.error.message);
      exhausted = j?.error?.status === "RESOURCE_EXHAUSTED";
    } catch {
      // keep default
    }
    const err = new GeminiError(message);
    err.status = res.status;
    if (res.status === 429 || exhausted) {
      err.quota = true;
      err.pausedUntil = pauseUntilFor(message, Date.now());
    }
    throw err;
  }
  const j = await res.json();
  const parts: { inlineData?: { data?: string; mimeType?: string }; inline_data?: { data?: string; mime_type?: string } }[] = j?.candidates?.[0]?.content?.parts ?? [];
  for (const p of parts) {
    const d = p.inlineData?.data ?? p.inline_data?.data;
    if (d) return audioBlobFrom(d, p.inlineData?.mimeType ?? p.inline_data?.mime_type);
  }
  throw new GeminiError("Gemini returned no audio");
}

/** TTS models discovered from Google's model list (newest first), used when the built-in names have been retired. */
const DISCOVERED_KEY = "pa-tts-models";
function discovered(): string[] {
  try {
    return JSON.parse(localStorage.getItem(DISCOVERED_KEY) || "[]");
  } catch {
    return [];
  }
}

/** Rank TTS model ids: flash before pro, higher version first. Exported for tests. */
export function rankTtsModels(names: string[]): string[] {
  const ver = (n: string) => parseFloat((n.match(/gemini-(\d+(?:\.\d+)?)/) || [])[1] || "0");
  return [...new Set(names)]
    .filter((n) => /tts/i.test(n))
    .sort((a, b) => Number(/pro/.test(a)) - Number(/pro/.test(b)) || ver(b) - ver(a));
}

/** Ask Google which speech models this key can use (doesn't count toward the budget: it's a free metadata call). */
async function discoverTtsModels(key: string, signal: AbortSignal): Promise<string[]> {
  try {
    const res = await fetch("https://generativelanguage.googleapis.com/v1beta/models?pageSize=200", { headers: { "x-goog-api-key": key }, signal });
    if (!res.ok) return [];
    const j = await res.json();
    const names: string[] = (j?.models ?? [])
      .filter((m: { supportedGenerationMethods?: string[] }) => (m.supportedGenerationMethods ?? []).includes("generateContent"))
      .map((m: { name: string }) => String(m.name).replace(/^models\//, ""));
    const ranked = rankTtsModels(names);
    if (ranked.length) localStorage.setItem(DISCOVERED_KEY, JSON.stringify(ranked));
    return ranked;
  } catch {
    return [];
  }
}

const modelGone = (e: unknown) => e instanceof GeminiError && (e.status === 404 || e.status === 410 || (e.status === 400 && /not found|not supported|no longer/i.test(e.message)));

/**
 * Speech for `text` as an audio Blob (audio/wav). Replays come from the cache (instant, free). Otherwise one counted
 * request per model tried (404 -> next model; 429 -> next model, and when every model is out of quota the natural voice
 * is paused until the retry time). Throws GeminiError (`blocked` when no request was made).
 */
export async function synthesize(text: string, o: SynthOptions): Promise<Blob> {
  const clean = clampText(text.trim());
  if (!clean) throw new GeminiError("Nothing to read");
  const cacheKey = ttsCacheKey(TTS_MODELS[0], o.voice, o.style, clean);
  const cached = await cacheGet(cacheKey);
  if (cached) return cached;

  const key = o.key ?? getKey();
  if (!key) throw new GeminiError("No Gemini key saved");
  const now = Date.now();
  const until = ttsBlockedUntil(now);
  if (until) throw blockedErr(quotaMessage(until, now), until);
  if (voiceRemaining(budgetDate()) <= 0) throw blockedErr(`Today's voice allowance (${VOICE_BUDGET} clips) is used up`);

  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), TTS_TIMEOUT_MS);
  o.signal?.addEventListener("abort", () => ctl.abort());
  let quotaErr: GeminiError | null = null;
  let lastErr: GeminiError | null = null;
  const tried = new Set<string>();
  let candidates = [...new Set([...discovered(), ...TTS_MODELS])];
  try {
    for (let round = 0; round < 2; round++) {
    for (const model of candidates) {
      if (tried.has(model)) continue;
      tried.add(model);
      if (voiceRemaining(budgetDate()) <= 0) break;
      try {
        const blob = await requestAudio(model, key, clean, o, ctl.signal);
        void cachePut(cacheKey, blob, o.kind ?? "clip");
        return blob;
      } catch (e) {
        if (ctl.signal.aborted) throw new GeminiError(o.signal?.aborted ? "Cancelled" : "Gemini took too long to make the audio");
        if (!(e instanceof GeminiError)) throw new GeminiError(`Couldn't reach Gemini (${(e as Error).message})`);
        lastErr = e;
        if (e.quota) quotaErr = e;
        else if (!modelGone(e)) throw e;
      }
    }
    // every known name was retired: ask Google for the current speech models and try those once
    if (round === 0 && !quotaErr && voiceRemaining(budgetDate()) > 0) {
      const fresh = (await discoverTtsModels(key, ctl.signal)).filter((m) => !tried.has(m));
      if (!fresh.length) break;
      candidates = fresh;
    } else break;
    }
  } finally {
    clearTimeout(timer);
  }
  if (quotaErr) {
    setTtsPausedUntil(quotaErr.pausedUntil ?? pauseUntilFor(quotaErr.message, Date.now()));
    throw quotaErr;
  }
  throw lastErr ?? blockedErr(`Today's voice allowance (${VOICE_BUDGET} clips) is used up`);
}
