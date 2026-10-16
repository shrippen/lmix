import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Cue lists (AUT.14): numbered cues run one after another with GO; Back
 * runs the one before, a click on a cue makes it the next. Edits go to
 * lmix-core at once and keep the place in the list.
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

    function save(cues) {
        act({ action: "set_cue_list", list: list.number, name: list.name, cues: cues })
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
            cues.splice(dialog.editing, dialog.editing < cues.length ? 1 : 0, { label: label, actions: actions })
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
                        readonly property bool isNext: dialog.list.next === index + 1
                        Layout.fillWidth: true
                        implicitHeight: cueRow.implicitHeight + Kirigami.Units.smallSpacing
                        color: isNext ? KanteStyle.selectionColor : "transparent"

                        RowLayout {
                            id: cueRow
                            anchors.fill: parent
                            anchors.leftMargin: Kirigami.Units.smallSpacing

                            QQC2.Label {
                                Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                                text: (entry.isNext ? "▶ " : "") + (entry.index + 1)
                                font.family: KanteStyle.monoFamily
                            }
                            QQC2.Label {
                                Layout.fillWidth: true
                                text: entry.modelData.label
                                elide: Text.ElideRight
                            }
                            QQC2.Label {
                                text: qsTr("Actions: %1").arg(entry.modelData.actions.length)
                                color: KanteStyle.mutedTextColor
                            }
                            KanteButton {
                                text: qsTr("Next")
                                enabled: !entry.isNext
                                QQC2.ToolTip.visible: hovered
                                QQC2.ToolTip.text: qsTr("Make this the cue GO runs next")
                                onClicked: dialog.act({ action: "cue_jump", list: dialog.list.number, cue: entry.index + 1 })
                            }
                            KanteButton {
                                text: qsTr("Edit…")
                                onClicked: {
                                    dialog.editing = entry.index
                                    cueDialog.label = entry.modelData.label
                                    cueDialog.actions = entry.modelData.actions
                                    cueDialog.open()
                                }
                            }
                            KanteButton {
                                text: qsTr("Remove")
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
                onClicked: {
                    dialog.editing = dialog.list.cues.length
                    cueDialog.label = qsTr("Cue %1").arg(dialog.list.cues.length + 1)
                    cueDialog.actions = []
                    cueDialog.open()
                }
            }
            Item { Layout.fillWidth: true }
            KanteButton {
                text: qsTr("Back")
                enabled: dialog.list !== null && dialog.list.next > 2
                onClicked: dialog.act({ action: "cue_back", list: dialog.list.number })
            }
            KanteButton {
                text: dialog.list && dialog.list.next <= dialog.list.cues.length
                    ? qsTr("GO: %1").arg(dialog.list.cues[dialog.list.next - 1].label) : qsTr("GO")
                emphasis: KanteButton.Emphasis.Primary
                enabled: dialog.list !== null && dialog.list.next <= dialog.list.cues.length
                onClicked: dialog.act({ action: "cue_go", list: dialog.list.number })
            }
        }
    }
}
