import QtQuick
import org.kde.kirigami as Kirigami

/**
 * Scrolls `flickable` while a drag holds the pointer near its top or bottom
 * edge (or beyond it), so a row goes to a place out of sight in one drag.
 * Runs only then; `scrolled` follows each step, since the row under a
 * pointer held still changes with the list.
 *
 *   ┌──────────────┐
 *   │ ▲ edge       │  pointer here: scrolls up
 *   │              │
 *   │ ▼ edge       │  pointer here: scrolls down
 *   └──────────────┘
 */
Timer {
    id: scroll

    /** The list to scroll (a ScrollView's contentItem). */
    required property Flickable flickable
    /** A drag over this list is on. */
    property bool dragging: false
    /** Where the pointer is, in the window. */
    property point scenePosition
    /** How near the edge the pointer scrolls. */
    readonly property real edge: Kirigami.Units.gridUnit * 2
    /** How far one step scrolls (about 30 rows a second). */
    readonly property real step: Kirigami.Units.gridUnit / 2

    signal scrolled()

    readonly property real top: flickable.originY - flickable.topMargin
    readonly property real bottom: Math.max(top, flickable.originY + flickable.contentHeight
        + flickable.bottomMargin - flickable.height)
    /** -1 up, 1 down, 0 still. */
    readonly property int direction: {
        if (!dragging) {
            return 0
        }
        const y = flickable.mapFromItem(null, scenePosition.x, scenePosition.y).y
        if (y < edge && flickable.contentY > top) {
            return -1
        }
        if (y > flickable.height - edge && flickable.contentY < bottom) {
            return 1
        }
        return 0
    }

    // One step a frame.
    interval: 16
    repeat: true
    running: direction !== 0
    onTriggered: {
        flickable.contentY = Math.min(bottom, Math.max(top, flickable.contentY + direction * step))
        scrolled()
    }
}
