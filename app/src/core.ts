// The bridge to the Rust core (WebAssembly). The whole State lives in memory and in IndexedDB (`pa-state`);
// every call hands the current state to the core and stores the new one when the response carries it.
import init, { init_state, call as wasmCall, has_stats } from "pa-wasm";
import { kvClear, kvDelete, kvGet, kvGetStrict, kvKeys, kvSet } from "./services/idb";
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
  // A read that fails is NOT "nothing saved": starting fresh here would overwrite everything, so stop instead.
  let saved: State | undefined;
  try {
    saved = await kvGetStrict<State>(STATE_DB, "state");
  } catch {
    try {
      saved = await kvGetStrict<State>(STATE_DB, "state");
    } catch (e) {
      throw new Error(`Your saved data couldn't be opened just now (${(e as Error)?.message || "storage error"}). Nothing has been changed. Please close the app fully and open it again.`);
    }
  }
  // a fresh start with snapshots around means the main copy went missing: recover from the newest snapshot
  if (!saved || !saved.version) {
    const snaps = await listSnapshots();
    if (snaps.length) saved = (await kvGet<State>(SNAP_DB, snaps[0].key)) ?? undefined;
  }
  state = saved && saved.version ? saved : JSON.parse(init_state(zonedNow(undefined).today));
  ready = true;
  // ask the browser to keep this app's data even when the phone is short of space (installed apps are normally granted it)
  try {
    void navigator.storage?.persist?.();
  } catch {
    // not supported: nothing to do
  }
  if (!saved) void persist();
  // upgrade an older stored state (the core does it on any call; this one makes it persist right away)
  else api("GET", "/api/state/migrate"); // no-op unless the stored state is older than the core
}

let saving: Promise<void> = Promise.resolve();
function persist(): Promise<void> {
  const snapshot = state;
  saving = saving.then(() => kvSet(STATE_DB, "state", snapshot)).then(() => (snapshot ? snapshotDaily(snapshot) : undefined));
  return saving;
}

// ---- automatic daily snapshots: a copy of everything, once a day, the last 14 days kept (Settings can restore one)
const SNAP_DB = "pa-snapshots";
const SNAP_KEEP = 14;
export interface SnapshotInfo {
  key: string;
  date: string;
  tasks: number;
  goals: number;
  leisure: number;
  sessions: number;
}
const counts = (s: State) => ({ tasks: s.tasks?.length ?? 0, goals: s.goals?.length ?? 0, leisure: (s as unknown as { leisure?: unknown[] }).leisure?.length ?? 0, sessions: s.gym?.sessions?.length ?? 0 });
async function snapshotDaily(s: State): Promise<void> {
  const date = zonedNow(s.settings?.timezone).today;
  const key = `snap:${date}`;
  const c = counts(s);
  // never let an emptier copy replace a fuller one from the same day (an accidental wipe stays recoverable)
  const prev = await kvGet<State>(SNAP_DB, key);
  if (prev) {
    const p = counts(prev);
    if (c.tasks + c.goals + c.leisure + c.sessions < p.tasks + p.goals + p.leisure + p.sessions) return;
  }
  await kvSet(SNAP_DB, key, s);
  const keys = (await kvKeys(SNAP_DB)).filter((k) => k.startsWith("snap:")).sort().reverse();
  for (const old of keys.slice(SNAP_KEEP)) await kvDelete(SNAP_DB, old);
}
/** The snapshots kept on this device, newest first, with what each holds. */
export async function listSnapshots(): Promise<SnapshotInfo[]> {
  const keys = (await kvKeys(SNAP_DB)).filter((k) => k.startsWith("snap:")).sort().reverse();
  const out: SnapshotInfo[] = [];
  for (const key of keys) {
    const s = await kvGet<State>(SNAP_DB, key);
    if (s?.version) out.push({ key, date: key.slice(5), ...counts(s) });
  }
  return out;
}
/** Bring back a snapshot (the current state is snapshotted first, so this can be undone the same way). */
export async function restoreSnapshot(key: string): Promise<void> {
  const s = await kvGet<State>(SNAP_DB, key);
  if (!s?.version) throw new Error("That snapshot couldn't be read");
  if (state) await kvSet(SNAP_DB, `snap:before-restore`, state);
  api("POST", "/api/import", s);
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

function buildCtx(path = "") {
  const { today, now } = zonedNow(state?.settings.timezone);
  const base = { today, now, has_key: hasKey(), weather: ctxCache.weather, holidays: ctxCache.holidays };
  // the news list is big: only briefing calls need it
  return path.startsWith("/api/briefing") || path.startsWith("/api/news") ? { ...base, news: ctxCache.news.items, news_generated_at: ctxCache.news.generated_at } : base;
}

/** Call the core like the REST API: returns the response body or throws ApiError. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function api<T = any>(method: string, path: string, body?: unknown): T {
  const raw = wasmCall(JSON.stringify(getState()), method, path, body === undefined ? "null" : JSON.stringify(body), JSON.stringify(buildCtx(path)));
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
export const assistantName = () => getState().settings.assistant_name || "Miranda";
