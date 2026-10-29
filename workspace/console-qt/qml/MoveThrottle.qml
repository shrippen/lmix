import QtQuick

/**
 * Passes a slider's moves on at most every `interval` ms: the first at
 * once, then the newest one per interval, and with `flush()` (on release)
 * the last one. Each move is an action and a state back; a drag gives one
 * per pixel, and their states froze the window (29.10.2026).
 */
Timer {
    id: throttle

    signal passed(var value)

    property var pending: undefined
    property bool hasPending: false

    interval: 33

    function push(value) {
        if (running) {
            pending = value
            hasPending = true
            return
        }
        passed(value)
        start()
    }

    function flush() {
        stop()
        if (hasPending) {
            hasPending = false
            passed(pending)
        }
    }

    onTriggered: {
        if (hasPending) {
            hasPending = false
            passed(pending)
            start()
        }
    }
}
