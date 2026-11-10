import { afterEach, describe, expect, it, vi } from "vitest";
import { generate } from "./gemini";

const ok = (text: string) => new Response(JSON.stringify({ candidates: [{ content: { parts: [{ text }] } }] }), { status: 200 });
const bad = (msg: string) => new Response(JSON.stringify({ error: { message: msg } }), { status: 400 });

afterEach(() => vi.unstubAllGlobals());

describe("reading the linked news articles", () => {
  it("asks with the URL tool (and without JSON mode) when links should be read", async () => {
    const bodies: Record<string, unknown>[] = [];
    vi.stubGlobal("fetch", async (_u: string, init: RequestInit) => {
      bodies.push(JSON.parse(String(init.body)));
      return ok('{"briefing":"hi"}');
    });
    const t = await generate({ system: "s", user: "u", json: true, readLinks: true, key: "k", model: "gemini-test-a", bypassBudget: true });
    expect(t).toContain("briefing");
    expect(bodies[0].tools).toEqual([{ url_context: {} }]);
    expect((bodies[0].generationConfig as Record<string, unknown>).responseMimeType).toBeUndefined();
  });
  it("falls back to a plain JSON request when the model refuses the tool", async () => {
    const bodies: Record<string, unknown>[] = [];
    vi.stubGlobal("fetch", async (_u: string, init: RequestInit) => {
      bodies.push(JSON.parse(String(init.body)));
      return bodies.length === 1 ? bad("Tool url_context is not supported for this model") : ok('{"briefing":"hi"}');
    });
    await generate({ system: "s", user: "u", json: true, readLinks: true, key: "k", model: "gemini-test-b", bypassBudget: true });
    expect(bodies).toHaveLength(2);
    expect(bodies[1].tools).toBeUndefined();
    expect((bodies[1].generationConfig as Record<string, unknown>).responseMimeType).toBe("application/json");
  });
  it("leaves ordinary requests alone", async () => {
    const bodies: Record<string, unknown>[] = [];
    vi.stubGlobal("fetch", async (_u: string, init: RequestInit) => {
      bodies.push(JSON.parse(String(init.body)));
      return ok("{}");
    });
    await generate({ system: "s", user: "u", json: true, key: "k", model: "gemini-test-c", bypassBudget: true });
    expect(bodies[0].tools).toBeUndefined();
  });
});

describe("Google busy (503)", () => {
  it("waits, tries again, and doesn't count the busy reply", async () => {
    const { _setBusyRetryMs } = await import("./gemini");
    _setBusyRetryMs(1);
    let n = 0;
    vi.stubGlobal("fetch", async () => (++n === 1 ? new Response(JSON.stringify({ error: { message: "The model is overloaded" } }), { status: 503 }) : ok("hello")));
    const t = await generate({ system: "s", user: "u", key: "k", model: "gemini-test-d", bypassBudget: true });
    expect(t).toBe("hello");
    expect(n).toBe(2);
  });
});

describe("streamed replies", () => {
  it("joins the text of every chunk and leaves out thoughts", async () => {
    const { readSse } = await import("./gemini");
    const chunk = (parts: unknown[]) => `data: ${JSON.stringify({ candidates: [{ content: { parts } }] })}\n\n`;
    const body = chunk([{ text: "thinking…", thought: true }]) + chunk([{ text: '{"briefing":"Morn' }]) + chunk([{ text: 'ing!"}' }]);
    let n = 0;
    const t = await readSse(new Response(body), () => n++);
    expect(t).toBe('{"briefing":"Morning!"}');
    expect(n).toBeGreaterThan(0);
  });
  it("asks the streaming endpoint when streaming", async () => {
    const urls: string[] = [];
    vi.stubGlobal("fetch", async (u: string) => {
      urls.push(u);
      return new Response(`data: ${JSON.stringify({ candidates: [{ content: { parts: [{ text: "hi there" }] } }] })}\n\n`);
    });
    const t = await generate({ system: "s", user: "u", key: "k", model: "gemini-test-e", bypassBudget: true, stream: true });
    expect(t).toBe("hi there");
    expect(urls[0]).toContain(":streamGenerateContent?alt=sse");
  });
});
