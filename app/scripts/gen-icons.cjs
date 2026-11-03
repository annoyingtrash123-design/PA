// Renders scripts/icon.svg to the PNG icons in public/ using Playwright's Chromium (dev-only; not a build step).
// Usage: NODE_PATH=$(npm root -g) node scripts/gen-icons.cjs
const { chromium } = require("playwright");
const fs = require("fs");
const path = require("path");

const svg = fs.readFileSync(path.join(__dirname, "icon.svg"), "utf8");
const out = path.join(__dirname, "..", "public");
const BG = "#B8A6E3"; // lavender, matches the icon background so maskable padding blends in

async function render(browser, size, scale, file) {
  const page = await browser.newPage({ viewport: { width: size, height: size } });
  const inner = Math.round(size * scale);
  const pad = Math.round((size - inner) / 2);
  await page.setContent(`<body style="margin:0;background:${BG}"><div style="position:absolute;left:${pad}px;top:${pad}px;width:${inner}px;height:${inner}px">${svg.replace(/width="512" height="512"/, 'width="100%" height="100%"')}</div></body>`);
  await page.screenshot({ path: path.join(out, file) });
  await page.close();
}

(async () => {
  const browser = await chromium.launch();
  await render(browser, 192, 1, "icon-192.png");
  await render(browser, 512, 1, "icon-512.png");
  await render(browser, 512, 0.72, "icon-maskable-512.png");
  await render(browser, 180, 1, "apple-touch-icon.png");
  await render(browser, 64, 1, "icon.png");
  await browser.close();
})();
