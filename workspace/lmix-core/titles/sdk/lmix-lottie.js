// Lottie in lmix titles (GFX.5): an After Effects / Bodymovin animation as
// a template, through lottie-web (sdk/lottie, MIT).
//
//   <div id="anim"></div>
//   <script src="/titles/sdk/lottie/lottie_svg.min.js"></script>
//   <script src="/titles/sdk/lmix-title.js"></script>
//   <script src="/titles/sdk/lmix-lottie.js"></script>
//   <script>lmixTitle(lmixLottie({ container: anim, animation: "animation.json" }));</script>
//
// The animation's conventions:
// - Text layers are fields: a layer named like a field shows its value.
// - Markers: "in" plays on play(), "out" on stop() (then "done"); every
//   other marker, in time order, is a step for next(). Without markers
//   play() runs the whole animation and stop() fades it out.
// - A layer named "kante:<token>" (e.g. "kante:bg0", "kante:yellow") takes
//   its fill, or its text colour, from Kante's token of that name, so the
//   animation follows Kante instead of fixed colours.
"use strict";

(function () {
  /** `#rrggbb` (or `rgb(…)`) of a CSS custom property as Lottie's 0–1 RGB. */
  function tokenRgb(name) {
    const probe = document.createElement("span");
    probe.style.color = `var(--${name})`;
    document.body.appendChild(probe);
    const rgb = getComputedStyle(probe).color.match(/[\d.]+/g);
    probe.remove();
    return rgb ? rgb.slice(0, 3).map((v) => Number(v) / 255) : null;
  }

  /** Fills and text colours of "kante:<token>" layers from the tokens. */
  function recolor(json) {
    const visit = (items, rgb) => {
      for (const item of items || []) {
        if (item.ty === "fl" && item.c && item.c.a === 0) item.c.k = [...rgb, 1];
        if (item.it) visit(item.it, rgb);
      }
    };
    for (const layer of json.layers || []) {
      const m = /^kante:([\w-]+)/.exec(layer.nm || "");
      const rgb = m && tokenRgb(m[1]);
      if (!rgb) continue;
      visit(layer.shapes, rgb);
      for (const key of (layer.t && layer.t.d && layer.t.d.k) || []) key.s.fc = rgb;
    }
  }

  /** CSS weight of a font style name ("Bold", "Medium", …). */
  function weightOf(style) {
    const names = { thin: 100, light: 300, regular: 400, medium: 500, semibold: 600, bold: 700, black: 900 };
    return names[String(style || "").toLowerCase().replace(/[\s-]/g, "")] || 400;
  }

  /** Field name of a text layer: its name without a "kante:…" prefix. */
  function fieldOf(layer) {
    return (layer.nm || "").replace(/^kante:[\w-]+\s*/, "").trim();
  }

  function lmixLottie(options) {
    let anim = null, json = null;
    let pending = {};
    const ready = (async () => {
      const resp = await fetch(options.animation, { cache: "no-store" });
      json = await resp.json();
      recolor(json);
      // Texts set before the animation exists go into it directly.
      for (const layer of json.layers || []) {
        const field = fieldOf(layer);
        if (layer.ty === 5) layer.t.d.k[0].s.t = String(pending[field] ?? "");
      }
      // lottie measures each letter with the font once; it has to be
      // there before (fonts load only when something uses them).
      await Promise.all(((json.fonts && json.fonts.list) || []).map((f) =>
        document.fonts.load(`${weightOf(f.fStyle)} 16px "${f.fFamily}"`).catch(() => [])));
      await document.fonts.ready;
      anim = lottie.loadAnimation({
        container: options.container,
        renderer: "svg",
        loop: false,
        autoplay: false,
        animationData: json,
      });
      await new Promise((resolve) => anim.addEventListener("DOMLoaded", resolve));
      // For tests and the browser console.
      window.lmixLottieAnimation = anim;
      anim.goToAndStop(0, true);
      options.container.style.opacity = "0";
    })();
    ready.catch((e) => console.warn("lmix lottie:", e));

    const markers = () => (json.markers || []).slice().sort((a, b) => a.tm - b.tm);
    const marker = (name) => markers().find((m) => m.cm === name);
    const steps = () => markers().filter((m) => m.cm !== "in" && m.cm !== "out");
    const ms = (frames) => (frames / (json.fr || 25)) * 1000;
    const playMarker = (m) => anim.playSegments([m.tm, m.tm + m.dr], true);

    return {
      // All values each time: a text layer without one is emptied.
      update(data) {
        pending = data;
        ready.then(() => {
          for (const element of anim.renderer.elements || []) {
            const layer = element && element.data;
            if (!layer || layer.ty !== 5) continue;
            element.updateDocumentData({ t: String(data[fieldOf(layer)] ?? "") }, 0);
          }
          // Paused animations draw the new text on the next frame.
          if (anim.isPaused) anim.goToAndStop(anim.currentFrame, true);
        });
      },
      play() {
        ready.then(() => {
          options.container.style.transition = "none";
          options.container.style.opacity = "1";
          const into = marker("in");
          if (into) playMarker(into);
          else anim.goToAndPlay(0, true);
        });
      },
      next(step) {
        ready.then(() => {
          const m = steps()[step - 1];
          if (m) playMarker(m);
        });
      },
      stop() {
        if (!anim) return 0;
        const out = marker("out");
        if (out) {
          playMarker(out);
          return ms(out.dr);
        }
        options.container.style.transition = "opacity var(--dur) var(--ease-out)";
        options.container.style.opacity = "0";
        return lmixTitle.duration("--dur");
      },
    };
  }

  window.lmixLottie = lmixLottie;
})();
