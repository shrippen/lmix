import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Asks before a running output stops (UI.10): one output, or a group
 * (`streams`, `recordings`, `all`). The question can turn itself off.
 */
KanteDialog {
    id: dialog

    required property var act
    /** The output to stop; null stops `group`. */
    property var output: null
    property string group: "streams"
    /** The user's answer to "Ask before stopping", after OK. */
    signal askAgainChosen(bool on)

    modal: true
    title: output ? qsTr("Stop %1?").arg(output.name)
        : (group === "recordings" ? qsTr("Stop all recordings?")
        : (group === "all" ? qsTr("Stop all outputs?") : qsTr("Stop all streams?")))
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    kanteBarColor: KanteStyle.negativeTextColor
    width: Kirigami.Units.gridUnit * 24
    // KanteDialog has a child of its own, so it does not size to this.
    contentHeight: stopColumn.implicitHeight
    onOpened: askAgain.checked = true
    onAccepted: {
        dialog.askAgainChosen(askAgain.checked)
        // By id: another window may renumber the outputs meanwhile.
        dialog.act(output ? { action: "stop_output", output: output.id }
            : { action: "stop_outputs", group: group })
    }

    ColumnLayout {
        id: stopColumn
        width: parent.width
        QQC2.Label {
            Layout.fillWidth: true
            text: qsTr("A stopped recording or stream cannot go on where it was.")
            wrapMode: Text.WordWrap
        }
        QQC2.CheckBox {
            id: askAgain
            text: qsTr("Ask before stopping")
            KanteCheckSkin { control: askAgain }
        }
    }
}
