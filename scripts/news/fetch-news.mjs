#!/usr/bin/env node
// Fetches the free RSS/Atom feeds in sources.json and writes news.json for the Miranda app.
// Node 22, built-in fetch only, no dependencies. Run by .github/workflows/pages.yml twice a day.
//   node scripts/news/fetch-news.mjs [--out app/public/news.json]
// A feed that fails is skipped and logged. If everything fails the output still has `items: []`.
// Only the headline and the feed's lead paragraph are kept. Article pages are fetched (politely, a few per run) only
// when a feed gives almost no lead, and only one paragraph is ever taken from them.
import { createHash } from "node:crypto";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const UA = "MirandaNewsBot/1.0 (personal reader)";
export const WINDOW_HOURS = 36;
export const MAX_ITEMS = 80;
export const MAX_PER_FEED = 40;
export const LEAD_MAX = 700;
export const THIN_LEAD = 120;
export const MAX_PAGE_FETCHES = 25;

// ---------------------------------------------------------------- text helpers
const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'", nbsp: " ", rsquo: "’", lsquo: "‘", rdquo: "”", ldquo: "“", ndash: "–", mdash: "—", hellip: "…" };

export function decodeEntities(s) {
  return s.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (m, e) => {
    if (e[0] === "#") {
      const code = e[1].toLowerCase() === "x" ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
      try {
        return Number.isFinite(code) && code > 0 ? String.fromCodePoint(code) : m;
      } catch {
        return m;
      }
    }
    return ENTITIES[e.toLowerCase()] ?? m;
  });
}

/** HTML (or escaped HTML) to plain text: tags removed, entities decoded, whitespace collapsed. */
export function stripHtml(html) {
  let s = String(html ?? "");
  s = s.replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1");
  // escaped markup (&lt;p&gt;) is common in RSS descriptions: decode once so the tags can be removed
  if (/&lt;\/?[a-z]/i.test(s)) s = decodeEntities(s);
  s = s.replace(/<(script|style)[\s\S]*?<\/\1>/gi, " ");
  s = s.replace(/<(br|\/p|\/div|\/li|\/h\d)\s*\/?>/gi, " ");
  s = s.replace(/<[^>]*>/g, " ");
  s = decodeEntities(s);
  return s.replace(/\s+/g, " ").trim();
}

