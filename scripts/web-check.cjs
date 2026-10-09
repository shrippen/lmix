// Drives lmix-core's web pages in Chromium (scripts/test-web.sh).
//   node scripts/web-check.cjs http://127.0.0.1:8897
"use strict";
const { chromium } = require("playwright");

const base = process.argv[2];
const api = (path) => fetch(base + path).then((r) => r.json());
const fail = (message) => { console.log("FAIL: " + message); process.exit(1); };
const pass = (message) => console.log("ok   " + message);

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1200, height: 800 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => { if (m.type() === "error" && !/404|503/.test(m.text())) errors.push(m.text()); });

  await page.goto(base + "/");
  await page.waitForSelector("header .state.online", { timeout: 5000 });
  pass("index: event stream online");

  await page.goto(base + "/mixer");
  await page.waitForFunction(() => document.querySelectorAll("#inputs .tile").length === 2, null, { timeout: 5000 });
  await page.click('#inputs .tile[data-input="2"]');
  await page.waitForSelector('#inputs .tile.preview[data-input="2"]', { timeout: 5000 });
  const state = await api("/api/state");
  if (state.preview !== state.inputs[1].id) fail("click on tile 2 did not preview it");
  pass("mixer: click puts an input into preview");

  await page.click("#takes .btn.primary");
  await page.waitForSelector('#inputs .tile.program[data-input="2"]', { timeout: 5000 });
  pass("mixer: Cut takes it to program");

  await page.dblclick('#inputs .tile[data-input="1"]');
  await page.waitForSelector('#inputs .tile.program[data-input="1"]', { timeout: 5000 });
  pass("mixer: double click cuts straight to program");
  await page.screenshot({ path: process.env.SHOT_DIR + "/mixer.png" });

  await page.goto(base + "/tally?input=1");
  await page.waitForSelector("#light.program", { timeout: 5000 });
  await page.screenshot({ path: process.env.SHOT_DIR + "/tally.png" });
  await page.selectOption("#input", "2");
  await page.waitForSelector("#light.preview", { timeout: 5000 });
  pass("tally: program red, preview green, input picker");

  await page.goto(base + "/keys");
  await page.waitForSelector('#keys .btn[data-shortcut="1"]', { timeout: 5000 });
  await page.screenshot({ path: process.env.SHOT_DIR + "/keys.png" });
  await page.click('#keys .btn[data-shortcut="1"]');
  await page.waitForFunction(async () => {
    const s = await (await fetch("/api/state")).json();
    return s.program === s.inputs[1].id;
  }, null, { timeout: 5000 });
  pass("keys: the shortcut button runs its actions");

  if (errors.length) fail("page errors: " + errors.join("; "));
  pass("no page errors");
  await browser.close();
})().catch((e) => fail(e.message));
