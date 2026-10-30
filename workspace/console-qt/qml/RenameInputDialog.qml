import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Renames an input (docs/DESIGN-DIALOGE.md, F13): one dialog for all
 * tiles, small, the name selected so typing replaces it, Enter renames.
 */
KanteDialog {
    id: dialog

    required property var act
    /** lmix `Input`s of the show. */
    required property var inputs
    /** Id of the input to rename; set before `open()`. */
    property string inputId: ""
    readonly property var input: StatePatch.byId(inputs, inputId) || null
    readonly property string wanted: nameField.text.trim()

    modal: true
    title: input ? qsTr("Rename input %1").arg(input.number) : ""
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: qsTr("Rename")
    acceptEnabled: input !== null && wanted.length > 0
    onOpened: {
        nameField.text = input ? input.name : ""
        nameField.forceActiveFocus()
        nameField.selectAll()
    }
    onAccepted: {
        if (input && wanted.length > 0 && wanted !== input.name) {
            dialog.act({ action: "rename_input", input: input.id, name: wanted })
        }
    }
    // Removed while open: nothing left to rename.
    onInputChanged: {
        if (visible && input === null) {
            close()
        }
    }

    KanteTextField {
        id: nameField
        width: Kirigami.Units.gridUnit * 18
        Accessible.name: qsTr("Name")
        onAccepted: {
            if (dialog.acceptEnabled) {
                dialog.accept()
            }
        }
    }
}
