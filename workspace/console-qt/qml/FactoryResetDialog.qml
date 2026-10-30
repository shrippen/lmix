import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Asks before the settings of this machine go back to the factory
 * (docs/DESIGN-DIALOGE.md F12): the question as the title, red, and the
 * button only once the word is typed. lmix-core cannot undo it, so the way
 * to a backup comes first.
 */
KanteDialog {
    id: dialog

    required property var act
    /** Export first: the shell opens its export dialog. */
    signal backupRequested()
    /** Typed to unlock the button. */
    readonly property string word: qsTr("reset")

    title: qsTr("Reset to factory settings?")
    modal: true
    width: Kirigami.Units.gridUnit * 24
    kanteBarColor: KanteStyle.negativeTextColor
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: qsTr("Reset settings")
    acceptEnabled: typed.text.trim().toLowerCase() === word.toLowerCase()
    // KanteDialog has a child of its own, so it does not size to this.
    contentHeight: column.implicitHeight
    onOpened: {
        typed.text = ""
        typed.forceActiveFocus()
    }
    onAccepted: dialog.act({ action: "reset_settings" })

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.largeSpacing
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: qsTr("Removes the shortcuts for all shows, the layout templates, the output targets with their secrets and the OSC subscribers of this machine. The show stays. This cannot be undone.")
        }
        KanteButton {
            text: qsTr("Export settings first…")
            icon.name: "document-export"
            onClicked: {
                dialog.close()
                dialog.backupRequested()
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            textFormat: Text.StyledText
            text: qsTr("Type %1 to confirm.").arg("<b>" + dialog.word + "</b>")
        }
        KanteTextField {
            id: typed
            Layout.fillWidth: true
            Accessible.name: qsTr("Confirmation word")
            onAccepted: {
                if (dialog.acceptEnabled) {
                    dialog.accept()
                }
            }
        }
    }
}
