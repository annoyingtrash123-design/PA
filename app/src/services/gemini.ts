// Google Gemini (generateContent) client and reply cleaning. No key -> callers use template texts.
import { DAILY_BUDGET, getMainPausedUntil, getPausedUntil, isMainPaused, isPaused, pauseUntilFor, quotaMessage, recordUse, remaining, setMainPausedUntil, setPausedUntil, unrecordUse } from "./ai-budget";
import { getKey, getModel } from "./cache";

/** The cheaper model tried once when the main one is out of quota. */
export const LITE_MODEL = "gemini-3.8-flash-lite";
let liteMissing = false; // remembered for the session once the lite model 404s

let dateProvider: () => string = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};
/** The app sets this to the date in the user's timezone; the daily budget resets on it. */
export const setDateProvider = (fn: () => string) => void (dateProvider = fn);
export const budgetDate = () => dateProvider();

/** Remove markdown decoration and emojis so the text reads (and speaks) naturally. */
export function cleanReply(text: string): string {
  let t = text.replace(/\r/g, "");
  t = t.replace(/```[a-z]*\n?/gi, "").replace(/```/g, "");
  t = t.replace(/^\s{0,3}#{1,6}\s+/gm, ""); // headings
  t = t.replace(/^\s*[-*•]\s+/gm, ""); // bullets
  t = t.replace(/^\s*\d+[.)]\s+/gm, ""); // numbered lists
  t = t.replace(/(\*\*|__)(.*?)\1/g, "$2"); // bold
  t = t.replace(/(^|[\s(])[*_]([^*_\n]+)[*_](?=[\s).,!?;:]|$)/g, "$1$2"); // italics
  t = t.replace(/`([^`]*)`/g, "$1");
  t = t.replace(/\[([^\]]+)\]\([^)]*\)/g, "$1"); // links
  t = t.replace(/[\p{Extended_Pictographic}‍️\u{1F1E6}-\u{1F1FF}\u{1F3FB}-\u{1F3FF}]/gu, "");
  t = t.replace(/[ \t]+\n/g, "\n").replace(/[ \t]{2,}/g, " ").replace(/\n{3,}/g, "\n\n");
  return t.trim();
}

/** Remove ```json fences (also an unclosed opening fence, as in a truncated reply). */
function stripFences(raw: string): string {
  let s = raw.trim();
  const fence = s.match(/```(?:json)?\s*([\s\S]*?)```/i);
  if (fence) return fence[1].trim();
  s = s.replace(/^```(?:json)?\s*/i, "");
  return s.trim();
}

/**
 * Complete top-level fields of a JSON object that was cut off part-way (a reply that hit the output limit).
 * Only fields whose value finished are kept: strings, null, numbers and booleans.
 */
export function salvageFields(raw: string): Record<string, unknown> | null {
  const s = stripFences(raw);
  const start = s.indexOf("{");
  if (start < 0) return null;
  const out: Record<string, unknown> = {};
  let i = start + 1;
  const ws = () => {
    while (i < s.length && /[\s,]/.test(s[i])) i++;
  };
  const readString = (): string | null => {
    // s[i] is the opening quote; returns the decoded string, or null when it never closes
    let j = i + 1;
    while (j < s.length) {
      if (s[j] === "\\") j += 2;
      else if (s[j] === '"') break;
      else j++;
    }
    if (j >= s.length) return null;
    const lit = s.slice(i, j + 1);
    i = j + 1;
    try {
      return JSON.parse(lit) as string;
    } catch {
      return null;
    }
  };
  for (;;) {
    ws();
    if (i >= s.length || s[i] !== '"') break;
    const key = readString();
    if (key === null) break;
    ws();
    if (s[i] !== ":") break;
    i++;
    ws();
    if (s[i] === '"') {
      const v = readString();
      if (v === null) break;
      out[key] = v;
    } else {
      const m = /^(null|true|false|-?\d+(?:\.\d+)?)/.exec(s.slice(i));
      if (!m) break; // a nested value or a cut-off literal: stop here
      i += m[1].length;
      out[key] = m[1] === "null" ? null : m[1] === "true" ? true : m[1] === "false" ? false : Number(m[1]);
    }
  }
  return Object.keys(out).length ? out : null;
}

/** True when the reply looks like JSON that stopped before its closing brace. */
export function looksTruncated(raw: string): boolean {
  const s = stripFences(raw);
  return s.startsWith("{") && !s.trimEnd().endsWith("}");
}

function parseStrict(raw: string): boolean {
  try {
    return typeof JSON.parse(stripFences(raw)) === "object";
  } catch {
    return false;
  }
}

function parseJsonObject(raw: string): Record<string, unknown> | null {
  const s = stripFences(raw);
  const tryParse = (x: string): unknown => {
    try {
      return JSON.parse(x);
    } catch {
      return undefined;
    }
  };
  let obj = tryParse(s);
  if (obj === undefined) {
    const a = s.indexOf("{");
    const b = s.lastIndexOf("}");
    if (a >= 0 && b > a) obj = tryParse(s.slice(a, b + 1));
  }
  if (!obj || typeof obj !== "object") obj = salvageFields(raw);
  if (!obj || typeof obj !== "object") return null;
  return obj as Record<string, unknown>;
}

/** Parse the morning reply {"greeting","weather_comment","affirmation","briefing"}. Tolerant: any field may be missing
 *  (callers fall back to the template part); null only if there is no usable briefing. */
export function parseMorningJson(raw: string): { greeting: string | null; weather_comment: string | null; affirmation: string | null; briefing: string; news: string | null; salvaged?: true } | null {
  const o = parseJsonObject(raw);
  if (!o) return null;
  const salvaged = looksTruncated(raw) && !parseStrict(raw);
  const get = (k: string) => (typeof o[k] === "string" ? cleanReply(o[k] as string) : "") || null;
  const briefing = get("briefing");
  if (!briefing) return null;
  return { greeting: get("greeting"), weather_comment: get("weather_comment"), affirmation: get("affirmation"), briefing, news: get("news"), ...(salvaged ? { salvaged: true as const } : {}) };
}

/** Parse the morning opening {"greeting","weather_comment","affirmation"}; null when nothing usable came back. */
export function parseOpeningJson(raw: string): { greeting: string | null; weather_comment: string | null; affirmation: string | null } | null {
  const o = parseJsonObject(raw);
  if (!o) return null;
  const get = (k: string) => (typeof o[k] === "string" ? cleanReply(o[k] as string) : "") || null;
  const out = { greeting: get("greeting"), weather_comment: get("weather_comment"), affirmation: get("affirmation") };
  return out.greeting || out.affirmation || out.weather_comment ? out : null;
}

/** Parse a news-only reply {"news": string}. Tolerant of fences and chatter; null when there is no usable news. */
export function parseNewsJson(raw: string): string | null {
  const o = parseJsonObject(raw);
  const n = o && typeof o.news === "string" ? cleanReply(o.news) : "";
  return n || null;
}

/** Parse the digest reply: JSON, possibly fenced or with chatter around it. Returns null if unusable. */
export function parseDigestJson(raw: string): { weather_comment: string | null; overview: string } | null {
  const o = parseJsonObject(raw);
  if (!o) return null;
  const overview = typeof o.overview === "string" ? cleanReply(o.overview) : "";
  if (!overview) return null;
  const wc = typeof o.weather_comment === "string" ? cleanReply(o.weather_comment) : "";
  return { weather_comment: wc || null, overview };
}

export interface GenerateOptions {
  system: string;
  user: string;
  json?: boolean;
  model?: string;
  key?: string;
  maxTokens?: number;
  signal?: AbortSignal;
  /** Test key: not blocked by the daily budget (still counted, and still blocked while paused). */
  bypassBudget?: boolean;
  /** let the model read web pages linked in the prompt (Gemini's URL context tool), when the model allows it */
  readLinks?: boolean;
  /** stream the reply: no fixed time limit while words keep arriving (for long texts like the briefings) */
  stream?: boolean;
}

const NO_URL_TOOL = "pa-no-url-context";
/** True once a model has refused the URL context tool (remembered, so we don't spend a request on it again). */
export function urlToolRefused(model: string): boolean {
  try {
    return (localStorage.getItem(NO_URL_TOOL) || "").split(",").includes(model);
  } catch {
    return false;
  }
}
function rememberUrlToolRefused(model: string) {
  try {
    const v = (localStorage.getItem(NO_URL_TOOL) || "").split(",").filter(Boolean);
    if (!v.includes(model)) localStorage.setItem(NO_URL_TOOL, [...v, model].join(","));
  } catch {
    // fine: we'll just try again next time
  }
}

export class GeminiError extends Error {
  /** HTTP status when Gemini answered */
  status?: number;
  /** Google's quota / rate limit (429, RESOURCE_EXHAUSTED) */
  quota = false;
  /** We did not call Gemini at all (paused or out of budget) */
  blocked = false;
  /** epoch ms when the quota should be back */
  pausedUntil?: number;
}

/** Google is busy (overloaded or briefly down): worth one more try after a short wait. */
export const isBusy = (status?: number) => status === 500 || status === 502 || status === 503 || status === 504;
/** How long to wait before trying a busy model again. */
export let BUSY_RETRY_MS = 4000;
export const _setBusyRetryMs = (ms: number) => void (BUSY_RETRY_MS = ms);

/** How long one request may take before we give up and use the template text instead. */
export const GEMINI_TIMEOUT_MS = 45_000;

/** Thinking settings per model family: keep thinking minimal so replies come back quickly. */
export function thinkingFor(model: string): Record<string, unknown> | null {
  if (/2\.5/.test(model)) return { thinkingBudget: 0 };
  if (/^gemini-[3-9]/.test(model)) return { thinkingLevel: "low" };
  return null;
}

/** Streaming: how long to wait for the first words, the longest quiet gap after that, and an overall cap. */
export const FIRST_CHUNK_MS = 60_000;
export const IDLE_MS = 30_000;
export const STREAM_CAP_MS = 240_000;

/** Read a streamed (server-sent events) reply: the text parts of every chunk, thoughts left out. */
export async function readSse(res: Response, onChunk: () => void = () => {}): Promise<string> {
  let out = "";
  const take = (line: string) => {
    if (!line.startsWith("data:")) return;
    const d = line.slice(5).trim();
    if (!d || d === "[DONE]") return;
    const j = JSON.parse(d);
    if (j?.error) throw new GeminiError(String(j.error.message ?? "Gemini sent an error"));
    for (const p of j?.candidates?.[0]?.content?.parts ?? []) if (typeof p?.text === "string" && !p.thought) out += p.text;
  };
  if (!res.body) {
    for (const l of (await res.text()).split(/\r?\n/)) take(l);
    return out;
  }
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    onChunk();
    buf += dec.decode(value, { stream: true });
    let i: number;
    while ((i = buf.indexOf("\n")) >= 0) {
      take(buf.slice(0, i).replace(/\r$/, ""));
      buf = buf.slice(i + 1);
    }
  }
  buf += dec.decode();
  for (const l of buf.split(/\r?\n/)) take(l);
  return out;
}

async function post(model: string, key: string, body: unknown, signal: AbortSignal, stream = false): Promise<Response> {
  const method = stream ? "streamGenerateContent?alt=sse" : "generateContent";
  return fetch(`https://generativelanguage.googleapis.com/v1beta/models/${encodeURIComponent(model)}:${method}`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-goog-api-key": key },
    body: JSON.stringify(body),
    signal,
  });
}

