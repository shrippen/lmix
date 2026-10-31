import QtQuick
import Kante

/**
 * A time in ms, typed as seconds ("2.5") or minutes and seconds
 * ("1:05.3"), shown as m:ss.s. Taken on Enter or when the field is left;
 * cue times go in steps of 10 ms. With `optional` an empty field means no
 * time (`edited(null)`). An unreadable time is not dropped: it stays in
 * the field, marked, until it reads or `key` (whose time it is) changes;
 * Enter on it keeps the keys in the field.
 */
KanteTextField {
    id: field

    /** The time; null shows an empty field. */
    property var ms: null
    property bool optional: false
    property int maxMs: 3600000
    /** Whose time it is, e.g. the cue's id. */
    property var key
    signal edited(var ms)

    readonly property var parsed: read(text)

    /** ms of `text`, null when empty, undefined when unreadable. */
    function read(text) {
        var t = text.trim().replace(",", ".")
        if (t === "") {
            return null
        }
        var m = /^(?:(\d+):)?(\d+(?:\.\d*)?|\.\d+)$/.exec(t)
        if (!m) {
            return undefined
        }
        var seconds = (m[1] ? parseInt(m[1]) * 60 : 0) + parseFloat(m[2])
        var value = Math.round(seconds * 100) * 10
        return value <= maxMs ? value : undefined
    }

    function show(value) {
        if (value === null || value === undefined) {
            return ""
        }
        var hundredths = Math.round(value / 10)
        var minutes = Math.floor(hundredths / 6000)
        var seconds = (hundredths % 6000) / 100
        // 1:05.3, 0:02.25: tenths unless the value has hundredths.
        var digits = hundredths % 10 === 0 ? 1 : 2
        return minutes + ":" + (seconds < 10 ? "0" : "") + seconds.toFixed(digits)
    }

    // What is typed stays until it is taken; a new state meanwhile does
    // not overwrite it.
    property bool typing: false
    onActiveFocusChanged: if (activeFocus) { typing = true }
    onKeyChanged: typing = false
    Binding on text {
        when: !field.typing
        value: field.show(field.ms)
        restoreMode: Binding.RestoreNone
    }
    invalid: parsed === undefined || (parsed === null && !optional)
    font.family: KanteStyle.monoFamily
    placeholderText: optional ? "–" : "0:00.0"
    // Enter on an unreadable time: no `accepted`, the keys stay here.
    Keys.onReturnPressed: event => event.accepted = invalid
    Keys.onEnterPressed: event => event.accepted = invalid
    onEditingFinished: {
        if (invalid) {
            return
        }
        if (parsed !== ms) {
            edited(parsed)
        }
        typing = false
    }
}
