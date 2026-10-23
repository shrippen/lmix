import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The master of program sound (AUD.2), the monitor (AUD.4) and the
 * masters of the aux buses: level, volume and mute. Bus 1 takes the sends
 * before the channel faders (monitor mix), bus 2 after them. What each
 * input sends is set in its channel strip. On the Audio page and in the
 * buses dialog.
 */
Item {
    id: panel

    required property var act
    /** `aux_buses` of the show state. */
    property var buses: []
    /** `master` of the show state. */
    property var master: ({ volume: 100, mute: false })
    /** `monitor_volume` of the show state. */
    property int monitorVolume: 100
    /** Levels as the bridge gives them (`aux1` … `aux7`). */
    property var levels: ({})
    readonly property real contentHeight: column.implicitHeight

    QQC2.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            id: column
            width: scroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            KanteSectionLabel { text: qsTr("Master · program sound") }
            KanteAudioMeter {
                Layout.fillWidth: true
                level: panel.levels.program !== undefined ? panel.levels.program : -100
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
                    value: panel.master.volume
                    Accessible.name: qsTr("Master volume")
                    onMoved: panel.act({ action: "set_master", volume: Math.round(value), mute: panel.master.mute })
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
                    checked: panel.master.mute
                    onToggled: panel.act({ action: "set_master", volume: panel.master.volume, mute: checked })
                    KanteCheckSkin { control: masterMute; shape: KanteCheckSkin.Shape.Switch }
                }
            }

            // What solo listening and the headphones hear (AUD.4).
            KanteSectionLabel { text: qsTr("Monitor · listening") }
            KanteAudioMeter {
                Layout.fillWidth: true
                level: panel.levels.monitor !== undefined ? panel.levels.monitor : -100
                scale: true
            }
            RowLayout {
                Layout.fillWidth: true
                QQC2.Slider {
                    id: monitorVolume
                    Layout.fillWidth: true
                    from: 0
                    to: 100
                    stepSize: 1
                    value: panel.monitorVolume
                    Accessible.name: qsTr("Monitor volume")
                    onMoved: panel.act({ action: "set_monitor_volume", volume: Math.round(value) })
                    KanteSliderSkin { control: monitorVolume }
                }
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                    text: monitorVolume.value.toFixed(0)
                    font.family: KanteStyle.monoFamily
                }
            }

            Repeater {
                model: panel.buses

                ColumnLayout {
                    id: bus
                    required property int index
                    required property var modelData
                    readonly property int number: index + 1
                    Layout.fillWidth: true

                    KanteSectionLabel {
                        text: Texts.auxName(bus.index)
                    }
                    KanteAudioMeter {
                        Layout.fillWidth: true
                        level: panel.levels["aux" + bus.number] !== undefined ? panel.levels["aux" + bus.number] : -100
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
                            onMoved: panel.act({ action: "set_aux_bus", bus: bus.number, volume: Math.round(value), mute: bus.modelData.mute })
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
                            onToggled: panel.act({ action: "set_aux_bus", bus: bus.number, volume: bus.modelData.volume, mute: checked })
                            KanteCheckSkin { control: mute; shape: KanteCheckSkin.Shape.Switch }
                        }
                    }
                }
            }
        }
    }
}