async function errorInfo(res: Response): Promise<{ message: string; exhausted: boolean }> {
  try {
    const j = await res.json();
    if (j?.error?.message) return { message: String(j.error.message), exhausted: j.error.status === "RESOURCE_EXHAUSTED" };
  } catch {
    // keep default
  }
  return { message: `Gemini said ${res.status}`, exhausted: false };
}
const errorMessage = async (res: Response) => (await errorInfo(res)).message;

function blocked(msg: string, pausedUntil?: number): GeminiError {
  const e = new GeminiError(msg);
  e.blocked = true;
  e.quota = pausedUntil !== undefined;
  e.pausedUntil = pausedUntil;
  return e;
}

/** One counted HTTP round trip to a model (with the thinking-setting retry). Throws GeminiError. */
async function callModel(o: GenerateOptions, model: string, key: string): Promise<string> {
  const generationConfig: Record<string, unknown> = { temperature: 0.8, maxOutputTokens: o.maxTokens ?? 4096 };
  const useTool = !!o.readLinks && !urlToolRefused(model);
  // JSON mode and tools don't always mix: with the tool on, the reply is parsed leniently instead
  if (o.json && !useTool) generationConfig.responseMimeType = "application/json";
  const thinking = thinkingFor(model);
  if (thinking) generationConfig.thinkingConfig = thinking;
  const body: Record<string, unknown> = {
    systemInstruction: { parts: [{ text: o.system }] },
    contents: [{ role: "user", parts: [{ text: o.user }] }],
    generationConfig,
  };
  if (useTool) body.tools = [{ url_context: {} }];
  const ctl = new AbortController();
  const stream = !!o.stream;
  let timer = setTimeout(() => ctl.abort(), (stream ? FIRST_CHUNK_MS : GEMINI_TIMEOUT_MS) + (useTool ? 30_000 : 0));
  const cap = stream ? setTimeout(() => ctl.abort(), STREAM_CAP_MS) : 0;
  o.signal?.addEventListener("abort", () => ctl.abort());
  let res: Response;
  try {
    recordUse(budgetDate());
    res = await post(model, key, body, ctl.signal, stream);
    // a model that doesn't accept our thinking setting: try once more without it
    if (!res.ok && res.status === 400 && thinking) {
      const msg = await errorMessage(res.clone());
      if (/thinking/i.test(msg)) {
        delete generationConfig.thinkingConfig;
        recordUse(budgetDate());
        res = await post(model, key, body, ctl.signal, stream);
      }
    }
    // a model that doesn't offer the URL tool: remember it and ask once more without it
    if (!res.ok && res.status === 400 && useTool) {
      const msg = await errorMessage(res.clone());
      if (/url|tool/i.test(msg)) {
        rememberUrlToolRefused(model);
        delete body.tools;
        if (o.json) generationConfig.responseMimeType = "application/json";
        recordUse(budgetDate());
        res = await post(model, key, body, ctl.signal, stream);
      }
    }
    // Google is overloaded right now (503 and friends): it didn't count, so wait a moment and try once more
    if (!res.ok && isBusy(res.status)) {
      unrecordUse(budgetDate());
      await new Promise((r) => setTimeout(r, BUSY_RETRY_MS));
      recordUse(budgetDate());
      res = await post(model, key, body, ctl.signal, stream);
      if (!res.ok && isBusy(res.status)) unrecordUse(budgetDate());
    }
  } catch (e) {
    clearTimeout(timer);
    clearTimeout(cap);
    // the request never reached Google (no connection): it didn't count
    if (!ctl.signal.aborted) unrecordUse(budgetDate());
    throw new GeminiError(ctl.signal.aborted ? "Gemini took too long to answer" : `Couldn't reach Gemini (${(e as Error).message})`);
  }
  if (!stream || !res.ok) {
    clearTimeout(timer);
    clearTimeout(cap);
  }
  if (!res.ok) {
    const info = await errorInfo(res);
    const err = new GeminiError(info.message);
    err.status = res.status;
    if (res.status === 429 || info.exhausted) {
      err.quota = true;
      err.pausedUntil = pauseUntilFor(info.message, Date.now());
    }
    throw err;
  }
  let text: string;
  if (stream) {
    // words keep arriving: keep waiting; a long silence (or the overall cap) gives up
    try {
      text = await readSse(res, () => {
        clearTimeout(timer);
        timer = setTimeout(() => ctl.abort(), IDLE_MS);
      });
    } catch (e) {
      if (e instanceof GeminiError) throw e;
      throw new GeminiError(ctl.signal.aborted ? "Gemini took too long to answer" : `Couldn't read Gemini's reply (${(e as Error).message})`);
    } finally {
      clearTimeout(timer);
      clearTimeout(cap);
    }
  } else {
    const j = await res.json();
    const parts: { text?: string; thought?: boolean }[] = j?.candidates?.[0]?.content?.parts ?? [];
    text = parts.filter((p) => !p.thought).map((p) => p.text ?? "").join("");
  }
  if (!text.trim()) throw new GeminiError("Gemini returned an empty reply");
  return o.json ? text : cleanReply(text);
}

