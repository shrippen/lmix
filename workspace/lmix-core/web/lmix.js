// Shared by lmix's web pages: the event stream with reconnect, actions.
"use strict";

/** Follows /api/events; calls onEvent for each event, onLink(true|false). */
function lmixEvents(onEvent, onLink) {
  // State changes come as diffs (API.6) and are applied here: pages see
  // whole states as before, the network carries only what changed.
  const url = (location.protocol === "https:" ? "wss://" : "ws://") + location.host + "/api/events?diff=true";
  let retry = 500;
  let state = null;
  const open = () => {
    const ws = new WebSocket(url);
    ws.onopen = () => { retry = 500; onLink && onLink(true); };
    ws.onmessage = (m) => {
      let event;
      try {
        event = JSON.parse(m.data);
        if (event.event === "state_diff") {
          if (!state) throw new Error("diff before a state");
          event = { event: "state", state: lmixPatch(state, event.ops) };
        }
        if (event.event === "state") state = event.state;
      } catch (e) {
        console.warn(e);
        // Out of step: start over with a whole state.
        state = null;
        ws.close();
        return;
      }
      // A page's own error does not drop the link.
      try { onEvent(event); } catch (e) { console.warn(e); }
    };
    ws.onclose = () => {
      onLink && onLink(false);
      setTimeout(open, retry);
      retry = Math.min(retry * 2, 5000);
    };
  };
  open();
}

/** `doc` with JSON Patch `ops` (add, remove, replace) applied. Objects
 *  and arrays on a changed path are copied, the rest is shared: a page may
 *  keep an older state, it never changes under it. */
function lmixPatch(doc, ops) {
  // Own properties only, set as data (a key `__proto__` stays a key).
  const put = (o, k, v) => Object.defineProperty(o, k, { value: v, writable: true, enumerable: true, configurable: true });
  const copy = (v) => {
    if (Array.isArray(v)) return v.slice();
    const c = {};
    for (const k of Object.keys(v)) put(c, k, v[k]);
    return c;
  };
  for (const op of ops) {
    const tokens = op.path.split("/").slice(1).map((t) => t.replace(/~1/g, "/").replace(/~0/g, "~"));
    if (tokens.length === 0) { doc = op.value; continue; }
    const root = copy(doc);
    let at = root;
    for (const t of tokens.slice(0, -1)) {
      put(at, t, copy(at[t]));
      at = at[t];
    }
    const last = tokens[tokens.length - 1];
    if (Array.isArray(at)) {
      const i = Number(last);
      if (op.op === "remove") at.splice(i, 1);
      else if (op.op === "add") at.splice(i, 0, op.value);
      else at[i] = op.value;
    } else if (op.op === "remove") {
      delete at[last];
    } else {
      put(at, last, op.value);
    }
    doc = root;
  }
  return doc;
}

/** The last action on its way: the next one waits for its answer, but
 *  goes out at most LMIX_ACT_WAIT_MS after it was made, however many
 *  requests hang before it (Wi-Fi gone, a slow action in lmix-core): GO
 *  and panic are never held back longer. Hanging requests are not
 *  cancelled: what lmix-core gets, it does. */
const LMIX_ACT_WAIT_MS = 2000;
let lmixActQueue = Promise.resolve();

/** Runs one action; resolves to the new state or throws the error text.
 *  Actions go out one after the other, in the order they were made: a
 *  double click (click, click, GO) sent at once could arrive overtaken. */
function lmixAct(action) {
  let answered;
  const gate = new Promise((open) => { answered = open; });
  const deadline = new Promise((go) => setTimeout(go, LMIX_ACT_WAIT_MS));
  const run = Promise.race([lmixActQueue, deadline]).then(async () => {
    try {
      const resp = await fetch("/api/actions", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(action),
      });
      if (!resp.ok) throw new Error(await resp.text());
      return resp.json();
    } finally {
      answered();
    }
  });
  lmixActQueue = gate;
  return run;
}

/** Shows the connection state in the header's .state element. */
function lmixLinkState(online) {
  const el = document.querySelector("header .state");
  if (!el) return;
  el.textContent = online ? "online" : "offline";
  el.classList.toggle("online", online);
}
