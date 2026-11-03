import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { buildItems, industryScore, leadFromPage, parseFeed, run, similarTitles, stripHtml } from "./fetch-news.mjs";

const RSS = `<?xml version="1.0"?>
<rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/"><channel><title>Test</title>
<item><title><![CDATA[Ferries hit by storm &amp; wind]]></title><link>https://ex.com/a</link>
<description><![CDATA[<p>Several <b>ferry</b> services were suspended on Sunday.</p><p>More below.</p>]]></description><pubDate>Sun, 01 Nov 2026 17:30:00 +0000</pubDate></item>
<item><title>Budget &quot;update&quot; due</title><link>https://ex.com/b</link>
<description>&lt;p&gt;The treasurer will speak &amp;amp; answer questions.&lt;/p&gt;</description><dc:date>2026-11-01T12:00:00Z</dc:date></item>
<item><title>Thin one</title><link>https://ex.com/c</link><description>Short.</description><content:encoded><![CDATA[<p>This is the first paragraph of the full content, long enough to count as a real lead for the story.</p><p>Second.</p>]]></content:encoded><pubDate>Sun, 01 Nov 2026 10:00:00 GMT</pubDate></item>
</channel></rss>`;

const ATOM = `<feed xmlns="http://www.w3.org/2005/Atom"><title>Conv</title>
<entry><title type="html">Why telescopes matter</title><link rel="alternate" type="text/html" href="https://conv.com/x?a=1&amp;b=2"/><link rel="self" href="https://conv.com/self"/>
<published>2026-11-01T09:00:00Z</published><updated>2026-11-01T10:00:00Z</updated><summary type="html">&lt;p&gt;A new telescope will change what we know.&lt;/p&gt;</summary></entry>
<entry><title>Second</title><link href="https://conv.com/y"/><updated>2026-11-01T08:00:00+11:00</updated><content type="html"><![CDATA[<p>Body of the second entry which is reasonably long for a paragraph here.</p>]]></content></entry>
</feed>`;

test("parses RSS 2.0 with CDATA, escaped HTML and content:encoded", () => {
  const items = parseFeed(RSS);
  assert.equal(items.length, 3);
  assert.equal(items[0].title, "Ferries hit by storm & wind");
  assert.equal(items[0].link, "https://ex.com/a");
  assert.equal(items[0].lead, "Several ferry services were suspended on Sunday. More below.");
  assert.equal(items[0].published, "2026-11-01T17:30:00.000Z");
  assert.equal(items[1].title, 'Budget "update" due');
  assert.equal(items[1].lead, "The treasurer will speak & answer questions.");
  assert.equal(items[1].published, "2026-11-01T12:00:00.000Z");
  assert.match(items[2].lead, /^This is the first paragraph of the full content/);
  assert.ok(!items[2].lead.includes("Second"));
});

test("parses Atom (alternate link, published/updated, summary and content)", () => {
  const items = parseFeed(ATOM);
  assert.equal(items.length, 2);
  assert.equal(items[0].link, "https://conv.com/x?a=1&b=2");
  assert.equal(items[0].lead, "A new telescope will change what we know.");
  assert.equal(items[0].published, "2026-11-01T09:00:00.000Z");
  assert.equal(items[1].link, "https://conv.com/y");
  assert.equal(items[1].published, "2026-10-31T21:00:00.000Z");
  assert.match(items[1].lead, /^Body of the second entry/);
});

test("stripHtml removes tags and decodes entities", () => {
  assert.equal(stripHtml("<p>Hello&nbsp;<em>there</em> &#8217;s &#x41;</p><script>x()</script>"), "Hello there ’s A");
});

test("lead is capped at about 700 characters on a boundary", () => {
  const long = `<item><title>T</title><description>${"Word ".repeat(300)}</description><pubDate>Sun, 01 Nov 2026 17:30:00 +0000</pubDate></item>`;
  const [it] = parseFeed(long);
  assert.ok(it.lead.length <= 702, it.lead.length);
});

