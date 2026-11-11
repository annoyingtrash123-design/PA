// Whole-app tests: opens the BUILT app (dist/) in a phone-sized browser and uses it the way a person would.
// Google, the weather and the news are stand-ins, so the tests are repeatable and use no real requests.
//   npm run build:web && node e2e/run.cjs
const { chromium, devices } = require("playwright");
const { spawn } = require("node:child_process");
const path = require("node:path");

const PORT = 4179;
const BASE = (process.env.BASE_PATH || "/").replace(/\/?$/, "/");
const URL0 = `http://localhost:${PORT}${BASE}`;
const SHOTS = process.env.E2E_SHOTS || "";

const NEWS = {
  generated_at: "2026-11-09T18:45:00Z",
  items: [
    { id: "a1", source: "ABC News", title: "Sydney ferries return to full timetable after repairs", summary: "All harbour routes are running again from Monday. Transport officials said the repairs finished a week early.", link: "https://example.org/a1", published: "2026-11-09T17:00:00Z", topics: ["australia"] },
    { id: "b1", source: "BBC News", title: "Rare orchid flowers for first time in a decade", summary: "Botanists at the garden said the plant bloomed overnight. Visitors can see it for about a week.", link: "https://example.org/b1", published: "2026-11-09T16:00:00Z", topics: ["life", "world"] },
    { id: "g1", source: "The Guardian", title: "Regional rail upgrade wins final approval", summary: "Work is due to start next year.", link: "https://example.org/g1", published: "2026-11-09T15:00:00Z", topics: ["australia"] },
  ],
};
const DAY_TEXT = "Right, so, your day. " + "It is a calm and steady one today, with a couple of small things to look after along the way. ".repeat(6) + "\n\nI'd start with the little jobs and leave the afternoon open.\n\nOh, and tomorrow looks quiet so far.\n\nHave a lovely Tuesday.";
const sse = (text) => text.match(/[\s\S]{1,60}/g).map((c) => `data: ${JSON.stringify({ candidates: [{ content: { parts: [{ text: c }] } }] })}\n\n`).join("");
const plain = (text) => JSON.stringify({ candidates: [{ content: { parts: [{ text }] } }] });

/** A stand-in for Gemini: answers each of the three small requests; `fail` makes one kind fail. */
function gemini(calls, fail = {}) {
  return async (route) => {
    const req = route.request();
    const body = JSON.parse(req.postData() || "{}");
    const system = body.systemInstruction?.parts?.[0]?.text || "";
    const kind = system.includes("three short opening parts") ? "opening" : system.includes("NEWS:") ? "news" : "day";
    calls.push(kind);
    // a dropped connection for the first N requests (the phone has just woken up)
    if (fail.drop > 0) {
      fail.drop--;
      return route.abort("internetdisconnected");
    }
    if (fail[kind]) return route.fulfill({ status: fail[kind], contentType: "application/json", body: JSON.stringify({ error: { message: "nope" } }) });
    const stream = req.url().includes("streamGenerateContent");
    let text;
    if (kind === "opening") text = JSON.stringify({ greeting: "Morning, you. Kettle's on in spirit.", weather_comment: null, affirmation: "A gentle one today, so take the doctor's visit at your own pace." });
    else if (kind === "news") {
      const user = body.contents?.[0]?.parts?.[0]?.text || "";
      const titles = [...user.matchAll(/Headline, quote exactly: (.*)\n/g)].map((m) => m[1].trim());
      text = JSON.stringify({ news: titles.map((t, i) => `${i ? "And the other one" : "First"}, the headline is: "${t}". So basically, that is the whole story told properly, with what happened and what comes next.`).join(" ") });
    } else text = DAY_TEXT;
    return route.fulfill({ status: 200, contentType: stream ? "text/event-stream" : "application/json", body: stream ? sse(text) : plain(text) });
  };
}

