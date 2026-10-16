import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Transport under a monitor (UI.5): play, pause, back to the start and,
 * for playlists, the item before and after, for the file, URL or playlist
 * input the monitor shows; the length of what plays beside it. A click in
 * the progress bar jumps there. Hidden for other inputs and layouts.
 */
RowLayout {
    id: bar

    /** lmix `Input` on the monitor, or null. */
    required property var input
    required property var act
    /** `[position ms, length ms]` of the input while it plays, else null. */
    property var position: null

    readonly property string kind: input ? input.source.kind : ""
    readonly property bool playlist: kind === "playlist"

    function send(transport) {
        bar.act({ action: "transport_input", input: bar.input.id, transport: transport })
    }
    /** `m:ss` of a length in ms. */
    function length(ms) {
        const s = Math.round(ms / 1000)
        return Math.floor(s / 60) + ":" + String(s % 60).padStart(2, "0")
    }

    visible: ["file", "url", "playlist"].indexOf(kind) >= 0
    spacing: Kirigami.Units.smallSpacing

    component Key: KanteButton {
        required property string transport
        required property string hint
        Accessible.name: hint
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: hint
        onClicked: bar.send(transport)
    }

    Key { text: "⏮"; transport: "previous"; hint: qsTr("Previous"); visible: bar.playlist }
    Key { text: "↺"; transport: "restart"; hint: qsTr("Restart") }
    Key { text: "▶"; transport: "play"; hint: qsTr("Play"); emphasis: KanteButton.Emphasis.Primary }
    Key { text: "⏸"; transport: "pause"; hint: qsTr("Pause") }
    Key { text: "⏭"; transport: "next"; hint: qsTr("Next"); visible: bar.playlist }
    // Where it plays (B39); the length alone until the player reports.
    KanteProgressBar {
        id: progress
        Layout.fillWidth: true
        visible: bar.position !== null && bar.position[1] > 0
        value: bar.position && bar.position[1] > 0 ? bar.position[0] / bar.position[1] : 0
        Accessible.name: qsTr("Position; click to jump")
        MouseArea {
            anchors.fill: parent
            // A taller target than the thin bar.
            anchors.topMargin: -Kirigami.Units.smallSpacing
            anchors.bottomMargin: -Kirigami.Units.smallSpacing
            cursorShape: Qt.PointingHandCursor
            onClicked: mouse => bar.act({ action: "seek_input", input: bar.input.id,
                position_ms: Math.round(Math.max(0, Math.min(1, mouse.x / width)) * bar.position[1]) })
        }
    }
    Item {
        Layout.fillWidth: true
        visible: !(bar.position !== null && bar.position[1] > 0)
    }
    QQC2.Label {
        readonly property real total: bar.position && bar.position[1] > 0 ? bar.position[1] : (bar.input && bar.input.clip_ms ? bar.input.clip_ms : 0)
        visible: total > 0
        text: (bar.position ? bar.length(bar.position[0]) + " / " : "") + bar.length(total)
        font.family: KanteStyle.monoFamily
        color: KanteStyle.mutedTextColor
        Accessible.name: qsTr("Length %1").arg(text)
    }
}
