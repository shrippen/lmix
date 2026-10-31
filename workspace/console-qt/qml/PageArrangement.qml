import QtQuick

/**
 * Which pages are in a window of their own and where those windows were
 * (docs/PLAN-SEITEN.md, "Seiten lösbar"), as one JSON text the window keeps
 * in its settings (`pages/windows` in lmix.conf, per machine):
 *
 *   { "show": { "detached": true, "x": 1920, "y": 0, "width": 1280,
 *               "height": 800, "screen": "DP-2", "visibility": "maximized" } }
 *
 * A page brought back keeps its place with `detached: false`, so the next
 * detaching opens it where it was. Only functions of the text: AppShell
 * applies them; tests/qml/tst_page_arrangement.qml checks them.
 */
QtObject {
    id: arrangement

    /** The saved text; AppShell binds and writes it. */
    property string text: "{}"
    /** The places by page id (see above); a broken text counts as none. */
    readonly property var places: parse(text)

    function parse(value) {
        try {
            const places = JSON.parse(value)
            return places !== null && typeof places === "object" && !Array.isArray(places) ? places : {}
        } catch (e) {
            return {}
        }
    }

    function isDetached(id) {
        return !!places[id] && places[id].detached === true
    }

    /** The text with page `id` detached (`on`) or docked, its place merged. */
    function changed(id, on, place) {
        const next = Object.assign({}, places)
        next[id] = Object.assign({}, places[id] || {}, place || {}, { detached: on })
        return JSON.stringify(next)
    }

    /** Whether page `id` of `order` (page ids) may leave the window: it is
     *  docked, and another docked page stays there. */
    function canDetach(order, id) {
        return order.indexOf(id) >= 0 && !isDetached(id)
            && order.some(other => other !== id && !isDetached(other))
    }

    /** The docked page to show when `gone` leaves the window: the next one
     *  after it in the bar, else the one before; "" when none is left. */
    function dockedAfter(order, gone) {
        const at = order.indexOf(gone)
        for (let i = 1; i < order.length; i++) {
            const id = order[(at + i) % order.length]
            if (id !== gone && !isDetached(id)) {
                return id
            }
        }
        return ""
    }

    /** What to remember of `window`: its normal geometry (not while
     *  maximized or full screen), its screen's name and how it is shown. */
    function placeOf(window, visibility) {
        const shown = visibility === Window.Maximized ? "maximized"
            : visibility === Window.FullScreen ? "fullscreen" : "normal"
        const place = { visibility: shown, screen: window.screen ? window.screen.name : "" }
        if (shown === "normal") {
            Object.assign(place, { x: window.x, y: window.y, width: window.width, height: window.height })
        }
        return place
    }

    /** `place` on one of `screens` (Qt.application.screens): its own screen
     *  while it is there, else `fallback`'s; the window inside the screen,
     *  at most as large. Returns `{ screen, x, y, width, height,
     *  visibility }`. `size` is the size for a place without one. */
    function fit(place, screens, fallback, size) {
        const named = screens.find(s => s.name === place.screen)
        const screen = named || fallback || screens[0] || null
        const area = screen ? { x: screen.virtualX, y: screen.virtualY, width: screen.width, height: screen.height }
            : { x: 0, y: 0, width: size.width, height: size.height }
        const width = Math.min(place.width > 0 ? place.width : size.width, area.width)
        const height = Math.min(place.height > 0 ? place.height : size.height, area.height)
        // Kept where it was on its own screen; centred on another one.
        const kept = named !== undefined && place.x !== undefined && place.y !== undefined
        const x = kept ? Math.max(area.x, Math.min(place.x, area.x + area.width - width))
            : area.x + Math.round((area.width - width) / 2)
        const y = kept ? Math.max(area.y, Math.min(place.y, area.y + area.height - height))
            : area.y + Math.round((area.height - height) / 2)
        return { screen: screen, x: x, y: y, width: width, height: height, visibility: place.visibility || "normal" }
    }

    /** For a page detached the first time: another screen than `current`
     *  if there is one (the second screen), else `current`. */
    function otherScreen(screens, current) {
        return screens.find(s => current && s.name !== current.name) || current || screens[0] || null
    }
}
