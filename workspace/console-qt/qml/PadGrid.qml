pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Pads (RUN.12): the cues of a pad list as buttons, no standby and no
 * GO. A press starts the cue, a press while it runs stops it
 * (`cue_toggle`); a running pad is lit. Companion presses the same with
 * OSC `/lmix/cue/toggle <list> <cue>`.
 */
ColumnLayout {
    id: grid

    required property var act
    /** The show's pad lists. */
    property var lists: []
    /** Ids of the cues that run now. */
    property var runningIds: []
    /** Width of one pad. */
    property real padWidth: Kirigami.Units.gridUnit * 9

    /** Shown list, by position. */
    property int current: 0
    readonly property var list: lists.length > 0 ? lists[Math.min(current, lists.length - 1)] : null

    spacing: Kirigami.Units.smallSpacing

    RowLayout {
        Layout.fillWidth: true
        KanteSectionLabel { text: qsTr("Pads") }
        KanteSegmented {
            visible: grid.lists.length > 1
            model: grid.lists.map(l => l.name)
            currentIndex: grid.current
            onActivated: index => grid.current = index
        }
        Item { Layout.fillWidth: true }
    }

    Flow {
        Layout.fillWidth: true
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: grid.list ? grid.list.cues : []
            delegate: ColumnLayout {
                id: pad
                required property var modelData
                readonly property bool runs: grid.runningIds.indexOf(modelData.id) >= 0
                width: grid.padWidth
                spacing: 0

                KanteButton {
                    Layout.fillWidth: true
                    focusPolicy: Qt.NoFocus
                    size: KanteButton.Size.Large
                    emphasis: pad.runs ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                    text: (pad.modelData.number !== "" ? pad.modelData.number + " " : "")
                        + (pad.modelData.name !== "" ? pad.modelData.name : pad.modelData.kind)
                    Accessible.name: pad.runs ? qsTr("Stop %1").arg(text) : qsTr("Start %1").arg(text)
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    QQC2.ToolTip.text: pad.runs ? qsTr("Runs: click to stop") : qsTr("Click to start")
                    onClicked: grid.act({ action: "cue_toggle", list: grid.list.number, cue: pad.modelData.id })
                }
                // The cue's colour (CUE.1) under its pad.
                Rectangle {
                    Layout.fillWidth: true
                    implicitHeight: Kirigami.Units.smallSpacing
                    visible: pad.modelData.color !== undefined && pad.modelData.color !== null
                    color: visible ? KanteStyle.labelColor(pad.modelData.color) : "transparent"
                }
            }
        }
    }

    QQC2.Label {
        visible: grid.list !== null && grid.list.cues.length === 0
        text: qsTr("No cues in %1 yet").arg(grid.list ? grid.list.name : "")
        color: KanteStyle.mutedTextColor
    }
}
