// Screenshots of every screen at 390x844 into dev-screenshots/ (against a running dev/preview server).
// Usage: BASE=http://localhost:5173/ NODE_PATH=$(npm root -g) node scripts/screenshots.cjs
// External hosts (Open-Meteo, Nager.Date, Gemini) are stubbed so the weather card and AI text paths show too.
const { chromium } = require("playwright");
const path = require("path");
const BASE = process.env.BASE || "http://localhost:5173/";
const OUT = path.join(__dirname, "..", "dev-screenshots");
const withKey = process.env.NO_KEY ? false : true;

const iso = (d) => d.toISOString().slice(0, 10);
const forecast = () => {
  const time = [], code = [], hi = [], lo = [], pp = [], uv = [];
  for (let i = 0; i < 16; i++) { const d = new Date(Date.now() + (i - 1) * 86400000); time.push(iso(d)); code.push(i % 3 === 0 ? 61 : 2); hi.push(14 + (i % 5)); lo.push(8 + (i % 3)); pp.push(i % 3 === 0 ? 80 : 10); uv.push(1.5 + (i % 4)); }
  return { daily: { time, weather_code: code, temperature_2m_max: hi, temperature_2m_min: lo, precipitation_probability_max: pp, uv_index_max: uv } };
};

(async () => {
  const browser = await chromium.launch();
  const ctx = await browser.newContext({ viewport: { width: 390, height: 844 }, deviceScaleFactor: 1, permissions: [] });
  await ctx.route(/open-meteo\.com/, (r) => r.fulfill({ json: forecast() }));
  await ctx.route(/date\.nager\.at/, (r) => r.fulfill({ json: [{ date: iso(new Date(Date.now() + 2 * 86400000)), localName: "Test Holiday", name: "Test Holiday", global: true, counties: null }] }));
  await ctx.route(/generativelanguage\.googleapis\.com/, async (r) => {
    const body = JSON.parse(r.request().postData() || "{}");
    const json = body.generationConfig?.responseMimeType === "application/json";
    const text = json ? JSON.stringify({ weather_comment: "A proper rainy one, 14 at most. Cosy if you're home, and nice to listen to from your desk.", overview: "A steady day." })
      : "Good morning! It's a gentle rainy one, so a good day to settle in.\n\nLooking back at yesterday, you watered the plants a bit late, but they're fine, and it counted.\n\nIf I were ordering the day, I'd do the morning routine, stand-up, then groceries on the way home. Have a lovely day!";
    await r.fulfill({ json: { candidates: [{ content: { parts: [{ text }] } }] } });
  });
  const page = await ctx.newPage();
  page.on("pageerror", (e) => console.log("PAGEERROR", e.message));
  page.on("console", (m) => m.type() === "error" && console.log("CONSOLE", m.text()));
  if (withKey) await page.addInitScript(() => localStorage.setItem("pa-gemini-key", "test-key"));
  const shot = async (name, full = false) => { await page.waitForTimeout(400); await page.screenshot({ path: path.join(OUT, `${name}.png`), fullPage: full }); };

  await page.goto(BASE + "#/settings");
  await page.getByRole("button", { name: "Load sample data" }).click();
  await page.getByRole("button", { name: "Load it" }).click();
  await page.waitForTimeout(300);
  await shot("settings");
  await shot("settings-full", true);
  await page.getByText("What Miranda knows about you").scrollIntoViewIfNeeded();
  await page.evaluate(() => window.scrollBy(0, -10));
  await shot("settings-memory");

  await page.goto(BASE + "#/");
  await page.waitForSelector(".summary p");
  await shot("home");
  await shot("home-full", true);

  await page.goto(BASE + "#/tasks");
  await page.waitForSelector(".occ");
  await shot("tasks");
  await shot("tasks-full", true);
  await page.locator(".occ-main").first().click();
  await shot("task-dialog");
  await page.keyboard.press("Escape");
  await page.locator(".sheet-overlay").click({ position: { x: 5, y: 5 } }).catch(() => {});
  await page.goto(BASE + "#/tasks");
  await page.getByRole("button", { name: "Add task" }).first().click();
  await shot("task-dialog-new");

  await page.goto(BASE + "#/goals");
  await page.getByRole("button", { name: /Achieved/ }).click({ timeout: 2000 }).catch(() => {});
  await shot("goals");
  await page.locator(".goal").first().click();
  await shot("goal-dialog");

  await page.goto(BASE + "#/gym");
  await page.waitForSelector(".card");
  await shot("gym");
  await shot("gym-full", true);
  await page.getByRole("button", { name: /(Start|Continue) workout/ }).click();
  await shot("workout");
  await shot("workout-full", true);
  await page.getByRole("button", { name: /Back \(keeps/ }).click();
  for (const seg of ["Rotation", "Templates", "Progress"]) { await page.getByRole("tab", { name: seg }).click(); await shot(`gym-${seg.toLowerCase()}`); }
  await shot("gym-templates-full", true);
  await page.getByRole("tab", { name: "Templates" }).click();
  await page.locator("button.card.goal", { hasText: "Pull 1" }).click();
  await shot("template-edit");
  await page.getByRole("button", { name: "Cancel" }).click();
  await page.goto(BASE + "#/gym");
  // finish Push 2, Pull 2 and Legs + Abs 2 (nothing ticked) so that Cardio is next
  await page.getByRole("tab", { name: "Today" }).click();
  for (let i = 0; i < 3; i++) {
    await page.getByRole("button", { name: /(Start|Continue) workout/ }).click();
    await page.getByRole("button", { name: "Finish workout" }).click();
    await page.getByRole("button", { name: "Yay" }).click();
  }
  await page.waitForSelector("text=Next workout: Cardio");
  await shot("gym-cardio");
  await page.getByRole("button", { name: /(Start|Continue) workout/ }).click();
  await page.getByRole("button", { name: "Swimming" }).click();
  await shot("workout-cardio");
  // assisted progress chart
  await page.getByRole("button", { name: /Back \(keeps/ }).click();
  await page.getByRole("tab", { name: "Progress" }).click();
  await page.locator("select").selectOption({ label: "Counterweighted pull-up, wide grip" });
  await shot("gym-progress-assisted");

  await page.goto(BASE + "#/review");
  await page.waitForSelector(".summary p");
  await shot("review");
  await shot("review-full", true);
  await browser.close();
})();
