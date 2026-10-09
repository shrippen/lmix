import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/** New input, or (with `replacing` set to an input) a new source for it. */
KanteDialog {
    id: dialog

    required property var core
    /** The input whose source is replaced; null adds a new input. */
    property var replacing: null

    title: replacing ? qsTr("Replace source of %1").arg(replacing.name) : qsTr("Add input")
    modal: true
    anchors.centerIn: parent
    width: Kirigami.Units.gridUnit * 26
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel

    onOpened: {
        core.refreshLists()
        fields.reset()
        nameField.text = ""
    }

    onAccepted: {
        if (replacing) {
            core.replaceSource(replacing.number, fields.kind, fields.value)
        } else {
            core.addInput(fields.kind, fields.value, nameField.text)
        }
    }

    Component.onCompleted: standardButton(QQC2.Dialog.Ok).enabled = Qt.binding(() => fields.complete)

    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel { text: qsTr("Source") }
        SourceFields {
            id: fields
            Layout.fillWidth: true
            core: dialog.core
        }
        KanteSectionLabel {
            visible: !dialog.replacing
            text: qsTr("Name")
        }
        KanteTextField {
            id: nameField
            visible: !dialog.replacing
            Layout.fillWidth: true
            placeholderText: qsTr("optional")
        }
    }
}
