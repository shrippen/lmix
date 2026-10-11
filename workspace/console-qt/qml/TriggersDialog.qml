import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Triggers of an input (AUT.7): when it goes on program or into preview,
 * leaves program, or comes in or out as an overlay, run actions after a
 * delay. With `graphics` set instead: triggers of a graphics channel on
 * its templates' reports (`zero` of a countdown, `done`), optionally of
 * one layer. Edited as a working copy; OK sends all of them.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    required property var outputs
    /** The input whose triggers are edited; set before `open()`. */
    property var input: null
    /** Or the graphics channel (lmix `Graphics`). */
    property var graphics: null
    /** Working copy: [{ on, delay_ms, before_ms, actions }], for a
     * channel [{ report, layer, delay_ms, actions }]. */
    property var triggers: []

    readonly property int maxTriggers: 8
    readonly property var events: [
        { on: "program", label: qsTr("Goes on program") },
        { on: "program_off", label: qsTr("Leaves program") },
        { on: "preview", label: qsTr("Goes into preview") },
        { on: "overlay_in", label: qsTr("Comes in as overlay") },
        { on: "overlay_out", label: qsTr("Goes out as overlay") },
        { on: "clip_end", label: qsTr("Clip ends"), clip: true }
    ]
    /** Events this input offers: clip ends only for clips and playlists. */
    readonly property bool plays: input !== null && ["file", "url", "playlist"].indexOf(input.source.kind) >= 0
    readonly property var choices: events.filter(e => !e.clip || plays)
    /** Index of the trigger whose actions are being edited; past the end
     * for a new one. */
    property int editing: -1

    function edited(index, field, value) {
        var list = triggers.slice()
        list[index] = Object.assign({}, list[index])
        list[index][field] = value
        triggers = list
    }

    title: graphics ? qsTr("Triggers of graphics %1 %2").arg(graphics.number).arg(graphics.name)
        : input ? qsTr("Triggers of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 36
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: triggers = JSON.parse(JSON.stringify((graphics || input).triggers || []))
    onAccepted: graphics
        ? act({ action: "set_title_triggers", graphics: graphics.number, triggers: triggers })
        : act({ action: "set_triggers", input: input.number, triggers: triggers })

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
                dialog.triggers = dialog.triggers.concat([dialog.graphics
                    ? { report: "zero", delay_ms: 0, actions: actions }
                    : { on: "program", delay_ms: 0, actions: actions }])
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
            text: dialog.graphics
                ? qsTr("No triggers. A trigger runs actions when a title reports something, e.g. a countdown reaching zero (zero) or a title gone after stop (done).")
                : qsTr("No triggers. A trigger runs actions when this input goes on air or off air, also after a delay. Actions run by triggers fire no triggers themselves.")
        }

        Repeater {
            model: dialog.triggers

            RowLayout {
                id: entry
                required property var modelData
                required property int index
                Layout.fillWidth: true

                // A channel's trigger: which report, from which layer.
                KanteTextField {
                    id: reportField
                    visible: dialog.graphics !== null
                    Layout.fillWidth: true
                    text: entry.modelData.report || ""
                    placeholderText: qsTr("Any report")
                    Accessible.name: qsTr("Report of trigger %1").arg(entry.index + 1)
                    onEditingFinished: dialog.edited(entry.index, "report", text.trim() === "" ? null : text.trim())
                }
                QQC2.Label {
                    visible: dialog.graphics !== null
                    text: qsTr("from layer")
                }
                QQC2.SpinBox {
                    id: layerBox
                    visible: dialog.graphics !== null
                    from: 0
                    to: 8
                    value: entry.modelData.layer || 0
                    textFromValue: value => value === 0 ? qsTr("any") : String(value)
                    Accessible.name: qsTr("Layer of trigger %1").arg(entry.index + 1)
                    onValueModified: dialog.edited(entry.index, "layer", value > 0 ? value : null)
                    KanteFieldSkin { control: layerBox }
                }
                QQC2.ComboBox {
                    id: whenBox
                    visible: dialog.graphics === null
                    Layout.fillWidth: true
                    model: dialog.choices.map(e => e.label)
                    currentIndex: Math.max(0, dialog.choices.findIndex(e => e.on === entry.modelData.on))
                    Accessible.name: qsTr("When trigger %1 fires").arg(entry.index + 1)
                    onActivated: index => {
                        dialog.edited(entry.index, "on", dialog.choices[index].on)
                        if (dialog.choices[index].on !== "clip_end")
                            dialog.edited(entry.index, "before_ms", null)
                    }
                    KanteFieldSkin { control: whenBox }
                }
                // Clip ends: also this long before the end (countdown); 0 = at it.
                QQC2.SpinBox {
                    id: before
                    visible: entry.modelData.on === "clip_end"
                    from: 0
                    to: 60000
                    stepSize: 500
                    editable: true
                    value: entry.modelData.before_ms || 0
                    Accessible.name: qsTr("Time before the clip ends, trigger %1").arg(entry.index + 1)
                    onValueModified: dialog.edited(entry.index, "before_ms", value > 0 ? value : null)
                    KanteFieldSkin { control: before }
                }
                QQC2.Label {
                    visible: before.visible
                    text: qsTr("ms before,")
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
