// Public holidays from Nager.Date, filtered to the user's region by the core, cached per year + region.
import { api, getState, today } from "../core";
import type { Holiday } from "../types";
import { ctxCache } from "./cache";
import { emitContextUpdate } from "./events";
import { kvGet, kvSet } from "./idb";

const DB = "pa-holidays";
const MAX_AGE = 7 * 24 * 3600 * 1000;

interface Entry {
  at: number;
  list: Holiday[];
}

export async function refreshHolidays(): Promise<void> {
  const region = (getState().memory.work.holiday_region || "").trim();
  if (!region) {
    ctxCache.holidays = [];
    emitContextUpdate();
    return;
  }
  const country = region.split("-")[0].toUpperCase();
  const year = Number(today().slice(0, 4));
  const all: Holiday[] = [];
  let changed = false;
  for (const y of [year - 1, year, year + 1]) {
    const key = `${y}|${region}`;
    let entry = await kvGet<Entry>(DB, key);
    if (!entry || Date.now() - entry.at > MAX_AGE) {
      try {
        const res = await fetch(`https://date.nager.at/api/v3/PublicHolidays/${y}/${country}`);
        if (!res.ok) throw new Error(String(res.status));
        const raw = await res.json();
        const list = api<Holiday[]>("POST", "/api/holidays/filter", { region, raw });
        entry = { at: Date.now(), list };
        await kvSet(DB, key, entry);
        changed = true;
      } catch {
        // keep the stale entry if there is one
      }
    }
    if (entry) all.push(...entry.list);
  }
  const before = JSON.stringify(ctxCache.holidays);
  ctxCache.holidays = all;
  if (changed || before !== JSON.stringify(all)) emitContextUpdate();
}
