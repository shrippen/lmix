import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Channel strip of an input with sound: trim, pan, high-pass, four-band
 * EQ, gate, compressor and delay (lip sync). Every change goes to
 * lmix-core as the whole strip, at most every 100 ms; sections switched
 * off keep their values.
 */
KanteDialog {
    id: dialog

    required property var act
    /** The input whose strip is edited; set before `open()`. */
    property var input: null
    /** Working copy, sent as a whole. */
    property var strip: null

    function edit(change) {
        var s = JSON.parse(JSON.stringify(strip))
        change(s)
        strip = s
        send.restart()
    }

    title: input ? qsTr("Channel strip of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 34
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself; the strip is taller than small windows and scrolls.
    contentHeight: Math.min(column.implicitHeight, (parent ? parent.height : 800) * 0.7)
    onOpened: strip = JSON.parse(JSON.stringify(input.strip))

    Timer {
        id: send
        interval: 100
        onTriggered: dialog.act({ action: "set_channel_strip", input: dialog.input.number, strip: dialog.strip })
    }

    /** A titled slider with its value and unit. */
    component SliderRow: KanteSettingRow {
        id: row
        property real from: 0
        property real to: 1
        property real step: 1
        property real value: 0
        property string unit: ""
        property int decimals: 0
        property bool live: true
        signal moved(real value)
        titleWidth: Kirigami.Units.gridUnit * 10
        RowLayout {
            QQC2.Slider {
                id: slider
                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                from: row.from
                to: row.to
                stepSize: row.step
                value: row.value
                enabled: row.live
                Accessible.name: row.title
                onMoved: row.moved(value)
                KanteSliderSkin { control: slider }
            }
            QQC2.Label {
                Layout.preferredWidth: Kirigami.Units.gridUnit * 4
                text: slider.value.toFixed(row.decimals) + " " + row.unit
                font.family: KanteStyle.monoFamily
                opacity: row.live ? 1 : 0.5
            }
        }
    }

    /** A section's on/off switch. */
    component OnRow: KanteSettingRow {
        id: onRow
        property bool on: false
        signal toggled(bool on)
        titleWidth: Kirigami.Units.gridUnit * 10
        QQC2.Switch {
            id: toggle
            checked: onRow.on
            Accessible.name: onRow.title
            onToggled: onRow.toggled(checked)
            KanteCheckSkin { control: toggle; shape: KanteCheckSkin.Shape.Switch }
        }
    }

    QQC2.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            id: column
            width: scroll.availableWidth
            visible: dialog.strip !== null
            spacing: 0

            SliderRow {
                Layout.fillWidth: true
                section: qsTr("Input")
                title: qsTr("Gain")
                from: -20; to: 20; step: 0.5; decimals: 1; unit: "dB"
                value: dialog.strip ? dialog.strip.gain_db : 0
                onMoved: v => dialog.edit(s => s.gain_db = v)
            }
            SliderRow {
                Layout.fillWidth: true
                title: qsTr("Pan")
                hint: qsTr("Left −1, centre 0, right 1")
                from: -1; to: 1; step: 0.05; decimals: 2
                value: dialog.strip ? dialog.strip.pan : 0
                onMoved: v => dialog.edit(s => s.pan = v)
            }
            SliderRow {
                Layout.fillWidth: true
                title: qsTr("Delay")
                hint: qsTr("Sound later than the picture (lip sync)")
                from: 0; to: 500; step: 10; unit: "ms"
                value: dialog.strip ? dialog.strip.delay_ms : 0
                onMoved: v => dialog.edit(s => s.delay_ms = Math.round(v))
            }

            OnRow {
                Layout.fillWidth: true
                section: qsTr("High-pass")
                title: qsTr("On")
                on: dialog.strip ? dialog.strip.hpf.on : false
                onToggled: on => dialog.edit(s => s.hpf.on = on)
            }
            SliderRow {
                Layout.fillWidth: true
                title: qsTr("Cutoff")
                from: 20; to: 500; step: 5; unit: "Hz"
                live: dialog.strip ? dialog.strip.hpf.on : false
                value: dialog.strip ? dialog.strip.hpf.hz : 80
                onMoved: v => dialog.edit(s => s.hpf.hz = v)
            }

            OnRow {
                Layout.fillWidth: true
                section: qsTr("EQ")
                title: qsTr("On")
                on: dialog.strip ? dialog.strip.eq.on : false
                onToggled: on => dialog.edit(s => s.eq.on = on)
            }
            Repeater {
                model: ["80 Hz", "400 Hz", "2 kHz", "8 kHz"]
                SliderRow {
                    required property int index
                    required property string modelData
                    Layout.fillWidth: true
                    title: modelData
                    from: -15; to: 15; step: 0.5; decimals: 1; unit: "dB"
                    live: dialog.strip ? dialog.strip.eq.on : false
                    value: dialog.strip ? dialog.strip.eq.gain_db[index] : 0
                    onMoved: v => dialog.edit(s => s.eq.gain_db[index] = v)
                }
            }

            OnRow {
                Layout.fillWidth: true
                section: qsTr("Gate")
                title: qsTr("On")
                on: dialog.strip ? dialog.strip.gate.on : false
                onToggled: on => dialog.edit(s => s.gate.on = on)
            }
            SliderRow {
                Layout.fillWidth: true
                title: qsTr("Threshold")
                from: -60; to: 0; step: 1; unit: "dB"
                live: dialog.strip ? dialog.strip.gate.on : false
                value: dialog.strip ? dialog.strip.gate.threshold_db : -40
                onMoved: v => dialog.edit(s => s.gate.threshold_db = v)
            }

            OnRow {
                Layout.fillWidth: true
                section: qsTr("Compressor")
                title: qsTr("On")
                on: dialog.strip ? dialog.strip.compressor.on : false
                onToggled: on => dialog.edit(s => s.compressor.on = on)
            }
            SliderRow {
                Layout.fillWidth: true
                title: qsTr("Threshold")
                from: -60; to: 0; step: 1; unit: "dB"
                live: dialog.strip ? dialog.strip.compressor.on : false
                value: dialog.strip ? dialog.strip.compressor.threshold_db : -20
                onMoved: v => dialog.edit(s => s.compressor.threshold_db = v)
            }
            SliderRow {
                Layout.fillWidth: true
                title: qsTr("Ratio")
                from: 1; to: 20; step: 0.5; decimals: 1; unit: ": 1"
                live: dialog.strip ? dialog.strip.compressor.on : false
                value: dialog.strip ? dialog.strip.compressor.ratio : 4
                onMoved: v => dialog.edit(s => s.compressor.ratio = v)
            }
        }
    }
}
