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
  // State changes arrive as diffs (API.6), whole states only now and then.
  const frames = { diff: 0, state: 0 };
  page.on("websocket", (ws) => ws.on("framereceived", (f) => {
    const kind = /^\{"event":"(state_diff|state)"/.exec(String(f.payload));
    if (kind) frames[kind[1] === "state_diff" ? "diff" : "state"]++;
  }));

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
  pass("mixer: double click runs GO (quick play) to program");
  await page.screenshot({ path: process.env.SHOT_DIR + "/mixer.png" });

  await page.goto(base + "/tally?input=1");
  await page.waitForSelector("#light.program", { timeout: 5000 });
  await page.screenshot({ path: process.env.SHOT_DIR + "/tally.png" });
  await page.selectOption("#input", "2");
  await page.waitForSelector("#light.preview", { timeout: 5000 });
  pass("tally: program red, preview green, input picker");

  await page.goto(base + "/keys");
  await page.waitForSelector("#keys .btn[data-shortcut]", { timeout: 5000 });
  await page.screenshot({ path: process.env.SHOT_DIR + "/keys.png" });
  await page.click("#keys .btn[data-shortcut]");
  await page.waitForFunction(async () => {
    const s = await (await fetch("/api/state")).json();
    return s.program === s.inputs[1].id;
  }, null, { timeout: 5000 });
  pass("keys: the shortcut button runs its actions");
  // It switches input 2: its picture, red now that it is on program.
  // The null engine has no pictures: the image is there, hidden.
  await page.waitForSelector("#keys .btn.program img[data-picture]", { state: "attached", timeout: 5000 });
  pass("keys: picture and tally of the input it switches");
  // A key without an input stands as its MIDI lamp: all overlays off is
  // on while an overlay shows, and off once it ran.
  await fetch(base + "/api/actions", { method: "POST", headers: { "content-type": "application/json" },
    body: JSON.stringify({ action: "overlay_in", channel: 1, input: 1, duration_ms: 0 }) });
  const lit = await page.waitForSelector("#keys .btn.on .lamp", { timeout: 5000 });
  if ((await lit.textContent()) !== "on") fail("keys: lamp word of a lit key");
  await page.click("#keys .btn.on");
  await page.waitForSelector("#keys .btn.on", { state: "detached", timeout: 5000 });
  if ((await api("/api/state")).overlays.some((id) => id)) fail("keys: overlays not all off");
  pass("keys: a key without an input stands as its MIDI lamp");

  if (frames.diff === 0 || frames.diff < frames.state) fail("diffs: " + JSON.stringify(frames));
  pass(`state as diffs (${frames.diff} diffs, ${frames.state} whole)`);

  if (errors.length) fail("page errors: " + errors.join("; "));
  pass("no page errors");

  // Actions go out in the order they were made: quick clicks (a double
  // click: click, click, GO) must not overtake each other on the way.
  // A page of its own: the previews it sends after the cut are refused on
  // purpose, which the browser logs as errors.
  const quick = await browser.newPage();
  await quick.goto(base + "/mixer");
  await quick.waitForFunction(() => document.querySelectorAll("#inputs .tile").length === 2, null, { timeout: 5000 });
  const program = (await api("/api/state")).program;
  const other = (await api("/api/state")).inputs.find((i) => i.id !== program);
  const order = await quick.evaluate(async (n) => {
    const sent = [];
    for (let i = 0; i < 100; i++) sent.push(lmixAct({ action: "preview_input", input: n }).catch(() => "refused"));
    sent.push(lmixAct({ action: "cut" }));
    for (let i = 0; i < 100; i++) sent.push(lmixAct({ action: "preview_input", input: n }).catch(() => "refused"));
    return (await Promise.all(sent)).filter((r) => r === "refused").length;
  }, other.number);
  if (order !== 100) fail(`actions out of order: ${order} of 100 previews after the cut refused (on program), not all`);
  pass("actions from a page keep their order");

  // A hanging request holds the next action back at most 2 s (GO, panic).
  const held = [];
  await quick.route("**/api/actions", (route) => (held.length === 0 ? held.push(route) : route.continue()));
  const waited = await quick.evaluate(async (n) => {
    lmixAct({ action: "preview_input", input: n }).catch(() => {});
    const t0 = performance.now();
    await lmixAct({ action: "cue_stop" }).catch(() => {});
    return performance.now() - t0;
  }, other.number);
  if (held.length !== 1) fail("hanging request: the first action did not hang");
  if (waited < 1500 || waited > 4000) fail(`hanging request: the next action waited ${Math.round(waited)} ms, not about 2000`);
  pass(`a hanging request holds the next action back ${Math.round(waited)} ms`);
  await held[0].continue();
  await quick.close();
  await browser.close();
})().catch((e) => fail(e.message));
