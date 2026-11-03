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

/** One generateContent call; resolves to the raw reply text. */
export async function generate(o: GenerateOptions): Promise<string> {
  const key = o.key ?? getKey();
  if (!key) throw new GeminiError("No Gemini key saved");
  const model = o.model ?? getModel();
  const generationConfig: Record<string, unknown> = { temperature: 0.8, maxOutputTokens: o.maxTokens ?? 2048 };
  if (o.json) generationConfig.responseMimeType = "application/json";
  if (/2\.5/.test(model)) generationConfig.thinkingConfig = { thinkingBudget: 0 };
  const res = await fetch(`https://generativelanguage.googleapis.com/v1beta/models/${encodeURIComponent(model)}:generateContent`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-goog-api-key": key },
    body: JSON.stringify({
      systemInstruction: { parts: [{ text: o.system }] },
      contents: [{ role: "user", parts: [{ text: o.user }] }],
      generationConfig,
    }),
    signal: o.signal,
  });
  if (!res.ok) {
    let msg = `Gemini said ${res.status}`;
    try {
      const j = await res.json();
      if (j?.error?.message) msg = String(j.error.message);
    } catch {
      // keep default
    }
    throw new GeminiError(msg);
  }
  const j = await res.json();
  const parts: { text?: string }[] = j?.candidates?.[0]?.content?.parts ?? [];
  const text = parts.map((p) => p.text ?? "").join("");
  if (!text.trim()) throw new GeminiError("Gemini returned an empty reply");
  return o.json ? text : cleanReply(text);
}
