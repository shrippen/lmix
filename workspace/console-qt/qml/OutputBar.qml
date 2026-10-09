import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Outputs: one button each (click starts or stops; right click removes),
 * state as a pill, running time and link health. "+ Output" adds one.
 */
RowLayout {
    id: bar

    required property var core
    required property var show
    required property var act
    required property var outputs
    required property var outputById
    signal addRequested()

    readonly property var links: core.links.length > 0 ? JSON.parse(core.links) : ({})

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
            text: modelData.name + (running ? "  " + elapsed : "") + (link ? "  " + link.rate : "")
            emphasis: running ? KanteButton.Emphasis.Primary
                : (status.state === "failed" ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal)
            enabled: bar.core.connected
            onClicked: bar.act({ action: running ? "stop_output" : "start_output", output: modelData.number })

            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: {
                var state = running ? qsTr("Running") : (status.state === "failed" ? qsTr("Failed: %1").arg(status.message) : qsTr("Stopped"))
                return state + (link ? "\n" + link.detail : "") + "\n" + qsTr("Click: start or stop. Right click: remove.")
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

    KanteButton {
        text: qsTr("+ Output")
        enabled: bar.core.connected
        onClicked: bar.addRequested()
    }

    Item { Layout.fillWidth: true }
}
