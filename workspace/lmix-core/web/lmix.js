// Shared by lmix's web pages: the event stream with reconnect, actions.
"use strict";

/** Follows /api/events; calls onEvent for each event, onLink(true|false). */
function lmixEvents(onEvent, onLink) {
  const url = (location.protocol === "https:" ? "wss://" : "ws://") + location.host + "/api/events";
  let retry = 500;
  const open = () => {
    const ws = new WebSocket(url);
    ws.onopen = () => { retry = 500; onLink && onLink(true); };
    ws.onmessage = (m) => { try { onEvent(JSON.parse(m.data)); } catch (e) { console.warn(e); } };
    ws.onclose = () => {
      onLink && onLink(false);
      setTimeout(open, retry);
      retry = Math.min(retry * 2, 5000);
    };
  };
  open();
}

/** Runs one action; resolves to the new state or throws the error text. */
async function lmixAct(action) {
  const resp = await fetch("/api/actions", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(action),
  });
  if (!resp.ok) throw new Error(await resp.text());
  return resp.json();
}

/** Shows the connection state in the header's .state element. */
function lmixLinkState(online) {
  const el = document.querySelector("header .state");
  if (!el) return;
  el.textContent = online ? "online" : "offline";
  el.classList.toggle("online", online);
}
