import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Channel strip of an input with sound: trim, pan, delay (lip sync), the
 * sends to the headphones and the buses, high-pass, four-band EQ, gate and
 * compressor. Strip changes go to lmix-core as the whole strip, at most
 * every 100 ms; sections switched off keep their values. Sends are actions
 * of their own. In the
 * channel strip dialog and the input settings (UI.8); `load()` takes the
 * input's strip.
 */
ColumnLayout {
    id: panel

    visible: strip !== null
    spacing: 0

    required property var act
    /** The input whose strip is edited. */
    property var input: null
    /** Working copy, sent as a whole. */
    property var strip: null

    /** `buses` of the show state. */
    property var buses: []
    /** Send levels by bus id and to the headphones, set apart from the
     *  strip. */
    property var sends: ({})
    property var phones: ({ on: true, level: 100 })

    function sendTo(bus, level) {
        var s = Object.assign({}, sends)
        s[bus] = level
        sends = s
        panel.act({ action: "set_send", input: panel.input.number, bus: bus, level: level })
    }

    function phonesTo(on, level) {
        phones = { on: on, level: level }
        panel.act({ action: "set_phones_send", input: panel.input.number, on: on, level: level })
    }

    function edit(change) {
        var s = JSON.parse(JSON.stringify(strip))
        change(s)
        strip = s
        send.restart()
    }

    function load() {
        strip = JSON.parse(JSON.stringify(input.strip))
        sends = Object.assign({}, input.sends || {})
        phones = input.phones ? Object.assign({}, input.phones) : { on: true, level: 100 }
    }

    Timer {
        id: send
        interval: 100
        onTriggered: panel.act({ action: "set_channel_strip", input: panel.input.number, strip: panel.strip })
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


    SliderRow {
        Layout.fillWidth: true
        section: qsTr("Input")
        title: qsTr("Gain")
        from: -20; to: 20; step: 0.5; decimals: 1; unit: "dB"
        value: panel.strip ? panel.strip.gain_db : 0
        onMoved: v => panel.edit(s => s.gain_db = v)
    }
    SliderRow {
        Layout.fillWidth: true
        title: qsTr("Pan")
        hint: qsTr("Left −1, centre 0, right 1")
        from: -1; to: 1; step: 0.05; decimals: 2
        value: panel.strip ? panel.strip.pan : 0
        onMoved: v => panel.edit(s => s.pan = v)
    }
    SliderRow {
        Layout.fillWidth: true
        title: qsTr("Delay")
        hint: qsTr("Sound later than the picture (lip sync)")
        from: 0; to: 500; step: 10; unit: "ms"
        value: panel.strip ? panel.strip.delay_ms : 0
        onMoved: v => panel.edit(s => s.delay_ms = Math.round(v))
    }
    // New inputs with sound start with this delay (AUD.6).
    KanteButton {
        Layout.alignment: Qt.AlignRight
        text: qsTr("Delay for new inputs")
        enabled: panel.strip !== null
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("New inputs with sound start with this delay; inputs already there keep theirs")
        onClicked: panel.act({ action: "set_default_delay", ms: panel.strip.delay_ms })
    }

    OnRow {
        Layout.fillWidth: true
        section: qsTr("Sends")
        title: qsTr("To the headphones")
        on: panel.phones.on
        onToggled: on => panel.phonesTo(on, panel.phones.level)
    }
    SliderRow {
        Layout.fillWidth: true
        title: qsTr("Headphones level")
        from: 0; to: 100; step: 1
        live: panel.phones.on
        value: panel.phones.level
        onMoved: v => panel.phonesTo(panel.phones.on, Math.round(v))
    }
    Repeater {
        model: panel.buses
        SliderRow {
            required property var modelData
            Layout.fillWidth: true
            title: modelData.name
            from: 0; to: 100; step: 1
            value: panel.sends[modelData.id] || 0
            onMoved: v => panel.sendTo(modelData.id, Math.round(v))
        }
    }

    OnRow {
        Layout.fillWidth: true
        section: qsTr("High-pass")
        title: qsTr("On")
        on: panel.strip ? panel.strip.hpf.on : false
        onToggled: on => panel.edit(s => s.hpf.on = on)
    }
    SliderRow {
        Layout.fillWidth: true
        title: qsTr("Cutoff")
        from: 20; to: 500; step: 5; unit: "Hz"
        live: panel.strip ? panel.strip.hpf.on : false
        value: panel.strip ? panel.strip.hpf.hz : 80
        onMoved: v => panel.edit(s => s.hpf.hz = v)
    }

    OnRow {
        Layout.fillWidth: true
        section: qsTr("EQ")
        title: qsTr("On")
        on: panel.strip ? panel.strip.eq.on : false
        onToggled: on => panel.edit(s => s.eq.on = on)
    }
    Repeater {
        model: ["80 Hz", "400 Hz", "2 kHz", "8 kHz"]
        SliderRow {
            required property int index
            required property string modelData
            Layout.fillWidth: true
            title: modelData
            from: -15; to: 15; step: 0.5; decimals: 1; unit: "dB"
            live: panel.strip ? panel.strip.eq.on : false
            value: panel.strip ? panel.strip.eq.gain_db[index] : 0
            onMoved: v => panel.edit(s => s.eq.gain_db[index] = v)
        }
    }

    OnRow {
        Layout.fillWidth: true
        section: qsTr("Gate")
        title: qsTr("On")
        on: panel.strip ? panel.strip.gate.on : false
        onToggled: on => panel.edit(s => s.gate.on = on)
    }
    SliderRow {
        Layout.fillWidth: true
        title: qsTr("Threshold")
        from: -60; to: 0; step: 1; unit: "dB"
        live: panel.strip ? panel.strip.gate.on : false
        value: panel.strip ? panel.strip.gate.threshold_db : -40
        onMoved: v => panel.edit(s => s.gate.threshold_db = v)
    }

    OnRow {
        Layout.fillWidth: true
        section: qsTr("Compressor")
        title: qsTr("On")
        on: panel.strip ? panel.strip.compressor.on : false
        onToggled: on => panel.edit(s => s.compressor.on = on)
    }
    SliderRow {
        Layout.fillWidth: true
        title: qsTr("Threshold")
        from: -60; to: 0; step: 1; unit: "dB"
        live: panel.strip ? panel.strip.compressor.on : false
        value: panel.strip ? panel.strip.compressor.threshold_db : -20
        onMoved: v => panel.edit(s => s.compressor.threshold_db = v)
    }
    SliderRow {
        Layout.fillWidth: true
        title: qsTr("Ratio")
        from: 1; to: 20; step: 0.5; decimals: 1; unit: ": 1"
        live: panel.strip ? panel.strip.compressor.on : false
        value: panel.strip ? panel.strip.compressor.ratio : 4
        onMoved: v => panel.edit(s => s.compressor.ratio = v)
    }
}
