import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The master, the headphones and the buses of the show (`BusPanel`).
 */
KanteDialog {
    id: dialog

    required property var act
    required property var core
    /** `buses` of the show state. */
    property var buses: []
    /** `master` of the show state. */
    property var master: ({ volume: 100, mute: false })
    /** `phones` of the show state. */
    property var phones: ({ volume: 100, mute: false, pre_fader: false })
    /** Levels as the bridge gives them. */
    property var levels: ({})

    title: qsTr("Master, headphones and buses")
    modal: true
    width: Kirigami.Units.gridUnit * 30
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one; many buses are taller than small windows and
    // scroll.
    contentHeight: Math.min(panel.contentHeight, (parent ? parent.height : 800) * 0.7)

    BusPanel {
        id: panel
        anchors.fill: parent
        act: dialog.act
        core: dialog.core
        buses: dialog.buses
        master: dialog.master
        phones: dialog.phones
        levels: dialog.levels
    }
}
