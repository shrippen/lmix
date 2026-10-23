import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The master of program sound (AUD.2) and the masters of the aux buses:
 * level, volume and mute. Bus 1 takes the sends before the channel faders
 * (monitor mix), bus 2 after them. What each input sends is set in its
 * channel strip.
 */
KanteDialog {
    id: dialog

    required property var act
    /** `aux_buses` of the show state. */
    property var buses: []
    /** `master` of the show state. */
    property var master: ({ volume: 100, mute: false })
    /** `monitor_volume` of the show state. */
    property int monitorVolume: 100
    /** Levels as the bridge gives them (`aux1` … `aux7`). */
    property var levels: ({})

    title: qsTr("Master and aux buses")
    modal: true
    width: Kirigami.Units.gridUnit * 30
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one; seven aux buses (AUD.2) are taller than
    // small windows and scroll.
    contentHeight: Math.min(panel.contentHeight, (parent ? parent.height : 800) * 0.7)

    AuxBusPanel {
        id: panel
        anchors.fill: parent
        act: dialog.act
        buses: dialog.buses
        master: dialog.master
        monitorVolume: dialog.monitorVolume
        levels: dialog.levels
    }
}
