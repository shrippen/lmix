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
 * the Audio page and in the buses dialog. Adding or removing a bus and
 * changing a tap build the engine's mixer anew (program stops for a
 * moment), so they ask first.
 */
Item {
    id: panel

    required property var act
    required property var core
    /** `buses` at most and `name_max` characters of a name (lmix-types). */
    readonly property var limits: JSON.parse(core.choices("bus_limits"))
    /** `buses` of the show state. */
    property var buses: []
    /** `master` of the show state. */
    property var master: ({ volume: 100, mute: false })
    /** `phones` of the show state. */
    property var phones: ({ volume: 100, mute: false, pre_fader: false })
    /** Levels as the bridge gives them (`program`, `phones`, `bus:<id>`). */
    /** AppShell.levels, or null. */
    property var levels: null
    function peak(key) {
        const db = levels ? levels.peak(key) : undefined
        return db !== undefined ? db : -100
    }
    readonly property real contentHeight: column.implicitHeight

    // Rows by id: a new state (every fader step brings one) keeps them, so
    // a drag or a name being typed goes on.
    KeyedModel { id: busModel }
    Component.onCompleted: busModel.sync(buses.map(b => b.id))
    onBusesChanged: busModel.sync(buses.map(b => b.id))

    /** Removes `bus` after asking: its sends go with it. */
    function remove(bus) {
        removeDialog.action = { action: "remove_bus", bus: bus.id }
        removeDialog.title = qsTr("Remove %1?").arg(bus.name)
        removeDialog.open()
    }

    KanteDialog {
        id: removeDialog
        property var action: null
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        width: Kirigami.Units.gridUnit * 24
        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
        // KanteDialog has a child of its own, so it does not size to this.
        contentHeight: removeText.implicitHeight
        onAccepted: panel.act(action)
        QQC2.Label {
            id: removeText
            width: parent.width
            text: qsTr("Every input's send to it goes with it.")
            wrapMode: Text.WordWrap
        }
    }

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
            showScale: true
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

    /** Where the sends are taken: before the channel faders or after.
     *  Shows the state; a click asks for the other (`wanted`). */
    component TapSwitch: QQC2.Switch {
        id: tap
        property bool on: false
        signal wanted(bool on)
        checked: on
        text: qsTr("Before the faders")
        onToggled: {
            const want = checked
            checked = Qt.binding(() => tap.on)
            tap.wanted(want)
        }
        KanteCheckSkin { control: tap; shape: KanteCheckSkin.Shape.Switch }
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        QQC2.ToolTip.text: qsTr("On: the sends ignore the channel faders and on/off (a monitor mix).")
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
                level: panel.peak("program")
                volume: panel.master.volume
                mute: panel.master.mute
                onChanged: (volume, mute) => panel.act({ action: "set_master", volume: volume, mute: mute })
            }

            // What every input sends to the headphones, or the solos (AUD.4).
            KanteSectionLabel { text: qsTr("Headphones · listening") }
            FaderRows {
                name: qsTr("headphones")
                level: panel.peak("phones")
                volume: panel.phones.volume
                mute: panel.phones.mute
                onChanged: (volume, mute) => panel.act({ action: "set_phones", volume: volume, mute: mute })
            }
            TapSwitch {
                on: panel.phones.pre_fader
                onWanted: on => panel.act({ action: "set_phones_tap", pre_fader: on })
            }

            Repeater {
                model: busModel

                ColumnLayout {
                    id: bus
                    required property string key
                    readonly property var modelData: panel.buses.find(b => b.id === key)
                        || ({ id: key, name: "", pre_fader: false, volume: 100, mute: false, listen: false })
                    Layout.fillWidth: true
                    Layout.topMargin: Kirigami.Units.smallSpacing

                    RowLayout {
                        Layout.fillWidth: true
                        KanteTextField {
                            id: name
                            Layout.fillWidth: true
                            text: bus.modelData.name
                            maximumLength: panel.limits.name_max
                            Accessible.name: qsTr("Name of the bus")
                            onEditingFinished: {
                                const wanted = text.trim()
                                if (wanted.length > 0 && wanted !== bus.modelData.name) {
                                    panel.act({ action: "rename_bus", bus: bus.modelData.id, name: wanted })
                                }
                                // Back to the show's name: the new one, or the old
                                // one when refused.
                                text = Qt.binding(() => bus.modelData.name)
                            }
                        }
                        // Shows the state; a click asks for the other.
                        KanteButton {
                            size: KanteButton.Size.Small
                            emphasis: bus.modelData.listen ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                            icon.name: "audio-headphones"
                            text: qsTr("Listen")
                            Accessible.checkable: true
                            Accessible.checked: bus.modelData.listen
                            onClicked: panel.act({ action: "set_bus_listen", bus: bus.modelData.id, on: !bus.modelData.listen })
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
                            onClicked: panel.remove(bus.modelData)
                        }
                    }
                    FaderRows {
                        name: bus.modelData.name
                        level: panel.peak("bus:" + bus.modelData.id)
                        volume: bus.modelData.volume
                        mute: bus.modelData.mute
                        onChanged: (volume, mute) => panel.act({ action: "set_bus", bus: bus.modelData.id, volume: volume, mute: mute })
                    }
                    TapSwitch {
                        on: bus.modelData.pre_fader
                        onWanted: on => panel.act({ action: "set_bus_tap", bus: bus.modelData.id, pre_fader: on })
                    }
                }
            }

            KanteButton {
                Layout.topMargin: Kirigami.Units.smallSpacing
                enabled: panel.buses.length < panel.limits.buses
                icon.name: "list-add"
                text: qsTr("Add bus")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                QQC2.ToolTip.text: qsTr("A mix of its own for an output: in-ear, a stream, a recording.")
                onClicked: panel.act({ action: "add_bus" })
            }
        }
    }
}
