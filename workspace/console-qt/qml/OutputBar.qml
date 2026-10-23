import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Outputs: one button each (click starts or stops; right click removes),
 * state as a pill, running time and link health. "+ Output" adds one.
 * Stopping a running output asks first unless `confirmStop` is off
 * (UI.10); the question can turn it off. With two or more streams, one
 * button starts the stopped ones or, when all run, stops them (STR.1).
 */
RowLayout {
    id: bar

    required property var core
    required property var show
    required property var act
    required property var outputs
    required property var outputById
    signal addRequested()
    /** Ask before stopping a running output (the shell's setting). */
    property bool confirmStop: true
    /** The answer to "Ask before stopping" after a stop; the page writes
     *  the setting, so `confirmStop` stays bound to it. */
    signal confirmStopChosen(bool on)

    readonly property var links: core.links.length > 0 ? JSON.parse(core.links) : ({})
    /** SRT outputs, started and stopped together with one button (STR.1). */
    readonly property var streams: (show && show.outputs ? show.outputs : []).filter(o => o.spec && o.spec.kind === "srt")
    readonly property bool allStreaming: streams.every(o => o.status.state === "running")

    spacing: Kirigami.Units.smallSpacing

    KanteSectionLabel { text: qsTr("Outputs") }

    Repeater {
        model: bar.outputs

        KanteButton {
            id: button
            required property string key
            readonly property var modelData: bar.outputById(key) || ({ id: key, number: 0, name: "", status: { state: "stopped" } })
            readonly property var status: modelData.status
            readonly property bool running: status.state === "running"
            readonly property var link: bar.links[modelData.id]

            // Running time ticks with the clock below.
            property string elapsed: ""
            text: modelData.name + (running ? "  " + elapsed : "") + (link ? "  " + Texts.linkRate(link) : "")
            emphasis: running ? KanteButton.Emphasis.Primary
                : (status.state === "failed" ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal)
            enabled: bar.core.connected
            onClicked: {
                if (!running) {
                    bar.act({ action: "start_output", output: modelData.number })
                } else if (bar.confirmStop) {
                    stopDialog.output = modelData
                    stopDialog.open()
                } else {
                    bar.act({ action: "stop_output", output: modelData.number })
                }
            }

            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: {
                var state = running ? qsTr("Running") : (status.state === "failed" ? qsTr("Failed: %1").arg(status.message) : qsTr("Stopped"))
                return state + (link ? "\n" + Texts.linkDetail(link) : "") + "\n" + qsTr("Click: start or stop. Right click: remove.")
            }

            Timer {
                running: button.running
                repeat: true
                triggeredOnStart: true
                interval: 1000
                onTriggered: button.elapsed = bar.core.runningTime(button.status.since_ms)
            }

            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: menu.popup()
            }
            QQC2.Menu {
                id: menu
                KantePopupSkin { popup: menu }
                QQC2.MenuItem {
                    text: qsTr("Remove %1").arg(button.modelData.name)
                    onTriggered: bar.act({ action: "remove_output", output: button.modelData.number })
                }
            }
        }
    }

    StopOutputDialog {
        id: stopDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: bar.act
        onAskAgainChosen: on => bar.confirmStopChosen(on)
    }

    KanteButton {
        visible: bar.streams.length > 1
        text: bar.allStreaming ? qsTr("Stop all streams") : qsTr("Start all streams")
        enabled: bar.core.connected
        onClicked: {
            if (!bar.allStreaming) {
                bar.act({ action: "start_outputs", group: "streams" })
            } else if (bar.confirmStop) {
                stopDialog.output = null
                stopDialog.open()
            } else {
                bar.act({ action: "stop_outputs", group: "streams" })
            }
        }
    }

    KanteButton {
        text: qsTr("+ Output")
        enabled: bar.core.connected
        onClicked: bar.addRequested()
    }

    Item { Layout.fillWidth: true }
}
