// Drives titles through lmix-core and checks them in the graphics page
// (scripts/test-titles.sh).
//   WORK=/tmp/x node scripts/titles-check.cjs http://127.0.0.1:8898
"use strict";
const fs = require("fs");
const path = require("path");
const { chromium } = require("playwright");

const base = process.argv[2];
const work = process.env.WORK;
const fail = (message) => { console.log("FAIL: " + message); process.exit(1); };
const pass = (message) => console.log("ok   " + message);

async function act(action) {
  const resp = await fetch(base + "/api/actions", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(action),
  });
  const body = await resp.json();
  if (!resp.ok) fail(`${action.action}: ${resp.status} ${JSON.stringify(body)}`);
  return body;
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });

  // The page records the reports it sees, as a client would.
  await page.addInitScript(() => {
    if (window.top !== window) return; // not in the template frames
    window.__reports = [];
    const ws = new WebSocket(`ws://${location.host}/api/events`);
    ws.onmessage = (m) => {
      const e = JSON.parse(m.data);
      if (e.event === "title") window.__reports.push(e);
    };
  });

  await act({ action: "add_graphics", name: "Titles" });
  await page.goto(base + "/graphics/1");
  const layerFrame = async (layer) => {
    const handle = await page.waitForSelector(`#stage iframe[style*="z-index: ${layer}"]`, { timeout: 5000 });
    const frame = await handle.contentFrame();
    await frame.waitForLoadState();
    return frame;
  };

  // Built-in name and role: load hidden, play, update, stop and done.
  await act({ action: "title_load", graphics: "Titles", layer: 2, template: "lmix/name-role",
    data: { name: "Name A", role: "Role A" } });
  let frame = await layerFrame(2);
  await frame.waitForFunction(() => document.getElementById("name").textContent === "Name A");
  if (await frame.$eval("#name", (e) => e.classList.contains("is-in"))) fail("loaded title is visible");
  pass("load: data in, still hidden");

  await act({ action: "title_play", graphics: "Titles", layer: 2 });
  await frame.waitForSelector("#name.is-in", { timeout: 3000 });
  await act({ action: "title_update", graphics: "Titles", layer: 2, data: { role: "Role B" } });
  await frame.waitForFunction(() => document.getElementById("role").textContent === "Role B");
  if ((await frame.textContent("#name")) !== "Name A") fail("update lost a field");
  pass("play and update: in, field changed, others kept");
  await page.waitForTimeout(800);
  await page.screenshot({ path: path.join(work, "name-role.png") });

  await act({ action: "title_stop", graphics: "Titles", layer: 2 });
  await frame.waitForSelector("#name:not(.is-in)", { timeout: 3000 });
  await page.waitForFunction(() => window.__reports.some((r) => r.layer === 2 && r.report === "done"), null, { timeout: 3000 });
  pass("stop: out animation, template reports done");

  // A multi-step title: next reveals the text.
  await act({ action: "title_load", graphics: 1, layer: 1, template: "lmix/card",
    data: { title: "Card", text: "Line" }, play: true });
  frame = await layerFrame(1);
  await frame.waitForSelector("#title.is-in", { timeout: 3000 });
  if (await frame.$eval("#text", (e) => e.classList.contains("is-in"))) fail("text before next");
  await act({ action: "title_next", graphics: 1, layer: 1 });
  await frame.waitForSelector("#text.is-in", { timeout: 3000 });
  pass("next: second step of the card");
  await act({ action: "title_clear", graphics: 1, layer: 1 });
  await page.waitForFunction(() => !document.querySelector('#stage iframe[style*="z-index: 1"]'), null, { timeout: 3000 });
  pass("clear: layer gone at once");

  // Countdown: one second, reports zero.
  await act({ action: "title_load", graphics: 1, layer: 3, template: "lmix/countdown",
    data: { duration: "0:01" }, play: true });
  await page.waitForFunction(() => window.__reports.some((r) => r.layer === 3 && r.report === "zero"), null, { timeout: 5000 });
  pass("countdown: reports zero");

  // A CasparCG-style template from the folder, XML data; edited while it
  // plays, it reloads with the same data.
  const caspar = (tag) => `<!doctype html><html><body><p id="d"></p><script>
    function update(s) { document.getElementById("d").textContent = "${tag}:" + s; }
    function play() { document.body.dataset.played = "yes"; }
    function stop() {}
    function shout(a, b) { document.body.dataset.shout = a + b; }
  </script></body></html>`;
  fs.writeFileSync(path.join(work, "templates", "caspar.html"), caspar("v1"));
  fs.writeFileSync(path.join(work, "templates", "caspar.json"), JSON.stringify({ data_format: "xml", functions: ["shout"] }));
  for (let i = 0; i < 40; i++) {
    const list = await (await fetch(base + "/api/templates")).json();
    if (list.some((t) => t.id === "caspar")) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  await act({ action: "title_load", graphics: 1, layer: 4, template: "caspar", data: { f0: "a&b" }, play: true });
  frame = await layerFrame(4);
  const xml = '<templateData><componentData id="f0"><data id="text" value="a&amp;b"/></componentData></templateData>';
  await frame.waitForFunction((x) => document.getElementById("d").textContent === "v1:" + x, xml, { timeout: 3000 });
  await frame.waitForSelector('body[data-played="yes"]', { timeout: 3000 });
  pass("folder template: CasparCG XML data, played");

  await act({ action: "title_call", graphics: 1, layer: 4, function: "shout", args: ["x", "y"] });
  await frame.waitForSelector('body[data-shout="xy"]', { timeout: 3000 });
  pass("call: function with arguments");

  await new Promise((r) => setTimeout(r, 1100)); // a new mtime
  fs.writeFileSync(path.join(work, "templates", "caspar.html"), caspar("v2"));
  await page.waitForFunction(async (x) => {
    const f = [...document.querySelectorAll("#stage iframe")].find((e) => e.style.zIndex === "4");
    const d = f && f.contentDocument && f.contentDocument.getElementById("d");
    return d && d.textContent === "v2:" + x && f.contentDocument.body.dataset.played === "yes";
  }, xml, { timeout: 5000 });
  pass("edited template reloads with its data and plays again");

  // A CSV data source feeds a field; the file changes, the title follows.
  const csv = path.join(work, "guests.csv");
  fs.writeFileSync(csv, "name;role\nName A;Role A\nName B;Role B\n");
  await act({ action: "title_load", graphics: 1, layer: 5, template: "lmix/lower-third",
    data: { headline: "Head" }, play: true });
  await act({ action: "add_data_source", spec: { kind: "csv", path: csv }, name: "Guests" });
  let state = await act({ action: "title_bind", graphics: 1, layer: 5, field: "text",
    binding: { source: "Guests", column: "role", format: "({0})" } });
  if (state.data_sources[0].rows.length !== 2) fail("CSV rows: " + JSON.stringify(state.data_sources[0]));
  frame = await layerFrame(5);
  await frame.waitForFunction(() => document.getElementById("text").textContent === "(Role A)", null, { timeout: 3000 });
  await act({ action: "data_next", source: "Guests" });
  await frame.waitForFunction(() => document.getElementById("text").textContent === "(Role B)", null, { timeout: 3000 });
  pass("data source: bound field follows the row");
  await new Promise((r) => setTimeout(r, 1100));
  fs.writeFileSync(csv, "name;role\nName A;Role A\nName B;Role C\n");
  await frame.waitForFunction(() => document.getElementById("text").textContent === "(Role C)", null, { timeout: 5000 });
  pass("data source: edited file reaches the title");

  // A preview page shows the same, but reports nothing (the countdown
  // starts again there and reaches zero a second time).
  const zeros = await page.evaluate(() => window.__reports.filter((r) => r.report === "zero").length);
  const preview = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  preview.on("pageerror", (e) => errors.push(e.message));
  await preview.goto(base + "/graphics/1?preview");
  await preview.waitForSelector("#note:not(.offline)", { timeout: 3000 });
  await preview.waitForTimeout(1600);
  await preview.screenshot({ path: path.join(work, "preview.png") });
  const after = await page.evaluate(() => window.__reports.filter((r) => r.report === "zero").length);
  if (after !== zeros) fail(`preview page reported (${zeros} -> ${after} zeros)`);
  pass("preview page: same picture, no reports");

  // The template alone in a browser: its preview bar.
  const alone = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  alone.on("pageerror", (e) => errors.push(e.message));
  await alone.goto(base + "/titles/lmix/scoreboard/index.html");
  await alone.waitForSelector(".lmix-preview-bar button", { timeout: 3000 });
  await alone.click("text=Play");
  await alone.waitForSelector("#board.is-in", { timeout: 3000 });
  await alone.waitForTimeout(800);
  await alone.screenshot({ path: path.join(work, "scoreboard-alone.png") });
  pass("template alone: preview bar plays it");
  await alone.goto(base + "/titles/sdk/skeleton/index.html");
  await alone.click("text=Play");
  await alone.waitForSelector("#box.is-in", { timeout: 3000 });
  pass("skeleton for own templates plays");

  if (errors.length) fail("page errors: " + errors.join(" | "));
  await browser.close();
})().catch((e) => fail(e.stack || String(e)));
