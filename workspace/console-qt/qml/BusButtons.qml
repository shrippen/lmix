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

    // Keyed by input: a state update hands over every input as a new
    // object, and a Repeater on the list itself made all buttons anew.
    KeyedModel { id: inputKeys }
    // Ids from ShowState: anew only when inputs come, go or move.
    readonly property var inputIds: show && show.inputIds ? show.inputIds : inputs.map(i => i.id)
    onInputIdsChanged: inputKeys.sync(inputIds)
    Component.onCompleted: inputKeys.sync(inputIds)
    function inputOf(key) {
        return (show && show.inputById ? show.inputById(key) : StatePatch.byId(inputs, key)) || null
    }

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
                    model: inputKeys
                    delegate: KanteButton {
                        required property string key
                        readonly property var input: rows.inputOf(key) || ({ id: key, number: 0, name: "" })
                        readonly property bool on: rows.show !== null && rows.show[busRow.modelData.bus] === key
                        width: Kirigami.Units.gridUnit * 6
                        height: Kirigami.Units.gridUnit * 2.5
                        text: input.number + " " + input.name
                        emphasis: on ? (busRow.modelData.bus === "program" ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Primary)
                            : KanteButton.Emphasis.Normal
                        enabled: rows.connected
                        Accessible.name: busRow.modelData.label + " " + input.number + " " + input.name
                        onClicked: rows.act(busRow.modelData.bus === "program"
                            ? { action: "cut_direct", input: input.number }
                            : { action: "preview_input", input: input.number })
                    }
                }
            }
        }
    }
}