/**
 * One generateContent call; resolves to the reply text. Never calls Gemini while quota-paused or (unless
 * bypassBudget) when the daily budget is used up; those throw a `blocked` error and are not counted.
 * On a quota error for the main model, tries the lite model once.
 */
export async function generate(o: GenerateOptions): Promise<string> {
  const key = o.key ?? getKey();
  if (!key) throw new GeminiError("No Gemini key saved");
  const model = o.model ?? getModel();
  const explicit = o.model !== undefined || o.key !== undefined; // the Test key button: no fallback, no main/lite switching
  const now = Date.now();
  if (isPaused(now)) throw blocked(quotaMessage(getPausedUntil(), now), getPausedUntil());
  if (!o.bypassBudget && remaining(budgetDate()) <= 0) throw blocked(`Today's AI allowance (${DAILY_BUDGET} requests) is used up`);
  const canLite = !explicit && model !== LITE_MODEL && !liteMissing;
  // main model already out of quota: go straight to lite
  if (canLite && isMainPaused(now)) return callLite(o, key, getMainPausedUntil());
  try {
    return await callModel(o, model, key);
  } catch (e) {
    // still busy after the retry, or far too slow: the lighter, faster model is often free when the main one is overloaded
    const slow = e instanceof GeminiError && /took too long/i.test(e.message);
    if (e instanceof GeminiError && (isBusy(e.status) || slow) && canLite && (o.bypassBudget || remaining(budgetDate()) > 0)) {
      try {
        return await callModel(o, LITE_MODEL, key);
      } catch (e2) {
        if (e2 instanceof GeminiError && (e2.status === 404 || (e2.status === 400 && /not found|not supported/i.test(e2.message)))) liteMissing = true;
        throw e;
      }
    }
    if (!(e instanceof GeminiError) || !e.quota) throw e;
    const until = e.pausedUntil ?? pauseUntilFor(e.message, Date.now());
    if (!canLite) {
      setPausedUntil(until);
      throw e;
    }
    setMainPausedUntil(until);
    if (!o.bypassBudget && remaining(budgetDate()) <= 0) {
      setPausedUntil(until);
      throw e;
    }
    return callLite(o, key, until, e);
  }
}

async function callLite(o: GenerateOptions, key: string, mainUntil: number, original?: GeminiError): Promise<string> {
  if (remaining(budgetDate()) <= 0 && !o.bypassBudget) throw blocked(`Today's AI allowance (${DAILY_BUDGET} requests) is used up`);
  try {
    return await callModel(o, LITE_MODEL, key);
  } catch (e) {
    if (e instanceof GeminiError && (e.status === 404 || (e.status === 400 && /not found|not supported/i.test(e.message)))) {
      liteMissing = true;
      setPausedUntil(mainUntil);
      throw original ?? blocked(quotaMessage(mainUntil, Date.now()), mainUntil);
    }
    if (e instanceof GeminiError && e.quota) setPausedUntil(e.pausedUntil ?? mainUntil);
    throw e;
  }
}

/** Test hook: forget the session's "lite model missing" flag. */
export const _resetLiteMissing = () => void (liteMissing = false);
