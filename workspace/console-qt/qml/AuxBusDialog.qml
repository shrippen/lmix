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
    /** Levels as the bridge gives them (`aux1`, `aux2`). */
    property var levels: ({})

    title: qsTr("Master and aux buses")
    modal: true
    width: Kirigami.Units.gridUnit * 30
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel { text: qsTr("Master · program sound") }
        KanteAudioMeter {
            Layout.fillWidth: true
            level: dialog.levels.program !== undefined ? dialog.levels.program : -100
            scale: true
        }
        RowLayout {
            Layout.fillWidth: true
            QQC2.Slider {
                id: masterVolume
                Layout.fillWidth: true
                from: 0
                to: 100
                stepSize: 1
                value: dialog.master.volume
                Accessible.name: qsTr("Master volume")
                onMoved: dialog.act({ action: "set_master", volume: Math.round(value), mute: dialog.master.mute })
                KanteSliderSkin { control: masterVolume }
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                text: masterVolume.value.toFixed(0)
                font.family: KanteStyle.monoFamily
            }
            QQC2.Switch {
                id: masterMute
                text: qsTr("Mute")
                checked: dialog.master.mute
                onToggled: dialog.act({ action: "set_master", volume: dialog.master.volume, mute: checked })
                KanteCheckSkin { control: masterMute; shape: KanteCheckSkin.Shape.Switch }
            }
        }

        Repeater {
            model: dialog.buses

            ColumnLayout {
                id: bus
                required property int index
                required property var modelData
                readonly property int number: index + 1
                Layout.fillWidth: true

                KanteSectionLabel {
                    text: bus.index === 0 ? qsTr("Aux 1 · before the faders") : qsTr("Aux 2 · after the faders")
                }
                KanteAudioMeter {
                    Layout.fillWidth: true
                    level: dialog.levels["aux" + bus.number] !== undefined ? dialog.levels["aux" + bus.number] : -100
                    scale: true
                }
                RowLayout {
                    Layout.fillWidth: true
                    QQC2.Slider {
                        id: volume
                        Layout.fillWidth: true
                        from: 0
                        to: 100
                        stepSize: 1
                        value: bus.modelData.volume
                        Accessible.name: qsTr("Volume of aux %1").arg(bus.number)
                        onMoved: dialog.act({ action: "set_aux_bus", bus: bus.number, volume: Math.round(value), mute: bus.modelData.mute })
                        KanteSliderSkin { control: volume }
                    }
                    QQC2.Label {
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                        text: volume.value.toFixed(0)
                        font.family: KanteStyle.monoFamily
                    }
                    QQC2.Switch {
                        id: mute
                        text: qsTr("Mute")
                        checked: bus.modelData.mute
                        onToggled: dialog.act({ action: "set_aux_bus", bus: bus.number, volume: bus.modelData.volume, mute: checked })
                        KanteCheckSkin { control: mute; shape: KanteCheckSkin.Shape.Switch }
                    }
                }
            }
        }
    }
}