/** Sentences of a text (not split at "U.S.", "Mr." or an ellipsis). */
export function sentencesOf(text) {
  const t = text.replace(/\s+/g, " ").trim();
  const out = [];
  let start = 0;
  for (let i = 0; i < t.length; i++) {
    const c = t[i];
    if (c !== "." && c !== "!" && c !== "?") continue;
    if (t[i + 1] === "." || t[i - 1] === ".") continue;
    let j = i + 1;
    while (j < t.length && /["'”’)\]]/.test(t[j])) j++;
    if (j < t.length && t[j] !== " ") continue;
    const word = t.slice(start, i).split(" ").pop() ?? "";
    if (c === "." && (/^[A-Z]$/.test(word) || word.includes(".") || /^(Mr|Mrs|Ms|Dr|St|No|Jr|Sr|Vs)$/.test(word))) continue;
    out.push(t.slice(start, j).trim());
    start = j;
  }
  const rest = t.slice(start).trim();
  return { sentences: out, rest };
}

const TAIL_JUNK = /\s*(?:\[(?:…|\.\.\.)\]|(?:…|\.\.\.)|(?:Continue reading|Read more|Read the full (?:story|article))\W*)\s*$/i;

/**
 * A lead that ends cleanly: never cut mid-sentence. Text that is short enough and ends a sentence is kept whole; otherwise
 * the complete sentences that fit in `max` characters are kept, and when not even the first fits (or the feed cut it
 * off before its first full stop) the lead is dropped (""), so the app never reads half a sentence.
 */
export function truncate(s, max = LEAD_MAX) {
  let t = String(s ?? "").replace(/\s+/g, " ").trim();
  let cutOff = false;
  for (let k = 0; k < 3 && TAIL_JUNK.test(t); k++) {
    t = t.replace(TAIL_JUNK, "").trim();
    cutOff = true;
  }
  if (!t) return "";
  const { sentences, rest } = sentencesOf(t);
  if (!cutOff && !rest && t.length <= max) return t;
  // a trailing fragment without an end (a cut-off feed lead, or a lead with no full stop at all)
  if (!cutOff && !sentences.length && t.length <= max) return t; // one short unpunctuated line is complete as given
  const kept = [];
  let len = 0;
  for (const sent of sentences) {
    const next = len + (kept.length ? 1 : 0) + sent.length;
    if (next > max) break;
    kept.push(sent);
    len = next;
  }
  return kept.join(" ");
}

// ---------------------------------------------------------------- tiny RSS / Atom parser
function tagBlocks(xml, tag) {
  const re = new RegExp(`<${tag}(?:\\s[^>]*)?>([\\s\\S]*?)</${tag}>`, "gi");
  const out = [];
  let m;
  while ((m = re.exec(xml))) out.push(m[1]);
  return out;
}

/** Raw inner text of the first <tag>, CDATA unwrapped (entities NOT decoded), or "". */
function rawTag(block, tag) {
  const re = new RegExp(`<${tag}(?:\\s[^>]*)?>([\\s\\S]*?)</${tag}>`, "i");
  const m = block.match(re);
  return m ? m[1].trim() : "";
}

/** Text of a tag as plain text (CDATA or escaped, HTML stripped). */
function textTag(block, tag) {
  const raw = rawTag(block, tag);
  if (!raw) return "";
  return /^<!\[CDATA\[/.test(raw) ? stripHtml(raw) : stripHtml(raw);
}

function linkOf(block) {
  // RSS: <link>url</link>; Atom: <link rel="alternate" href="url"/> (possibly several links)
  const text = rawTag(block, "link").replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1").trim();
  if (text && /^https?:/i.test(text)) return decodeEntities(text);
  const links = [...block.matchAll(/<link\b([^>]*)>/gi)].map((m) => m[1]);
  const pick = links.find((a) => /rel=["']alternate["']/i.test(a)) ?? links.find((a) => !/rel=/i.test(a)) ?? links[0];
  const href = pick && pick.match(/href=["']([^"']+)["']/i);
  if (href) return decodeEntities(href[1]);
  const guid = rawTag(block, "guid");
  return /^https?:/i.test(guid) ? decodeEntities(guid) : "";
}

function firstParagraph(html) {
  const raw = String(html ?? "").replace(/<!\[CDATA\[([\s\S]*?)\]\]>/g, "$1");
  const decoded = /&lt;\/?p/i.test(raw) ? decodeEntities(raw) : raw;
  for (const m of decoded.matchAll(/<p\b[^>]*>([\s\S]*?)<\/p>/gi)) {
    const t = stripHtml(m[1]);
    if (t.length >= 40) return t;
  }
  return "";
}

/** The lead: the feed's description, or the first paragraph of content:encoded / content when that is fuller. */
export function leadOf(block) {
  const desc = textTag(block, "description") || textTag(block, "summary");
  const content = rawTag(block, "content:encoded") || rawTag(block, "content");
  const para = content ? firstParagraph(content) : "";
  const best = desc.length >= THIN_LEAD || !para ? desc : para.length > desc.length ? para : desc;
  return truncate(best);
}

export function parseDate(s) {
  if (!s) return null;
  const t = Date.parse(s.trim());
  return Number.isFinite(t) ? new Date(t) : null;
}

/** Parse an RSS 2.0 or Atom document into raw entries (no filtering). */
export function parseFeed(xml) {
  const blocks = [...tagBlocks(xml, "item"), ...tagBlocks(xml, "entry")];
  const out = [];
  for (const b of blocks) {
    const title = textTag(b, "title");
    if (!title) continue;
    const date = parseDate(textTag(b, "pubDate")) ?? parseDate(textTag(b, "published")) ?? parseDate(textTag(b, "updated")) ?? parseDate(textTag(b, "dc:date"));
    out.push({ title, link: linkOf(b), lead: leadOf(b), published: date ? date.toISOString() : "" });
  }
  return out;
}

// ---------------------------------------------------------------- topics, similarity, dedupe
const INDUSTRY_STRONG = ["semiconductor", "chipmaker", "tsmc", "sk hynix", "hynix", "cathode", "lithium", "rare earth", "rare-earth", "critical mineral", "battery material", "nickel", "cobalt", "gigafactory", "wafer", "foundry", "nvidia", "samsung electronics", "lg energy", "catl", "electric vehicle", "magnet production", "battery maker", "battery plant", "export control", "chip export"];
const INDUSTRY_WEAK = ["chip", "chips", "battery", "batteries", "magnet", "magnets", "ev", "evs", "byd", "posco"];

export function industryScore(title, lead = "") {
  const t = `${title} ${lead}`.toLowerCase();
  const words = new Set(t.split(/[^\p{L}\p{N}]+/u).filter(Boolean));
  return INDUSTRY_STRONG.filter((k) => t.includes(k)).length * 2 + INDUSTRY_WEAK.filter((k) => words.has(k)).length;
}

const STOP = new Set(["the", "and", "for", "with", "from", "that", "this", "has", "are", "was", "will", "after", "over", "into", "says", "say"]);
export function titleTokens(title) {
  return [...new Set(title.toLowerCase().split(/[^\p{L}\p{N}]+/u).filter((w) => w.length >= 3 && !STOP.has(w)))].sort();
}

/** Near-identical titles (same story from another outlet): equal token sets or Jaccard >= 0.6. */
export function similarTitles(a, b) {
  const x = titleTokens(a);
  const y = titleTokens(b);
  if (!x.length || !y.length) return a.trim().toLowerCase() === b.trim().toLowerCase();
  const inter = x.filter((t) => y.includes(t)).length;
  const union = x.length + y.length - inter;
  return inter / union >= 0.6;
}

export const idOf = (link, title) => createHash("sha1").update(link || title).digest("hex").slice(0, 16);

/** Merge entries from all feeds: window, dedupe by link and similar title, newest first, capped. */
export function buildItems(entries, now = new Date(), windowHours = WINDOW_HOURS, cap = MAX_ITEMS) {
  const min = now.getTime() - windowHours * 3600 * 1000;
  const fresh = entries.filter((e) => e.published && Date.parse(e.published) >= min && Date.parse(e.published) <= now.getTime() + 3600 * 1000);
  fresh.sort((a, b) => Date.parse(b.published) - Date.parse(a.published));
  const kept = [];
  for (const e of fresh) {
    const dup = kept.find((k) => k.link && k.link === e.link || similarTitles(k.title, e.title));
    if (dup) {
      // keep the first (newest-first order) but fold in topics, and prefer the fuller lead
      for (const t of e.topics) if (!dup.topics.includes(t)) dup.topics.push(t);
      if (e.lead.length > dup.lead.length) dup.lead = e.lead;
      continue;
    }
    kept.push({ ...e, topics: [...e.topics] });
  }
  return kept.slice(0, cap).map((e) => {
    const topics = [...e.topics];
    if (industryScore(e.title, e.lead) >= 2 && !topics.includes("industry-tech")) topics.push("industry-tech");
    return { id: idOf(e.link, e.title), source: e.source, title: e.title, summary: e.lead, link: e.link, published: e.published, topics };
  });
}

// ---------------------------------------------------------------- article page lead (only when a feed gives almost nothing)
/** og:description, else the first substantial <p> inside <article>/<main>. One paragraph, max LEAD_MAX characters. */
export function leadFromPage(html) {
  const meta = html.match(/<meta[^>]+(?:property|name)=["']og:description["'][^>]*>/i) || html.match(/<meta[^>]+content=["'][^"']*["'][^>]+(?:property|name)=["']og:description["'][^>]*>/i);
  if (meta) {
    const c = meta[0].match(/content=["']([^"']*)["']/i);
    const t = c ? stripHtml(c[1]) : "";
    if (t.length >= 40) return truncate(t);
  }
  const region = html.match(/<article\b[\s\S]*?<\/article>/i) || html.match(/<main\b[\s\S]*?<\/main>/i);
  if (region) {
    for (const m of region[0].matchAll(/<p\b[^>]*>([\s\S]*?)<\/p>/gi)) {
      const t = stripHtml(m[1]);
      if (t.length >= 80) return truncate(t);
    }
  }
  return "";
}

async function fetchText(url, ms) {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), ms);
  try {
    const r = await fetch(url, { headers: { "User-Agent": UA, Accept: "application/rss+xml, application/atom+xml, application/xml, text/xml, text/html;q=0.8, */*;q=0.5" }, signal: ctl.signal, redirect: "follow" });
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    return await r.text();
  } finally {
    clearTimeout(timer);
  }
}

// ---------------------------------------------------------------- main
export async function run({ sourcesPath, outPath, now = new Date(), log = (m) => console.error(m), fetcher = fetchText } = {}) {
  const cfg = JSON.parse(await readFile(sourcesPath, "utf8"));
  const entries = [];
  let ok = 0;
  await Promise.all(
    cfg.sources.map(async (src) => {
      try {
        const xml = await fetcher(src.url, 15000);
        const parsed = parseFeed(xml).slice(0, MAX_PER_FEED);
        if (!parsed.length) throw new Error("no items");
        for (const p of parsed) entries.push({ ...p, source: src.name, topics: [...(src.topics ?? [])] });
        ok++;
        log(`ok   ${src.name} ${src.url} (${parsed.length})`);
      } catch (e) {
        log(`skip ${src.name} ${src.url}: ${e.message}`);
      }
    }),
  );
  // thin leads: look at the article page, newest first, sequentially, a limited number per run
  const recent = entries.filter((e) => e.published && Date.parse(e.published) >= now.getTime() - WINDOW_HOURS * 3600 * 1000 && e.link && e.lead.length < THIN_LEAD);
  recent.sort((a, b) => Date.parse(b.published) - Date.parse(a.published));
  let pages = 0;
  for (const e of recent) {
    if (pages >= MAX_PAGE_FETCHES) break;
    pages++;
    try {
      const lead = leadFromPage(await fetcher(e.link, 8000));
      if (lead.length > e.lead.length) e.lead = lead;
    } catch {
      // paywall / timeout: skip silently
    }
  }
  const items = buildItems(entries, now);
  const out = { generated_at: now.toISOString(), items };
  await mkdir(dirname(outPath), { recursive: true });
  await writeFile(outPath, JSON.stringify(out, null, 1) + "\n");
  log(`wrote ${items.length} items from ${ok}/${cfg.sources.length} feeds, ${pages} page lookups -> ${outPath}`);
  return out;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const here = dirname(fileURLToPath(import.meta.url));
  const i = process.argv.indexOf("--out");
  const outPath = resolve(i > 0 ? process.argv[i + 1] : resolve(here, "../../app/public/news.json"));
  try {
    await run({ sourcesPath: resolve(here, "sources.json"), outPath });
  } catch (e) {
    console.error(`news fetch failed: ${e.message}`);
    await mkdir(dirname(outPath), { recursive: true });
    await writeFile(outPath, JSON.stringify({ generated_at: new Date().toISOString(), items: [] }) + "\n");
  }
}
