import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Edits a list of actions, optionally with a label (a cue's). Does not send
 * anything itself: `done` hands the result to whoever opened it.
 */
KanteDialog {
    id: dialog

    required property var inputs
    required property var outputs
    /** Shows the label field. */
    property bool labelled: false
    /** OK needs at least one action (cues do). */
    property bool needsAction: false
    /** Set before `open()`. */
    property string label: ""
    property var actions: []

    signal done(string label, var actions)

    modal: true
    width: Kirigami.Units.gridUnit * 38
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: qsTr("Apply")
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: {
        labelField.text = label
        rows.load(actions)
    }

    acceptEnabled: rows.error === "" && (!needsAction || rows.filled > 0) && (!labelled || labelField.text.trim() !== "")

    onAccepted: done(labelField.text.trim(), rows.collect())

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteTextField {
            id: labelField
            Layout.fillWidth: true
            visible: dialog.labelled
            placeholderText: qsTr("Label")
        }

        ActionRows {
            id: rows
            Layout.fillWidth: true
            inputs: dialog.inputs
            outputs: dialog.outputs
        }
    }
}
