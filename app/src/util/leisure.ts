// Pure helpers for the leisure list (Plans tab) and its AI blurbs.
import type { Leisure, LeisureKind } from "../types";

export const KINDS: { id: LeisureKind; label: string; icon: string }[] = [
  { id: "watch", label: "Watch", icon: "tv" },
  { id: "read", label: "Read", icon: "book" },
  { id: "listen", label: "Listen", icon: "headphones" },
  { id: "play", label: "Play", icon: "gamepad" },
  { id: "try", label: "Try", icon: "compass" },
];

export const kindIcon = (k: LeisureKind): string => KINDS.find((x) => x.id === k)?.icon ?? "compass";

/** Stored when the model says it does not know the work. */
export const UNKNOWN_BLURB = "I don't know much about this one yet.";

/** Open (not finished) items for a kind filter, current ones first, otherwise in the given order. */
export function openItems(items: Leisure[], filter: LeisureKind | "all"): Leisure[] {
  const v = items.filter((l) => l.status !== "finished" && (filter === "all" || l.kind === filter));
  return [...v.filter((l) => l.status === "current"), ...v.filter((l) => l.status !== "current")];
}

/** Finished items, most recently finished first. */
export function finishedItems(items: Leisure[]): Leisure[] {
  return items.filter((l) => l.status === "finished").sort((a, b) => (b.finished_on ?? "").localeCompare(a.finished_on ?? ""));
}

/** What a model reply means for a blurb: null = unusable (keep what we have), UNKNOWN_BLURB, or the cleaned text. */
export function blurbFromReply(raw: string): string | null {
  const t = raw.replace(/\r/g, "").replace(/\s+/g, " ").trim().replace(/^["'“”]+|["'“”]+$/g, "").trim();
  if (!t) return null;
  if (/^unknown\W*$/i.test(t)) return UNKNOWN_BLURB;
  if (/^unknown\b/i.test(t) && t.length < 40) return UNKNOWN_BLURB;
  return t.length > 700 ? t.slice(0, 700).replace(/\s+\S*$/, "") : t;
}
