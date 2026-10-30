import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * An output's menu (docs/DESIGN-DIALOGE.md M07 B): a head with name,
 * kind and the bitrate while it sends; Start or Stop…, Settings…, Copy
 * address, Statistics ▸; Remove last and red, locked while it sends.
 *   output   the output (`ShowState.outputs`)
 *   link     its link stats (SRT: `Core.links`), or undefined
 *   elapsed  its running time as shown
 * `startRequested()`, `stopRequested()`, `settingsRequested()`,
 * `removeRequested()`; the owner asks before stopping and removing.
 */
QQC2.Menu {
    id: outputMenu

    property var output: ({ id: "", number: 0, name: "", spec: { kind: "" }, status: { state: "stopped" } })
    property var link: undefined
    property string elapsed: ""
    signal startRequested()
    signal stopRequested()
    signal settingsRequested()
    signal removeRequested()

    readonly property string phase: output.status.state
    readonly property bool sending: phase === "running"
    /** A stop ends a run or the wait for the cards. */
    readonly property bool stoppable: sending || phase === "cards_off"
    readonly property string address: Texts.outputTarget(output.spec) || ""
    /** The rate while it sends: measured on a link, else the one set. */
    readonly property string rate: !sending ? ""
        : link ? Texts.linkRate(link)
        : output.spec.video_kbps !== undefined ? qsTr("%1 kbit/s").arg(output.spec.video_kbps) : ""

    /** An address without path and query: a stream key stays out of sight. */
    function shortAddress(text) {
        const url = text.match(/^([a-z]+:\/\/[^\/?]*)/i)
        if (url) {
            return url[1]
        }
        return text.split("/").filter(p => p !== "").pop() || text
    }

    // QML has no clipboard API; a hidden text edit copies.
    readonly property TextEdit clipboard: TextEdit { visible: false }
    function copy(value) {
        clipboard.text = value
        clipboard.selectAll()
        clipboard.copy()
    }

    KantePopupSkin { popup: outputMenu }
    delegate: KanteMenuItem {
        iconRoom: true
        icon.name: subMenu && subMenu.iconName ? subMenu.iconName : ""
    }

    RowLayout {
        objectName: "head"
        width: parent ? parent.width : implicitWidth
        spacing: Kirigami.Units.largeSpacing
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing
            Layout.rightMargin: 0
            spacing: 0
            QQC2.Label {
                Layout.fillWidth: true
                text: outputMenu.output.name
                font.bold: true
                elide: Text.ElideRight
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: Texts.outputKind(outputMenu.output.spec)
                color: KanteStyle.mutedTextColor
                font.pointSize: Kirigami.Theme.smallFont.pointSize
                elide: Text.ElideRight
            }
        }
        QQC2.Label {
            objectName: "rate"
            Layout.rightMargin: Kirigami.Units.largeSpacing
            visible: outputMenu.rate !== ""
            text: outputMenu.rate
            font: KanteStyle.active ? KanteStyle.monoFont(KanteStyle.smallFont.pointSize) : KanteStyle.smallFont
            color: KanteStyle.mutedTextColor
        }
    }

    KanteMenuItem {
        text: outputMenu.stoppable ? qsTr("Stop…") : qsTr("Start")
        icon.name: outputMenu.stoppable ? "media-playback-stop" : "media-playback-start"
        onTriggered: outputMenu.stoppable ? outputMenu.stopRequested() : outputMenu.startRequested()
    }
    KanteMenuItem {
        text: qsTr("Settings…")
        icon.name: "configure"
        onTriggered: outputMenu.settingsRequested()
    }
    KanteMenuItem {
        objectName: "copyAddress"
        text: qsTr("Copy address")
        icon.name: "edit-copy"
        valueText: outputMenu.shortAddress(outputMenu.address)
        visible: outputMenu.address !== ""
        height: visible ? implicitHeight : 0
        onTriggered: outputMenu.copy(outputMenu.address)
    }
    // Read only: the figures while it sends.
    QQC2.Menu {
        id: stats
        title: qsTr("Statistics")
        readonly property string iconName: "view-statistics"
        enabled: outputMenu.sending
        KantePopupSkin { popup: stats }
        KanteMenuItem { text: qsTr("Running"); valueText: outputMenu.elapsed }
        KanteMenuItem { text: qsTr("Rate"); valueText: outputMenu.rate }
        KanteMenuItem {
            text: qsTr("Packets lost")
            visible: !!outputMenu.link
            height: visible ? implicitHeight : 0
            valueText: outputMenu.link ? String(outputMenu.link.packets_lost) : ""
        }
        KanteMenuItem {
            text: qsTr("Packets dropped")
            visible: !!outputMenu.link
            height: visible ? implicitHeight : 0
            valueText: outputMenu.link ? String(outputMenu.link.packets_dropped) : ""
        }
        KanteMenuItem {
            text: qsTr("Round trip")
            visible: !!outputMenu.link
            height: visible ? implicitHeight : 0
            valueText: outputMenu.link && outputMenu.link.rtt_ms !== null && outputMenu.link.rtt_ms !== undefined
                ? qsTr("%1 ms").arg(outputMenu.link.rtt_ms) : "–"
        }
    }

    QQC2.MenuSeparator {}
    // Not while it sends (M07): stop it first, the line says so.
    KanteMenuItem {
        objectName: "remove"
        text: qsTr("Remove…")
        icon.name: "edit-delete"
        destructive: true
        enabled: !outputMenu.sending
        valueText: outputMenu.sending ? qsTr("sending") : ""
        onTriggered: outputMenu.removeRequested()
    }
    QQC2.Label {
        objectName: "removeHint"
        visible: outputMenu.sending
        height: visible ? implicitHeight : 0
        width: parent ? parent.width : implicitWidth
        leftPadding: Kirigami.Units.largeSpacing
        rightPadding: Kirigami.Units.largeSpacing
        bottomPadding: Kirigami.Units.smallSpacing
        text: qsTr("Stop it first, then remove it.")
        color: KanteStyle.mutedTextColor
        font.pointSize: Kirigami.Theme.smallFont.pointSize
    }
}
