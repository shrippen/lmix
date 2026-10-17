import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Cue lists (AUT.14, show mode CUE/RUN): GO starts the standby cue and
 * its follow chain; Back and Standby move standby without starting
 * anything. Only actions cues are edited here; waits, starts and groups
 * (e.g. from a script) are shown and kept. Edits go to lmix-core at once
 * and keep standby and running cues.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    required property var outputs
    required property var lists

    /** Shown list, by position. */
    property int current: 0
    readonly property var list: lists.length > 0 ? lists[Math.min(current, lists.length - 1)] : null
    /** Cue whose actions are being edited; past the end for a new one. */
    property int editing: -1
    /** The list comes from a script and is edited there (IMP.9). */
    readonly property bool fromScript: list !== null && list.source !== undefined && list.source !== null

    function save(cues) {
        act({ action: "set_cue_list", list: list.number, name: list.name, cues: cues })
    }

    /** The cue's name, or its kind when it has none. */
    function cueName(cue) {
        return cue.name !== "" ? cue.name : cue.kind
    }

    title: qsTr("Cue lists")
    modal: true
    width: Kirigami.Units.gridUnit * 36
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    ActionListDialog {
        id: cueDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        inputs: dialog.inputs
        outputs: dialog.outputs
        labelled: true
        needsAction: true
        title: dialog.list && dialog.editing < dialog.list.cues.length ? qsTr("Cue %1").arg(dialog.editing + 1) : qsTr("New cue")
        onDone: (label, actions) => {
            var cues = dialog.list.cues.slice()
            var old = dialog.editing < cues.length ? cues[dialog.editing] : {}
            cues.splice(dialog.editing, dialog.editing < cues.length ? 1 : 0,
                        Object.assign({}, old, { kind: "actions", name: label, actions: actions }))
            dialog.save(cues)
        }
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            QQC2.ComboBox {
                id: listBox
                Layout.fillWidth: true
                visible: dialog.lists.length > 0
                model: dialog.lists.map(l => l.number + " " + l.name)
                currentIndex: dialog.current
                onActivated: index => dialog.current = index
                KanteFieldSkin { control: listBox }
            }
            KanteTextField {
                id: newName
                Layout.fillWidth: dialog.lists.length === 0
                placeholderText: qsTr("New list")
            }
            KanteButton {
                text: qsTr("+ List")
                enabled: newName.text.trim() !== ""
                onClicked: {
                    dialog.act({ action: "add_cue_list", name: newName.text.trim(), cues: [] })
                    dialog.current = dialog.lists.length
                    newName.text = ""
                }
            }
            KanteButton {
                text: qsTr("Remove list")
                visible: dialog.list !== null
                onClicked: dialog.act({ action: "remove_cue_list", list: dialog.list.number })
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.fromScript
            wrapMode: Text.WordWrap
            color: KanteStyle.mutedTextColor
            text: dialog.fromScript ? qsTr("This list comes from %1 and is edited there; reload the script to change it.").arg(dialog.list.source.replace(/^script:/, "")) : ""
        }

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.list === null
            wrapMode: Text.WordWrap
            text: qsTr("No cue lists yet. A cue list runs prepared steps of a show one after another, each with GO.")
        }

        QQC2.ScrollView {
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(cueColumn.implicitHeight, Kirigami.Units.gridUnit * 18)
            visible: dialog.list !== null

            ColumnLayout {
                id: cueColumn
                width: parent.width
                spacing: 0

                Repeater {
                    model: dialog.list ? dialog.list.cues : []

                    Rectangle {
                        id: entry
                        required property var modelData
                        required property int index
                        readonly property bool isNext: dialog.list.standby === index + 1
                        readonly property bool editable: modelData.kind === "actions"
                        Layout.fillWidth: true
                        implicitHeight: cueRow.implicitHeight + Kirigami.Units.smallSpacing
                        color: isNext ? KanteStyle.selectionColor : "transparent"

                        RowLayout {
                            id: cueRow
                            anchors.fill: parent
                            anchors.leftMargin: Kirigami.Units.smallSpacing

                            QQC2.Label {
                                Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                                text: (entry.isNext ? "▶ " : "") + (entry.modelData.number !== "" ? entry.modelData.number : entry.index + 1)
                                font.family: KanteStyle.monoFamily
                            }
                            QQC2.Label {
                                Layout.fillWidth: true
                                text: dialog.cueName(entry.modelData)
                                elide: Text.ElideRight
                            }
                            QQC2.Label {
                                text: entry.editable ? qsTr("Actions: %1").arg(entry.modelData.actions.length) : entry.modelData.kind
                                color: KanteStyle.mutedTextColor
                            }
                            KanteButton {
                                text: qsTr("Standby")
                                enabled: !entry.isNext
                                QQC2.ToolTip.visible: hovered
                                QQC2.ToolTip.text: qsTr("Make this the cue GO starts next")
                                onClicked: dialog.act({ action: "cue_standby", list: dialog.list.number, to: { index: entry.index + 1 } })
                            }
                            KanteButton {
                                text: qsTr("Edit…")
                                enabled: entry.editable && !dialog.fromScript
                                onClicked: {
                                    dialog.editing = entry.index
                                    cueDialog.label = entry.modelData.name
                                    cueDialog.actions = entry.modelData.actions
                                    cueDialog.open()
                                }
                            }
                            KanteButton {
                                text: qsTr("Remove")
                                enabled: !dialog.fromScript
                                onClicked: {
                                    var cues = dialog.list.cues.slice()
                                    cues.splice(entry.index, 1)
                                    dialog.save(cues)
                                }
                            }
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: dialog.list !== null

            KanteButton {
                text: qsTr("+ Cue")
                enabled: !dialog.fromScript
                onClicked: {
                    dialog.editing = dialog.list.cues.length
                    cueDialog.label = qsTr("Cue %1").arg(dialog.list.cues.length + 1)
                    cueDialog.actions = []
                    cueDialog.open()
                }
            }
            Item { Layout.fillWidth: true }
            KanteButton {
                text: qsTr("Stop")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Stop the running cues of this list")
                onClicked: dialog.act({ action: "cue_stop", list: dialog.list.number })
            }
            KanteButton {
                text: qsTr("Back")
                enabled: dialog.list !== null && dialog.list.standby > 1
                onClicked: dialog.act({ action: "cue_standby", list: dialog.list.number, to: "previous" })
            }
            KanteButton {
                text: dialog.list && dialog.list.standby <= dialog.list.cues.length
                    ? qsTr("GO: %1").arg(dialog.cueName(dialog.list.cues[dialog.list.standby - 1])) : qsTr("GO")
                emphasis: KanteButton.Emphasis.Primary
                enabled: dialog.list !== null && dialog.list.standby <= dialog.list.cues.length
                onClicked: dialog.act({ action: "cue_go", list: dialog.list.number })
            }
        }
    }
}
