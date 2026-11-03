/** Split a long summary into paragraphs for reading (port of HomeView._paragraphs). */
export function paragraphs(text: string): string[] {
  const clean = text.replace(/\r/g, "");
  let paras = clean.split(/\n\s*\n/).map((p) => p.trim()).filter(Boolean);
  if (paras.length <= 1 && clean.length > 500) {
    const sents = clean.trim().split(/(?<=[.!?])\s+/);
    const n = Math.max(3, Math.min(5, Math.round(sents.length / 4)));
    const size = Math.ceil(sents.length / n);
    paras = [];
    for (let i = 0; i < sents.length; i += size) paras.push(sents.slice(i, i + size).join(" "));
  }
  return paras;
}

/** Split text into sentence-sized chunks (<= max chars) so speech synthesis never hits Chrome's ~15 s cutoff. */
export function speechChunks(text: string, max = 180): string[] {
  const out: string[] = [];
  const sentences = text.replace(/\s+/g, " ").trim().match(/[^.!?]+[.!?]+["')\]]*\s*|[^.!?]+$/g) ?? [];
  let cur = "";
  const push = () => {
    if (cur.trim()) out.push(cur.trim());
    cur = "";
  };
  for (const s of sentences) {
    if (s.length > max) {
      push();
      // very long sentence: break on commas/semicolons, then on spaces
      let piece = "";
      for (const part of s.split(/(?<=[,;:])\s+/)) {
        if ((piece + " " + part).trim().length > max && piece) {
          out.push(piece.trim());
          piece = part;
        } else piece = (piece + " " + part).trim();
        while (piece.length > max) {
          const cut = piece.lastIndexOf(" ", max);
          const at = cut > 0 ? cut : max;
          out.push(piece.slice(0, at).trim());
          piece = piece.slice(at).trim();
        }
      }
      if (piece) out.push(piece.trim());
    } else if ((cur + s).length > max) {
      push();
      cur = s;
    } else cur += s;
  }
  push();
  return out;
}

/** "5" -> 5, "62,5" -> 62.5, junk -> fallback. */
export function parseNum(s: string, fallback = 0): number {
  const n = Number(String(s).replace(",", ".").trim());
  return Number.isFinite(n) && String(s).trim() !== "" ? n : fallback;
}

export function fmtWeight(w: number | null | undefined): string {
  if (w === null || w === undefined || (w as unknown) === "") return "";
  const n = Number(w);
  if (!Number.isFinite(n)) return String(w);
  return String(n);
}

/** "4 × 10" when min and max reps are equal, else "3 × 8–12". */
export function fmtSetsReps(sets: number, min: number, max: number): string {
  return min === max ? `${sets} × ${min}` : `${sets} × ${min}–${max}`;
}

/** "10" or "8–12". */
export function fmtRepRange(min: number, max: number): string {
  return min === max ? String(min) : `${min}–${max}`;
}
