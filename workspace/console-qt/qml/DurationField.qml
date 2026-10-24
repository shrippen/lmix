import QtQuick
import Kante

/**
 * A time in ms, typed as seconds ("2.5") or minutes and seconds
 * ("1:05.3"), shown as m:ss.s. Taken on Enter or when the field is left;
 * cue times go in steps of 10 ms. With `optional` an empty field means no
 * time (`edited(null)`).
 */
KanteTextField {
    id: field

    /** The time; null shows an empty field. */
    property var ms: null
    property bool optional: false
    property int maxMs: 3600000
    signal edited(var ms)

    readonly property var parsed: read(text)

    /** ms of `text`, null when empty, undefined when unreadable. */
    function read(text) {
        var t = text.trim().replace(",", ".")
        if (t === "")
            return null
        var m = /^(?:(\d+):)?(\d+(?:\.\d*)?|\.\d+)$/.exec(t)
        if (!m)
            return undefined
        var seconds = (m[1] ? parseInt(m[1]) * 60 : 0) + parseFloat(m[2])
        var value = Math.round(seconds * 100) * 10
        return value <= maxMs ? value : undefined
    }

    function show(value) {
        if (value === null || value === undefined)
            return ""
        var hundredths = Math.round(value / 10)
        var minutes = Math.floor(hundredths / 6000)
        var seconds = (hundredths % 6000) / 100
        // 1:05.3, 0:02.25: tenths unless the value has hundredths.
        var digits = hundredths % 10 === 0 ? 1 : 2
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds.toFixed(digits)
    }

    text: show(ms)
    onMsChanged: if (!activeFocus) text = show(ms)
    invalid: parsed === undefined || (parsed === null && !optional)
    font.family: KanteStyle.monoFamily
    placeholderText: optional ? "–" : "0:00.0"
    onEditingFinished: {
        if (!invalid && parsed !== ms)
            edited(parsed)
        text = Qt.binding(() => show(ms))
    }
}
