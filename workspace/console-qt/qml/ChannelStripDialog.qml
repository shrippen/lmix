import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Channel strip of an input in a dialog of its own (see ChannelStripPanel).
 */
KanteDialog {
    id: dialog

    required property var act
    /** The input whose strip is edited; set before `open()`. */
    property var input: null

    title: input ? qsTr("Channel strip of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 34
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself; the strip is taller than small windows and scrolls.
    contentHeight: Math.min(panel.implicitHeight, (parent ? parent.height : 800) * 0.7)
    onOpened: panel.load()

    QQC2.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ChannelStripPanel {
            id: panel
            width: scroll.availableWidth
            act: dialog.act
            input: dialog.input
        }
    }
}
