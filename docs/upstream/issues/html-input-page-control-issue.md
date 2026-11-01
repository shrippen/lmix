Title: HTML Input: let the API script the page (evaluate, CSS, freeze/resume)

## What is missing

An HTML Input can be pointed at a URL and handed out as a remote-control link, but a control system cannot drive the page itself: call the page's own JavaScript functions (start a lower third, update a score), add CSS to it, or stop it rendering while it is off air.

## Proposed direction

`POST /api/flows/{flow}/blocks/{block}/devtools/page` taking an `HtmlPageCommand` (in `strom-types`):
- `evaluate`: `Runtime.evaluate`, a returned promise awaited, the value returned;
- `set_css`: one managed `<style>` element, the CSS passed as a quoted string literal;
- `freeze` / `resume`: `Page.setWebLifecycleState`.

It talks to the page's own DevTools endpoint on the loopback debug port, outside the remote-control link filter, and only for blocks with a new `page_scripting` property switched on (off by default, changeable live). Annotated with `utoipa` and registered in `openapi.rs`.

## Questions for the maintainers

1. Is script evaluation through the API acceptable at all? It is arbitrary JavaScript in the page; the property keeps it off unless a flow opts in, and the API's authentication applies. Would you want a narrower command set (only named functions)?
2. One endpoint with a tagged command, or one endpoint per command?
3. Freeze/resume overlaps with pausing the source; is the lifecycle state the right lever?

## Test case

The DevTools exchange is testable without Chromium against a stand-in DevTools WebSocket endpoint: each command produces the expected CDP message (`Runtime.evaluate` with `awaitPromise`, the managed style element, `Page.setWebLifecycleState`), the evaluate result is returned, and a block without `page_scripting` answers 403. An end-to-end run needs `cefsrc`.
