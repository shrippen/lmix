// Stills of the built-in title templates (GFX.19): each template played in
// Chromium with its field labels as text, transparent, 480×270, written to
// workspace/lmix-core/titles/lmix/<name>/thumbnail.png.
//   lmix-core --engine none --listen 127.0.0.1:8896 &
//   node scripts/make-title-thumbnails.cjs http://127.0.0.1:8896
"use strict";
const path = require("path");
const { chromium } = require("playwright");

const base = process.argv[2];
const out = path.join(__dirname, "..", "workspace", "lmix-core", "titles");

(async () => {
  const templates = (await (await fetch(base + "/api/templates")).json()).filter((t) => t.builtin);
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: 0.25 });
  for (const t of templates) {
    await page.goto(base + "/health"); // same origin as the frame
    await page.setContent(`<body style="margin:0;background:transparent">
      <iframe id="t" src="${base + t.url}" style="border:0;width:1920px;height:1080px;background:transparent"></iframe></body>`);
    const frame = await (await page.waitForSelector("#t")).contentFrame();
    await frame.waitForLoadState();
    await page.waitForTimeout(500);
    // Labels stand in for values; choices and numbers take their default.
    const data = Object.fromEntries(t.fields.map((f) => [f.id, f.default ?? f.label]));
    await frame.evaluate((d) => { update(JSON.stringify(d)); play(); }, data);
    if (t.steps > 0 && t.steps < 100) await frame.evaluate(() => next());
    await page.waitForTimeout(1500);
    const file = path.join(out, t.id, "thumbnail.png");
    await page.screenshot({ path: file, omitBackground: true });
    console.log("wrote", file);
  }
  await browser.close();
})().catch((e) => { console.error(e); process.exit(1); });
