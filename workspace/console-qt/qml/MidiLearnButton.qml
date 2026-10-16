import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * A MIDI binding learned from the controller: click, then press a pad or
 * move a control; Escape stops listening, the cross clears it.
 */
RowLayout {
    id: learn

    /** The Console backend; its `midiReceived` brings controller messages. */
    required property var core
    /** `{ kind, channel, number }` or `null`. */
    property var binding: null
    /** Only controllers ("cc"), or any note or controller (""). */
    property string only: ""
    property bool listening: false
    /** The owner sets `binding` from `chosen` (kept in lmix-core). */
    property bool controlled: false

    /** Learned or cleared (`null`). */
    signal chosen(var binding)

    function choose(b) {
        if (!controlled)
            binding = b
        chosen(b)
    }

    function describe(b) {
        if (!b)
            return qsTr("No MIDI")
        var what = b.kind === "cc" ? qsTr("CC %1").arg(b.number) : qsTr("Note %1").arg(b.number)
        return b.channel ? qsTr("%1, channel %2").arg(what).arg(b.channel) : what
    }

    spacing: 2

    KanteButton {
        id: button
        text: learn.listening ? qsTr("Press or move on the controller…") : learn.describe(learn.binding)
        Accessible.name: qsTr("MIDI: %1").arg(text)
        onClicked: {
            learn.listening = !learn.listening
            forceActiveFocus()
        }
        onActiveFocusChanged: if (!activeFocus) learn.listening = false
        Keys.onEscapePressed: learn.listening = false
    }
    KanteButton {
        visible: learn.binding !== null && !learn.listening
        Layout.preferredWidth: Kirigami.Units.gridUnit * 2
        text: "×"
        Accessible.name: qsTr("Remove MIDI binding")
        onClicked: learn.choose(null)
    }

    Connections {
        target: learn.core
        enabled: learn.listening
        function onMidiReceived(message) {
            var m = JSON.parse(message)
            // Notes on press only (not their release); controllers when they move.
            if (learn.only === "cc" && m.kind !== "cc")
                return
            if (m.kind === "note" && m.value === 0)
                return
            learn.listening = false
            learn.choose({ kind: m.kind, channel: m.channel, number: m.number })
        }
    }
}
