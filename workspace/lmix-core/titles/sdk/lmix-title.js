// lmix title SDK (GFX.13): the CasparCG convention for a template in a few
// lines, and a preview in a normal browser.
//
//   <script src="/titles/sdk/lmix-title.js"></script>
//   <script>
//     lmixTitle({
//       update(data) { … },          // field values as an object
//       play() { … },                // in animation
//       next(step) { … },            // 1, 2, … after play
//       stop() { … return 400; },    // out animation; its length in ms
//     });
//   </script>
//
// lmixTitle defines the global play(), next(), stop() and update(text)
// that lmix (and CasparCG) call. update gets the data as text, JSON or
// CasparCG's <templateData> XML; the SDK hands the template an object.
// After stop() it reports "done" once the out animation is over.
// lmixTitle.report(name) reports something else (e.g. "zero").
//
// Opened on its own (not inside the graphics page), the template shows a
// bar with Play / Next / Stop and its fields from manifest.json (styled by
// lmix-title.css).
"use strict";

(function () {
  /** Field values from update's text: JSON object or CasparCG XML. */
  function parseData(text) {
    if (text === undefined || text === null || text === "") return {};
    if (typeof text === "object") return text;
    const trimmed = String(text).trim();
    if (trimmed.startsWith("<")) {
      const doc = new DOMParser().parseFromString(trimmed, "application/xml");
      const data = {};
      for (const component of doc.querySelectorAll("componentData")) {
        const value = component.querySelector("data");
        if (value) data[component.getAttribute("id")] = value.getAttribute("value") || "";
      }
      return data;
    }
    try {
      const parsed = JSON.parse(trimmed);
      return parsed && typeof parsed === "object" ? parsed : {};
    } catch (e) {
      console.warn("lmix title: data is neither JSON nor XML", e);
      return {};
    }
  }

  function report(name) {
    try {
      if (window.parent !== window && window.parent.lmixReport) window.parent.lmixReport(window, name);
    } catch (e) {
      console.warn("lmix title: report failed", e);
    }
  }

  function lmixTitle(impl) {
    let step = 0;
    let doneTimer = null;
    const data = {};

    window.update = function (text) {
      Object.assign(data, parseData(text));
      if (impl.update) impl.update(Object.assign({}, data));
    };
    window.play = function () {
      clearTimeout(doneTimer);
      step = 0;
      if (impl.play) impl.play();
    };
    window.next = function () {
      step += 1;
      if (impl.next) impl.next(step);
    };
    window.stop = function () {
      const ms = impl.stop ? impl.stop() : 0;
      clearTimeout(doneTimer);
      doneTimer = setTimeout(() => report("done"), typeof ms === "number" ? ms : 0);
    };
    // Functions besides the convention, for CG INVOKE / title_call.
    for (const name of Object.keys(impl.functions || {})) {
      window[name] = (...args) => impl.functions[name](...args);
    }
    if (window.parent === window) document.addEventListener("DOMContentLoaded", previewBar);
  }

  /** A duration token (`--dur`, `--dur-slow`) of the page in ms. */
  function duration(name) {
    const text = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
    const value = parseFloat(text);
    if (Number.isNaN(value)) return 0;
    return text.endsWith("ms") ? value : value * 1000;
  }

  /** Screen pixels per --u (a pixel of a 1080p picture), measured. */
  function unit() {
    const probe = document.createElement("div");
    probe.style.cssText = "position:absolute;visibility:hidden;width:calc(1000 * var(--u))";
    document.body.appendChild(probe);
    const px = probe.getBoundingClientRect().width / 1000;
    probe.remove();
    return px || 1;
  }

  lmixTitle.report = report;
  lmixTitle.unit = unit;
  lmixTitle.parseData = parseData;
  lmixTitle.duration = duration;

  /** Preview outside lmix: a bar with the convention's calls and the
   *  manifest's fields (labels stand in for values). */
  async function previewBar() {
    let manifest = {};
    try {
      const resp = await fetch("manifest.json", { cache: "no-store" });
      if (resp.ok) manifest = await resp.json();
    } catch (e) {
      // A template without manifest still previews.
    }
    document.documentElement.classList.add("lmix-preview");
    const bar = document.createElement("form");
    bar.className = "lmix-preview-bar";
    const fields = manifest.fields || [];
    for (const field of fields) {
      const input = document.createElement(field.kind === "long_text" ? "textarea" : "input");
      input.name = field.id;
      input.placeholder = field.label || field.id;
      input.value = field.default !== undefined ? field.default : field.label || field.id;
      if (field.kind === "color") input.type = "color";
      if (field.kind === "number") input.type = "number";
      bar.appendChild(input);
    }
    const values = () => Object.fromEntries(new FormData(bar).entries());
    const button = (label, act) => {
      const b = document.createElement("button");
      b.type = "button";
      b.textContent = label;
      b.onclick = act;
      bar.appendChild(b);
    };
    button("Update", () => window.update(JSON.stringify(values())));
    button("Play", () => window.play());
    button("Next", () => window.next());
    button("Stop", () => window.stop());
    for (const name of manifest.functions || []) {
      button(name, () => window[name] && window[name]());
    }
    document.body.appendChild(bar);
    window.update(JSON.stringify(values()));
  }

  window.lmixTitle = lmixTitle;
})();