async function newPage(browser, { when, key, geminiHandler } = {}) {
  const ctx = await browser.newContext({ ...devices["Pixel 7"], timezoneId: "Australia/Sydney", locale: "en-AU", serviceWorkers: "block" });
  const page = await ctx.newPage();
  const problems = [];
  page.on("console", (m) => {
    const t = m.text();
    if (m.type() === "error" && !/Failed to load resource|net::ERR/.test(t)) problems.push(t);
    if (/Content Security Policy/i.test(t)) problems.push(`CSP: ${t}`);
  });
  page.on("pageerror", (e) => problems.push(`page error: ${e.message}`));
  await ctx.route("**/news.json*", (r) => r.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(NEWS) }));
  await ctx.route("https://api.open-meteo.com/**", (r) => r.fulfill({ status: 500, body: "{}" }));
  await ctx.route("https://date.nager.at/**", (r) => r.fulfill({ status: 200, contentType: "application/json", body: "[]" }));
  await ctx.route("https://generativelanguage.googleapis.com/**", geminiHandler || ((r) => r.fulfill({ status: 500, body: "{}" })));
  if (when) await page.clock.setFixedTime(new Date(when));
  if (key) await page.addInitScript((k) => localStorage.setItem("pa-gemini-key", k), key);
  await page.goto(URL0);
  await page.waitForSelector("#tabs a");
  return { ctx, page, problems };
}

const go = async (page, hash) => {
  await page.click(`#tabs a[href="${hash}"]`);
  await page.waitForTimeout(150);
};
const text = (page) => page.evaluate(() => document.querySelector("#view").innerText);
const assert = (cond, msg) => {
  if (!cond) throw new Error(msg);
};
async function addTask(page, title, opts = {}) {
  await page.click('button:has-text("New task")');
  await page.fill(".sheet-overlay input.input", title);
  if (opts.date) await page.fill('.sheet-overlay input[type="date"]', opts.date);
  if (opts.check) await opts.check();
  await page.click('.sheet-overlay button:has-text("Save")');
  await page.waitForSelector(".sheet-overlay", { state: "detached" });
}

