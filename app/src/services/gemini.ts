// Google Gemini (generateContent) client and reply cleaning. No key -> callers use template texts.
import { getKey, getModel } from "./cache";

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

/** Parse the digest reply: JSON, possibly fenced or with chatter around it. Returns null if unusable. */
export function parseDigestJson(raw: string): { weather_comment: string | null; overview: string } | null {
  let s = raw.trim();
  const fence = s.match(/```(?:json)?\s*([\s\S]*?)```/i);
  if (fence) s = fence[1].trim();
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
  if (!obj || typeof obj !== "object") return null;
  const o = obj as Record<string, unknown>;
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
}

export class GeminiError extends Error {}

/** How long one request may take before we give up and use the template text instead. */
export const GEMINI_TIMEOUT_MS = 45_000;

/** Thinking settings per model family: keep thinking minimal so replies come back quickly. */
export function thinkingFor(model: string): Record<string, unknown> | null {
  if (/2\.5/.test(model)) return { thinkingBudget: 0 };
  if (/^gemini-[3-9]/.test(model)) return { thinkingLevel: "low" };
  return null;
}

async function post(model: string, key: string, body: unknown, signal: AbortSignal): Promise<Response> {
  return fetch(`https://generativelanguage.googleapis.com/v1beta/models/${encodeURIComponent(model)}:generateContent`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-goog-api-key": key },
    body: JSON.stringify(body),
    signal,
  });
}

async function errorMessage(res: Response): Promise<string> {
  try {
    const j = await res.json();
    if (j?.error?.message) return String(j.error.message);
  } catch {
    // keep default
  }
  return `Gemini said ${res.status}`;
}

/** One generateContent call; resolves to the raw reply text. Times out after GEMINI_TIMEOUT_MS. */
export async function generate(o: GenerateOptions): Promise<string> {
  const key = o.key ?? getKey();
  if (!key) throw new GeminiError("No Gemini key saved");
  const model = o.model ?? getModel();
  const generationConfig: Record<string, unknown> = { temperature: 0.8, maxOutputTokens: o.maxTokens ?? 4096 };
  if (o.json) generationConfig.responseMimeType = "application/json";
  const thinking = thinkingFor(model);
  if (thinking) generationConfig.thinkingConfig = thinking;
  const body = {
    systemInstruction: { parts: [{ text: o.system }] },
    contents: [{ role: "user", parts: [{ text: o.user }] }],
    generationConfig,
  };
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), GEMINI_TIMEOUT_MS);
  o.signal?.addEventListener("abort", () => ctl.abort());
  let res: Response;
  try {
    res = await post(model, key, body, ctl.signal);
    // a model that doesn't accept our thinking setting: try once more without it
    if (!res.ok && res.status === 400 && thinking) {
      const msg = await errorMessage(res.clone());
      if (/thinking/i.test(msg)) {
        delete generationConfig.thinkingConfig;
        res = await post(model, key, body, ctl.signal);
      }
    }
  } catch (e) {
    throw new GeminiError(ctl.signal.aborted ? "Gemini took too long to answer" : `Couldn't reach Gemini (${(e as Error).message})`);
  } finally {
    clearTimeout(timer);
  }
  if (!res.ok) throw new GeminiError(await errorMessage(res));
  const j = await res.json();
  const parts: { text?: string }[] = j?.candidates?.[0]?.content?.parts ?? [];
  const text = parts.map((p) => p.text ?? "").join("");
  if (!text.trim()) throw new GeminiError("Gemini returned an empty reply");
  return o.json ? text : cleanReply(text);
}
