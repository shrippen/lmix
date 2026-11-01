import QtQuick

/**
 * A drag that drops only when it ends with the pointer let go (`dropped`).
 * A drag taken away (a list that scrolls, a popup, the window losing the
 * pointer) ends without a drop. `ended` comes once per drag, to clear what
 * the drag showed, and after `dropped` when there is one: a drop may still
 * read what the drag showed. A drop that rebuilds the handler's delegate
 * takes its `ended` with it, so such a drop clears what the drag showed
 * itself. `moved` follows the pointer while it drags.
 */
DragHandler {
    id: handler

    signal moved(point scenePosition)
    signal dropped(point scenePosition)
    signal ended()

    /** Dragging and not yet let go or taken away. */
    property bool dragging: false

    /** Ends the drag once, after its drop if any. */
    function finish(point) {
        if (!dragging) {
            return
        }
        dragging = false
        if (point && point.state === EventPoint.Released) {
            handler.dropped(point.scenePosition)
        }
        handler.ended()
    }

    target: null
    onCentroidChanged: if (active) handler.moved(centroid.scenePosition)
    // Qt clears `active` before it reports the grab going, so the end
    // waits for that (below). Only when no grab change comes at all (the
    // handler turned off midway) does it end here, a moment later.
    onActiveChanged: {
        if (active) {
            dragging = true
        } else {
            Qt.callLater(() => {
                if (!handler.active) {
                    handler.finish(null)
                }
            })
        }
    }
    // Let go: the grab goes with the released point; taken away: cancelled.
    onGrabChanged: (transition, point) => {
        if (transition === PointerDevice.CancelGrabExclusive) {
            finish(null)
        } else if (transition === PointerDevice.UngrabExclusive) {
            finish(point)
        }
    }
}