const MORNING = "2026-11-10T07:30:00+11:00";
const tests = {
  "a fresh install opens, every tab fits a phone screen, and nothing errors": async (browser) => {
    const { ctx, page, problems } = await newPage(browser, { when: MORNING });
    assert((await text(page)).includes("Built-in summary — add a Gemini key in Settings"), "the built-in label with its reason is shown at the top of the card");
    for (const hash of ["#/tasks", "#/goals", "#/gym", "#/settings", "#/"]) {
      await go(page, hash);
      const over = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      assert(over <= 1, `${hash} is wider than the screen by ${over}px`);
    }
    assert(!problems.length, `console problems: ${problems.join(" | ")}`);
    await ctx.close();
  },

  "tasks typed as commands are filed, timed and spoken sensibly": async (browser) => {
    const { ctx, page, problems } = await newPage(browser, { when: MORNING });
    await go(page, "#/tasks");
    await addTask(page, "go to the doctor", {
      check: async () => {
        const r = await page.innerText('[data-testid="task-reading"]');
        assert(r.includes("a doctor's appointment") && r.includes("Appointments") && r.includes("plus getting there"), `Miranda's reading of the task: ${r}`);
      },
    });
    assert((await page.innerText(".toast")).includes("Filed under Appointments"), "filed under Appointments");
    await addTask(page, "Haircut");
    await go(page, "#/");
    const home = await text(page);
    assert(home.includes("a doctor's appointment"), `Home says it naturally:\n${home}`);
    assert(!home.includes("go to the doctor"), "the typed command is never read out as typed");
    assert(!/haircut[^.]*ten minutes|ten minutes[^.]*haircut/i.test(home), "a haircut is never a ten-minute job");
    assert(!problems.length, `console problems: ${problems.join(" | ")}`);
    await ctx.close();
  },

  "press and hold moves a task to tomorrow; a tap does not": async (browser) => {
    const { ctx, page } = await newPage(browser, { when: MORNING });
    await go(page, "#/tasks");
    await addTask(page, "Sort the photo boxes");
    const today = page.locator(".card.today");
    const arrow = today.locator(".hold-btn.icon-only").first();
    await arrow.click();
    assert((await page.innerText(".toast")).includes("Press and hold"), "a tap only explains");
    assert((await today.innerText()).includes("Sort the photo boxes"), "the task stayed put after a tap");
    const box = await arrow.boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.waitForTimeout(1000);
    await page.mouse.up();
    await page.waitForTimeout(200);
    assert(!(await page.locator(".card.today").innerText()).includes("Sort the photo boxes"), "the task left today after a hold");
    assert((await text(page)).includes("Sort the photo boxes"), "and it is still in the week (tomorrow)");
    await ctx.close();
  },

  "with a key, the summary is written by three small requests": async (browser) => {
    const calls = [];
    const { ctx, page, problems } = await newPage(browser, { when: MORNING, key: "test-key", geminiHandler: gemini(calls) });
    await page.waitForFunction(() => document.querySelector('[data-testid="source-label"]')?.textContent?.startsWith("Written by"), null, { timeout: 15000 });
    const home = await text(page);
    assert((await page.innerText('[data-testid="source-label"]')) === "Written by Miranda's AI", "plain AI label");
    assert(home.includes("Kettle's on in spirit"), "the AI hello is shown");
    assert(home.includes("It is a calm and steady one today"), "the AI day text is shown");
    assert(home.includes('"Sydney ferries return to full timetable after repairs"'), "the AI news quotes the headline");
    assert(calls.slice().sort().join(",") === "day,news,opening", `exactly three requests, got: ${calls.join(",")}`);
    await go(page, "#/settings");
    if (!/AI writing today/.test(await text(page))) await page.click('button.collapse-head:has-text("AI (Gemini)")');
    const usage = (await text(page)).match(/AI writing today: \d+ of \d+/)?.[0];
    assert(usage === "AI writing today: 3 of 12", `three requests counted, Settings says: ${usage}; calls: ${calls.join(",")}`);
    // coming back does not write it again
    await go(page, "#/");
    await page.waitForTimeout(500);
    assert(calls.length === 3, `no extra requests on return, got ${calls.length}`);
    assert(!problems.length, `console problems: ${problems.join(" | ")}`);
    await ctx.close();
  },

  "one part failing does not lose the others": async (browser) => {
    const calls = [];
    const { ctx, page } = await newPage(browser, { when: MORNING, key: "test-key", geminiHandler: gemini(calls, { news: 400, opening: 400 }) });
    await page.waitForFunction(() => document.querySelector('[data-testid="source-label"]')?.textContent?.startsWith("Written by"), null, { timeout: 15000 });
    const label = await page.innerText('[data-testid="source-label"]');
    assert(label.includes("the news part is the built-in version"), `label names the news fallback: ${label}`);
    const home = await text(page);
    assert(home.includes("It is a calm and steady one today"), `the day is still AI-written; calls: ${calls.join(",")}\n${home.slice(0, 900)}`);
    assert(home.includes("Sydney ferries return to full timetable after repairs"), "the built-in news is shown instead");
    await ctx.close();
  },

  "a dropped connection is tried again and doesn't use up the allowance": async (browser) => {
    const calls = [];
    const { ctx, page } = await newPage(browser, { when: MORNING, key: "test-key", geminiHandler: gemini(calls, { drop: 3 }) });
    await page.waitForFunction(() => document.querySelector('[data-testid="source-label"]')?.textContent?.startsWith("Written by"), null, { timeout: 20000 });
    assert((await text(page)).includes("It is a calm and steady one today"), "the AI text arrives after the retry");
    await go(page, "#/settings");
    if (!/AI writing today/.test(await text(page))) await page.click('button.collapse-head:has-text("AI (Gemini)")');
    const usage = (await text(page)).match(/AI writing today: \d+ of \d+/)?.[0];
    assert(usage === "AI writing today: 3 of 12", `dropped requests aren't counted, Settings says: ${usage}`);
    await page.click('summary:has-text("Recent AI requests")');
    const log = await text(page);
    assert(/Couldn't reach Gemini/.test(log) && /morning day · [\w.-]+ · [\d.]+s · ok/.test(log), `the log shows the drop and the success:\n${log.slice(-700)}`);
    await ctx.close();
  },

  "when Google fails, the built-in summary shows with the reason and a Try again button, and doesn't keep retrying": async (browser) => {
    const calls = [];
    const { ctx, page } = await newPage(browser, { when: MORNING, key: "test-key", geminiHandler: gemini(calls, { day: 400, news: 400, opening: 400 }) });
    await page.waitForFunction(() => /Built-in summary — Google replied with an error/.test(document.querySelector('[data-testid="source-label"]')?.textContent || ""), null, { timeout: 15000 });
    assert(await page.locator('button:has-text("Try again")').count(), "Try again is offered");
    const n = calls.length;
    for (let i = 0; i < 3; i++) {
      await page.reload();
      await page.waitForSelector('[data-testid="source-label"]');
      await page.waitForTimeout(400);
    }
    assert(calls.length <= n + 3, `reopening the app makes at most one more automatic attempt (was ${n}, now ${calls.length})`);
    await ctx.close();
  },

  "the evening asks once about tomorrow's unplaced tasks": async (browser) => {
    const { ctx, page } = await newPage(browser, { when: "2026-11-10T21:30:00+11:00" });
    await go(page, "#/tasks");
    await addTask(page, "Haircut", { date: "2026-11-11" });
    await go(page, "#/");
    const home = await text(page);
    assert(/Tomorrow check-in/i.test(home) && home.includes("Haircut"), `the check-in asks about tomorrow:\n${home.slice(0, 400)}`);
    await page.click('button:has-text("After work")');
    await page.click('button:has-text("All set")');
    await page.waitForTimeout(300);
    assert(!/Tomorrow check-in/i.test(await text(page)), "it goes away once answered");
    await go(page, "#/tasks");
    assert(/Haircut\s*\n\s*Evening/.test(await text(page)), "the haircut now sits in the evening");
    await ctx.close();
  },
};

(async () => {
  const preview = spawn("npx", ["vite", "preview", "--port", String(PORT), "--strictPort"], { cwd: path.join(__dirname, ".."), stdio: "pipe", env: process.env, detached: true });
  // stop the server and its children when done (a leftover one would answer the next run with an old build)
  const stopPreview = () => {
    try {
      process.kill(-preview.pid);
    } catch {
      preview.kill();
    }
  };
  let up = false;
  for (let i = 0; i < 60 && !up; i++) {
    await new Promise((r) => setTimeout(r, 250));
    up = await fetch(URL0).then((r) => r.ok).catch(() => false);
  }
  if (!up) {
    console.error("preview server did not start (run `npm run build:web` first, and check nothing else is using the port)");
    stopPreview();
    process.exit(1);
  }
  const browser = await chromium.launch();
  let failed = 0;
  for (const [name, fn] of Object.entries(tests)) {
    try {
      await fn(browser);
      console.log(`  ok    ${name}`);
    } catch (e) {
      failed++;
      console.log(`  FAIL  ${name}\n        ${String(e.message).split("\n").join("\n        ")}`);
      if (SHOTS) for (const c of browser.contexts()) for (const p of c.pages()) await p.screenshot({ path: path.join(SHOTS, `${name.replace(/\W+/g, "-").slice(0, 60)}.png`), fullPage: true }).catch(() => {});
    }
    for (const c of browser.contexts()) await c.close().catch(() => {});
  }
  await browser.close();
  stopPreview();
  console.log(failed ? `\n${failed} of ${Object.keys(tests).length} failed` : `\nall ${Object.keys(tests).length} passed`);
  process.exit(failed ? 1 : 0);
})();
