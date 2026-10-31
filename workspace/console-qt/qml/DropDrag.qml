import QtQuick

/**
 * A drag that drops only when it ends with the pointer let go (`dropped`).
 * A drag taken away (a list that scrolls, a popup, the window losing the
 * pointer) ends without a drop; `ended` comes either way, to clear what
 * the drag showed. `moved` follows the pointer while it drags.
 */
DragHandler {
    id: handler

    signal moved(point scenePosition)
    signal dropped(point scenePosition)
    signal ended()

    /** Dragging and not yet let go or taken away. */
    property bool dragging: false

    target: null
    onCentroidChanged: if (active) handler.moved(centroid.scenePosition)
    onActiveChanged: {
        if (active) {
            dragging = true
        } else {
            handler.ended()
        }
    }
    // Let go: the grab goes with the released point; taken away: cancelled.
    onGrabChanged: (transition, point) => {
        if (!dragging) {
            return
        }
        if (transition === PointerDevice.CancelGrabExclusive) {
            dragging = false
            handler.ended()
        } else if (transition === PointerDevice.UngrabExclusive) {
            dragging = false
            if (point.state === EventPoint.Released) {
                handler.dropped(point.scenePosition)
            }
            handler.ended()
        }
    }
}