test("titles: similar detection", () => {
  assert.ok(similarTitles("Ceasefire talks resume in Geneva", "Talks resume in Geneva on ceasefire"));
  assert.ok(!similarTitles("Ceasefire talks resume in Geneva", "Sydney ferry services disrupted"));
});

const NOW = new Date("2026-11-02T06:00:00Z");
const e = (title, hoursAgo, extra = {}) => ({ title, link: `https://ex.com/${encodeURIComponent(title)}`, lead: "A lead.", source: "ABC News", topics: ["australia"], published: new Date(NOW - hoursAgo * 3600e3).toISOString(), ...extra });

test("keeps only the last 36 hours, newest first, de-duplicated, capped", () => {
  const items = buildItems(
    [e("Old story from days ago", 40), e("Harbour Bridge lane closed overnight", 2), e("Ceasefire talks resume in Geneva", 5, { source: "BBC News", topics: ["world"] }), e("Talks resume in Geneva on ceasefire", 4, { source: "Al Jazeera", topics: ["world"] }), e("Parliament passes housing bill", 1), e("Undated", 1, { published: "" })],
    NOW,
  );
  assert.deepEqual(items.map((i) => i.title), ["Parliament passes housing bill", "Harbour Bridge lane closed overnight", "Talks resume in Geneva on ceasefire"]);
  assert.ok(items.every((i) => /^[0-9a-f]{16}$/.test(i.id)));
  const many = buildItems(Array.from({ length: 200 }, (_, k) => e(`Distinct headline number ${k} zebra${k}x alpha${k}y`, 1 + (k % 30))), NOW);
  assert.equal(many.length, 80);
  for (let k = 1; k < many.length; k++) assert.ok(many[k - 1].published >= many[k].published);
});

test("adds the industry-tech topic from keywords", () => {
  assert.ok(industryScore("Samsung lifts chip output", "semiconductor makers") >= 2);
  const [it] = buildItems([e("Japan funds rare earth magnet production", 2)], NOW);
  assert.ok(it.topics.includes("industry-tech"));
  const [plain] = buildItems([e("Council approves bike lane", 2)], NOW);
  assert.ok(!plain.topics.includes("industry-tech"));
});

test("page lead: og:description, else first substantial paragraph in article", () => {
  assert.equal(leadFromPage('<meta property="og:description" content="A good description that is long enough to use as a lead for the story.">'), "A good description that is long enough to use as a lead for the story.");
  assert.match(leadFromPage("<article><p>Short.</p><p>This paragraph is long enough to be substantial and should be taken as the lead of the article page.</p></article>"), /^This paragraph is long enough/);
  assert.equal(leadFromPage("<p>nothing</p>"), "");
});

test("run(): skips failing feeds, writes news.json, tolerates total failure", async () => {
  const dir = await mkdtemp(join(tmpdir(), "news-"));
  const sourcesPath = join(dir, "sources.json");
  await writeFile(sourcesPath, JSON.stringify({ sources: [{ name: "Good", url: "https://good/rss", topics: ["world"] }, { name: "Bad", url: "https://bad/rss", topics: ["world"] }] }));
  const now = new Date("2026-11-02T00:00:00Z");
  const xml = `<rss><channel><item><title>Only good story</title><link>https://good/1</link><description>${"Lead sentence. ".repeat(12)}</description><pubDate>Sun, 01 Nov 2026 20:00:00 +0000</pubDate></item></channel></rss>`;
  const out = join(dir, "out", "news.json");
  const logs = [];
  const res = await run({ sourcesPath, outPath: out, now, log: (m) => logs.push(m), fetcher: async (url) => { if (url.includes("bad")) throw new Error("HTTP 403"); return xml; } });
  assert.equal(res.items.length, 1);
  assert.equal(JSON.parse(await readFile(out, "utf8")).items[0].source, "Good");
  assert.ok(logs.some((l) => l.startsWith("skip Bad")));
  const none = await run({ sourcesPath, outPath: out, now, log: () => {}, fetcher: async () => { throw new Error("offline"); } });
  assert.deepEqual(none.items, []);
  assert.equal(none.generated_at, now.toISOString());
});
