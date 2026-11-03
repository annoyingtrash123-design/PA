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
