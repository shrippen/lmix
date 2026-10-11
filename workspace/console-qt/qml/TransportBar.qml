import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Transport under a monitor (UI.5): play, pause, back to the start and,
 * for playlists, the item before and after, for the file, URL or playlist
 * input the monitor shows; the length of what plays beside it. Hidden for
 * other inputs and layouts.
 */
RowLayout {
    id: bar

    /** lmix `Input` on the monitor, or null. */
    required property var input
    required property var act

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
    Item { Layout.fillWidth: true }
    QQC2.Label {
        visible: bar.input !== null && !!bar.input.clip_ms
        text: bar.input && bar.input.clip_ms ? bar.length(bar.input.clip_ms) : ""
        font.family: KanteStyle.monoFamily
        color: KanteStyle.mutedTextColor
        Accessible.name: qsTr("Length %1").arg(text)
    }
}
