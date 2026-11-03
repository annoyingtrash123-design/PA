// The bridge to the Rust core (WebAssembly). The whole State lives in memory and in IndexedDB (`pa-state`);
// every call hands the current state to the core and stores the new one when the response carries it.
import init, { init_state, call as wasmCall, has_stats } from "pa-wasm";
import { kvClear, kvGet, kvSet } from "./services/idb";
import { ctxCache, hasKey } from "./services/cache";
import { zonedNow } from "./util/dates";
import type { CoreResponse, State } from "./types";

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
  }
}

const STATE_DB = "pa-state";
let state: State | null = null;
let ready = false;

export const getState = (): State => {
  if (!state) throw new Error("Core not started");
  return state;
};

export async function startCore(): Promise<void> {
  if (ready) return;
  await init();
  const saved = await kvGet<State>(STATE_DB, "state");
  state = saved && saved.version ? saved : JSON.parse(init_state(zonedNow(undefined).today));
  ready = true;
  if (!saved) void persist();
  // upgrade an older stored state (the core does it on any call; this one makes it persist right away)
  else if (saved.version < 2) api("GET", "/api/state/migrate");
}

let saving: Promise<void> = Promise.resolve();
function persist(): Promise<void> {
  const snapshot = state;
  saving = saving.then(() => kvSet(STATE_DB, "state", snapshot));
  return saving;
}

/** Replace the whole state (restore / reset) and save it. */
export function replaceState(next: State): Promise<void> {
  state = next;
  return persist();
}

export async function resetEverything(): Promise<void> {
  state = JSON.parse(init_state(zonedNow(undefined).today));
  await persist();
  await kvClear("pa-ai-cache");
}

export const hasStats = (text: string): boolean => has_stats(text);

function buildCtx() {
  const { today, now } = zonedNow(state?.settings.timezone);
  return { today, now, has_key: hasKey(), weather: ctxCache.weather, holidays: ctxCache.holidays };
}

/** Call the core like the REST API: returns the response body or throws ApiError. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function api<T = any>(method: string, path: string, body?: unknown): T {
  const raw = wasmCall(JSON.stringify(getState()), method, path, body === undefined ? "null" : JSON.stringify(body), JSON.stringify(buildCtx()));
  const res = JSON.parse(raw) as CoreResponse;
  if (res.status >= 400) {
    const msg = (res.body as { error?: string } | null)?.error ?? `Something went wrong (${res.status})`;
    throw new ApiError(res.status, msg);
  }
  if (res.state) {
    state = res.state;
    void persist();
  } else if (path === "/api/import" && res.body && typeof res.body === "object") {
    state = res.body as State;
    void persist();
  }
  return res.body as T;
}

export const now = () => zonedNow(getState().settings.timezone);
export const today = () => now().today;
export const assistantName = () => getState().settings.assistant_name || "Sunny";
