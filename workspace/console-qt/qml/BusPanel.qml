import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The ways sound leaves the mixer (AUD.2, AUD.4): the master of program
 * sound and the headphones are always there, the show adds buses (an
 * in-ear mix, a stream without the presenter) with a name each. Level,
 * volume, mute and where the sends are taken; a bus can be listened to on
 * the headphones. What each input sends is set in its channel strip. On
 * the Audio page and in the buses dialog.
 */
Item {
    id: panel

    required property var act
    /** `buses` of the show state. */
    property var buses: []
    /** `master` of the show state. */
    property var master: ({ volume: 100, mute: false })
    /** `phones` of the show state. */
    property var phones: ({ volume: 100, mute: false, pre_fader: false })
    /** Levels as the bridge gives them (`program`, `phones`, `bus:<id>`). */
    property var levels: ({})
    readonly property real contentHeight: column.implicitHeight

    /** A meter over volume and mute. */
    component FaderRows: ColumnLayout {
        id: rows
        property real level: -100
        property int volume: 100
        property bool mute: false
        property string name
        signal changed(int volume, bool mute)
        Layout.fillWidth: true

        KanteAudioMeter {
            Layout.fillWidth: true
            level: rows.level
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
                value: rows.volume
                Accessible.name: qsTr("Volume of %1").arg(rows.name)
                onMoved: rows.changed(Math.round(value), rows.mute)
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
                checked: rows.mute
                onToggled: rows.changed(rows.volume, checked)
                KanteCheckSkin { control: mute; shape: KanteCheckSkin.Shape.Switch }
            }
        }
    }

    /** Where the sends are taken: before the channel faders or after. */
    component TapSwitch: QQC2.Switch {
        id: tap
        text: qsTr("Before the faders")
        KanteCheckSkin { control: tap; shape: KanteCheckSkin.Shape.Switch }
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        QQC2.ToolTip.text: qsTr("On: the sends ignore the channel faders and on/off (a monitor mix). Changing it rebuilds the mixer: the sound drops out briefly.")
    }

    QQC2.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            id: column
            width: scroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            KanteSectionLabel { text: qsTr("Master · program sound") }
            FaderRows {
                name: qsTr("master")
                level: panel.levels.program !== undefined ? panel.levels.program : -100
                volume: panel.master.volume
                mute: panel.master.mute
                onChanged: (volume, mute) => panel.act({ action: "set_master", volume: volume, mute: mute })
            }

            // What every input sends to the headphones, or the solos (AUD.4).
            KanteSectionLabel { text: qsTr("Headphones · listening") }
            FaderRows {
                name: qsTr("headphones")
                level: panel.levels.phones !== undefined ? panel.levels.phones : -100
                volume: panel.phones.volume
                mute: panel.phones.mute
                onChanged: (volume, mute) => panel.act({ action: "set_phones", volume: volume, mute: mute })
            }
            TapSwitch {
                checked: panel.phones.pre_fader
                onToggled: panel.act({ action: "set_phones_tap", pre_fader: checked })
            }

            Repeater {
                model: panel.buses

                ColumnLayout {
                    id: bus
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        KanteTextField {
                            id: name
                            Layout.fillWidth: true
                            text: bus.modelData.name
                            maximumLength: 40
                            Accessible.name: qsTr("Name of the bus")
                            onEditingFinished: {
                                const wanted = text.trim()
                                if (wanted.length > 0 && wanted !== bus.modelData.name)
                                    panel.act({ action: "rename_bus", bus: bus.modelData.id, name: wanted })
                                else
                                    text = bus.modelData.name
                            }
                        }
                        KanteButton {
                            size: KanteButton.Size.Small
                            checkable: true
                            checked: bus.modelData.listen
                            emphasis: checked ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                            icon.name: "audio-headphones"
                            text: qsTr("Listen")
                            onClicked: panel.act({ action: "set_bus_listen", bus: bus.modelData.id, on: checked })
                        }
                        KanteButton {
                            size: KanteButton.Size.Small
                            emphasis: KanteButton.Emphasis.Quiet
                            icon.name: "edit-delete"
                            display: QQC2.AbstractButton.IconOnly
                            text: qsTr("Remove %1").arg(bus.modelData.name)
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            QQC2.ToolTip.text: text
                            onClicked: panel.act({ action: "remove_bus", bus: bus.modelData.id })
                        }
                    }
                    FaderRows {
                        name: bus.modelData.name
                        level: panel.levels["bus:" + bus.modelData.id] !== undefined ? panel.levels["bus:" + bus.modelData.id] : -100
                        volume: bus.modelData.volume
                        mute: bus.modelData.mute
                        onChanged: (volume, mute) => panel.act({ action: "set_bus", bus: bus.modelData.id, volume: volume, mute: mute })
                    }
                    TapSwitch {
                        checked: bus.modelData.pre_fader
                        onToggled: panel.act({ action: "set_bus_tap", bus: bus.modelData.id, pre_fader: checked })
                    }
                }
            }

            KanteButton {
                Layout.topMargin: Kirigami.Units.smallSpacing
                // BUSES in lmix-types.
                enabled: panel.buses.length < 16
                icon.name: "list-add"
                text: qsTr("Add bus")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                QQC2.ToolTip.text: qsTr("A mix of its own for an output: in-ear, a stream, a recording. Adding one rebuilds the mixer: the sound drops out briefly.")
                onClicked: panel.act({ action: "add_bus" })
            }
        }
    }
}
