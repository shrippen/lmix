import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Asks before a running output stops (UI.10): one output, or a group
 * (`streams`, `recordings`, `all`), or `media`: every recording and
 * stream, leaving screens and NDI; the page stops those (mediaRequested).
 * The question can turn itself off.
 */
KanteDialog {
    id: dialog

    required property var act
    /** The output to stop; null stops `group`. */
    property var output: null
    property string group: "streams"
    /** The user's answer to "Ask before stopping", after OK. */
    signal askAgainChosen(bool on)
    /** OK for `media`: the page stops its recordings and streams. */
    signal mediaRequested()

    modal: true
    title: output ? qsTr("Stop %1?").arg(output.name)
        : (group === "recordings" ? qsTr("Stop all recordings?")
        : group === "media" ? qsTr("Stop all recordings and streams?")
        : (group === "all" ? qsTr("Stop all outputs?") : qsTr("Stop all streams?")))
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: qsTr("Stop")
    kanteBarColor: KanteStyle.negativeTextColor
    width: Kirigami.Units.gridUnit * 24
    // KanteDialog has a child of its own, so it does not size to this.
    contentHeight: stopColumn.implicitHeight
    onOpened: askAgain.checked = true
    onAccepted: {
        dialog.askAgainChosen(askAgain.checked)
        // By id: another window may renumber the outputs meanwhile.
        if (output) {
            dialog.act({ action: "stop_output", output: output.id })
        } else if (group === "media") {
            dialog.mediaRequested()
        } else {
            dialog.act({ action: "stop_outputs", group: group })
        }
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
