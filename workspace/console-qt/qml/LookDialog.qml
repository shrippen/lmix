import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Colour correction of an input in a dialog of its own (see LookPanel).
 */
KanteDialog {
    id: dialog

    required property var act
    /** The input whose look is edited; set before `open()`. */
    property var input: null

    title: input ? qsTr("Colour of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 30
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: panel.implicitHeight
    onOpened: panel.load()

    LookPanel {
        id: panel
        width: parent.width
        spacing: 0
        act: dialog.act
        input: dialog.input
    }
}
