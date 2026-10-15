import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Triggers of an input (AUT.7): when it goes on program or into preview,
 * leaves program, or comes in or out as an overlay, run actions after a
 * delay. Edited as a working copy; OK sends all of them.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    required property var outputs
    /** The input whose triggers are edited; set before `open()`. */
    property var input: null
    /** Working copy: [{ on, delay_ms, actions }]. */
    property var triggers: []

    readonly property int maxTriggers: 8
    readonly property var events: [
        { on: "program", label: qsTr("Goes on program") },
        { on: "program_off", label: qsTr("Leaves program") },
        { on: "preview", label: qsTr("Goes into preview") },
        { on: "overlay_in", label: qsTr("Comes in as overlay") },
        { on: "overlay_out", label: qsTr("Goes out as overlay") }
    ]
    /** Index of the trigger whose actions are being edited; past the end
     * for a new one. */
    property int editing: -1

    function edited(index, field, value) {
        var list = triggers.slice()
        list[index] = Object.assign({}, list[index])
        list[index][field] = value
        triggers = list
    }

    title: input ? qsTr("Triggers of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 36
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: triggers = JSON.parse(JSON.stringify(input.triggers || []))
    onAccepted: act({ action: "set_triggers", input: input.number, triggers: triggers })

    ActionListDialog {
        id: actionsDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        inputs: dialog.inputs
        outputs: dialog.outputs
        title: qsTr("Actions of trigger %1").arg(dialog.editing + 1)
        needsAction: true
        onDone: (label, actions) => {
            if (dialog.editing < dialog.triggers.length)
                dialog.edited(dialog.editing, "actions", actions)
            else
                dialog.triggers = dialog.triggers.concat([{ on: "program", delay_ms: 0, actions: actions }])
        }
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.triggers.length === 0
            wrapMode: Text.WordWrap
            text: qsTr("No triggers. A trigger runs actions when this input goes on air or off air, also after a delay. Actions run by triggers fire no triggers themselves.")
        }

        Repeater {
            model: dialog.triggers

            RowLayout {
                id: entry
                required property var modelData
                required property int index
                Layout.fillWidth: true

                QQC2.ComboBox {
                    id: whenBox
                    Layout.fillWidth: true
                    model: dialog.events.map(e => e.label)
                    currentIndex: Math.max(0, dialog.events.findIndex(e => e.on === entry.modelData.on))
                    Accessible.name: qsTr("When trigger %1 fires").arg(entry.index + 1)
                    onActivated: index => dialog.edited(entry.index, "on", dialog.events[index].on)
                    KanteFieldSkin { control: whenBox }
                }
                QQC2.Label { text: qsTr("after") }
                QQC2.SpinBox {
                    id: delay
                    from: 0
                    to: 30000
                    stepSize: 100
                    editable: true
                    value: entry.modelData.delay_ms || 0
                    Accessible.name: qsTr("Delay of trigger %1").arg(entry.index + 1)
                    onValueModified: dialog.edited(entry.index, "delay_ms", value)
                    KanteFieldSkin { control: delay }
                }
                QQC2.Label { text: "ms" }
                QQC2.Label {
                    text: qsTr("Actions: %1").arg(entry.modelData.actions.length)
                    color: KanteStyle.mutedTextColor
                }
                KanteButton {
                    text: qsTr("Edit…")
                    onClicked: {
                        dialog.editing = entry.index
                        actionsDialog.actions = entry.modelData.actions
                        actionsDialog.open()
                    }
                }
                KanteButton {
                    text: qsTr("Remove")
                    onClicked: {
                        var list = dialog.triggers.slice()
                        list.splice(entry.index, 1)
                        dialog.triggers = list
                    }
                }
            }
        }

        KanteButton {
            Layout.alignment: Qt.AlignRight
            text: qsTr("+ Trigger")
            enabled: dialog.triggers.length < dialog.maxTriggers
            // Actions first: a trigger needs at least one.
            onClicked: {
                dialog.editing = dialog.triggers.length
                actionsDialog.actions = []
                actionsDialog.open()
            }
        }
    }
}
