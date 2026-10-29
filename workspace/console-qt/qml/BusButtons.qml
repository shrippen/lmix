import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import Kante

/**
 * Buttons: the program row cuts the input straight on, the preview row
 * puts it into preview (SWI.1, SWI.2).
 */
ColumnLayout {
    id: rows

    required property var show
    required property var inputs
    required property var act
    property bool connected: true

    spacing: Kirigami.Units.smallSpacing

    Repeater {
        model: [
            { label: qsTr("Program"), bus: "program", color: KanteStyle.negativeTextColor },
            { label: qsTr("Preview"), bus: "preview", color: KanteStyle.positiveTextColor }
        ]
        delegate: RowLayout {
            id: busRow
            required property var modelData
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing
            KanteSectionLabel {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                text: busRow.modelData.label
                color: busRow.modelData.color
            }
            Flow {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: rows.inputs
                    delegate: KanteButton {
                        required property var modelData
                        readonly property bool on: rows.show !== null && rows.show[busRow.modelData.bus] === modelData.id
                        width: Kirigami.Units.gridUnit * 6
                        height: Kirigami.Units.gridUnit * 2.5
                        text: modelData.number + " " + modelData.name
                        emphasis: on ? (busRow.modelData.bus === "program" ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Primary)
                            : KanteButton.Emphasis.Normal
                        enabled: rows.connected
                        Accessible.name: busRow.modelData.label + " " + modelData.number + " " + modelData.name
                        onClicked: rows.act(busRow.modelData.bus === "program"
                            ? { action: "cut_direct", input: modelData.number }
                            : { action: "preview_input", input: modelData.number })
                    }
                }
            }
        }
    }
}
