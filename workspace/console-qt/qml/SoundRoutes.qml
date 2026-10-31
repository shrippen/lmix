import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Where a sound cue's channels go (SND.5): crosspoints file channel →
 * zone channel, each with its level. None: file channel n to channel n of
 * the show's first zone.
 *
 *   File 1 → [Hall ▾] 1  [0.0 dB] ✕
 *   File 2 → [Hall ▾] 2  [0.0 dB] ✕
 *   [+ CROSSPOINT]
 *
 * Each change sends the whole list of crosspoints. Until the state brings
 * it back, the next change builds on what was sent (`draft`), so two
 * quick changes both stay.
 */
ColumnLayout {
    id: routing

    /** The cue's crosspoints from the state. */
    property var routes: []
    /** The show's zones: [{name, channels}]. */
    property var zones: []
    /** Whose crosspoints they are, e.g. the cue's id. */
    property var key
    /** The window's Core, or null: a refused change drops the draft. */
    property var core: null
    /** The crosspoints to send. */
    signal edited(var routes)
    /** A box took its value: the keys may go back to the show. */
    signal finished()

    /** Highest file channel (SND.1, SOUND_FILE_CHANNELS in lmix-types). */
    readonly property int fileChannels: 24
    /** Most crosspoints of a cue (SOUND_ROUTES). */
    readonly property int maxRoutes: 64
    /** dB in tenths: −60 dB (silence) to +12 dB, as the level of a cue. */
    readonly property int gainFrom: -600
    readonly property int gainTo: 120

    /** What was sent last, until the state brings it; null: none. */
    property var draft: null
    readonly property var shown: draft !== null ? draft : (routes || [])
    readonly property var zoneNames: zones.map(z => z.name)

    onRoutesChanged: draft = null
    onKeyChanged: draft = null

    function zoneChannels(name) {
        var zone = zones.find(z => z.name === name)
        return zone ? zone.channels : 1
    }

    /** Sends `next` and builds on it until the state comes. */
    function send(next) {
        draft = next
        edited(next)
        finished()
    }

    /** Crosspoint `index` with `changes`. */
    function change(index, changes) {
        var next = shown.map(r => Object.assign({}, r))
        Object.assign(next[index], changes)
        send(next)
    }

    function remove(index) {
        var next = shown.slice()
        next.splice(index, 1)
        send(next)
    }

    /** The next channel of the last crosspoint's zone, else file 1 to the
     *  first zone. */
    function add() {
        var last = shown.length > 0 ? shown[shown.length - 1] : null
        var zone = last ? last.zone : zoneNames[0]
        var next = shown.slice()
        next.push({
            channel: last ? Math.min(fileChannels, last.channel + 1) : 1,
            zone: zone,
            zone_channel: last ? Math.min(zoneChannels(zone), last.zone_channel + 1) : 1,
            gain_db: 0
        })
        send(next)
    }

    /** dB in tenths of a typed level. */
    function db(text) {
        return Math.round(parseFloat(text.replace(",", ".").replace(/[^0-9.\-]/g, "")) * 10)
    }

    spacing: Kirigami.Units.smallSpacing

    Connections {
        target: routing.core
        enabled: routing.draft !== null
        function onAnswered(action, refusal) {
            if (refusal.length > 0 && JSON.parse(action).action === "update_cue") {
                routing.draft = null
            }
        }
    }

    KanteSectionLabel { text: qsTr("Crosspoints") }
    QQC2.Label {
        Layout.fillWidth: true
        visible: routing.zones.length === 0
        text: qsTr("The show has no zones yet (Zones…).")
        color: KanteStyle.warningColor
        wrapMode: Text.WordWrap
    }
    QQC2.Label {
        Layout.fillWidth: true
        visible: routing.zones.length > 0 && routing.shown.length === 0
        text: qsTr("File channel n plays on channel n of zone %1.").arg(routing.zoneNames[0] || "")
        color: KanteStyle.mutedTextColor
        wrapMode: Text.WordWrap
    }

    // Rows by place: a change does not rebuild them (and the box being
    // clicked), one more adds a row.
    KeyedModel { id: rows }
    onShownChanged: rows.sync(shown.map((r, i) => String(i)))
    Component.onCompleted: rows.sync(shown.map((r, i) => String(i)))

    Repeater {
        model: rows

        delegate: RowLayout {
            id: crosspoint
            required property int index
            readonly property var route: routing.shown[index] || ({ channel: 1, zone: "", zone_channel: 1, gain_db: 0 })
            spacing: Kirigami.Units.smallSpacing

            QQC2.Label { text: qsTr("File") }
            KanteSpinBox {
                from: 1
                to: routing.fileChannels
                editable: true
                value: crosspoint.route.channel
                Accessible.name: qsTr("File channel of crosspoint %1").arg(crosspoint.index + 1)
                onValueModified: routing.change(crosspoint.index, { channel: value })
            }
            QQC2.Label { text: "→" }
            KanteComboBox {
                id: zoneBox
                Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                focusPolicy: Qt.NoFocus
                // A zone that is gone stays named, marked.
                readonly property bool missing: routing.zoneNames.indexOf(crosspoint.route.zone) < 0
                model: missing ? [crosspoint.route.zone].concat(routing.zoneNames) : routing.zoneNames
                currentIndex: missing ? 0 : routing.zoneNames.indexOf(crosspoint.route.zone)
                invalid: missing
                Accessible.name: qsTr("Zone of crosspoint %1").arg(crosspoint.index + 1)
                onActivated: index => {
                    var zone = model[index]
                    routing.change(crosspoint.index, { zone: zone, zone_channel: Math.min(crosspoint.route.zone_channel, routing.zoneChannels(zone)) })
                }
            }
            KanteSpinBox {
                from: 1
                to: Math.max(1, routing.zoneChannels(crosspoint.route.zone))
                editable: true
                value: crosspoint.route.zone_channel
                Accessible.name: qsTr("Zone channel of crosspoint %1").arg(crosspoint.index + 1)
                onValueModified: routing.change(crosspoint.index, { zone_channel: value })
            }
            KanteSpinBox {
                from: routing.gainFrom
                to: routing.gainTo
                stepSize: 5
                editable: true
                value: Math.round((crosspoint.route.gain_db || 0) * 10)
                textFromValue: v => (v / 10).toFixed(1) + " dB"
                valueFromText: t => routing.db(t)
                Accessible.name: qsTr("Level of crosspoint %1").arg(crosspoint.index + 1)
                onValueModified: routing.change(crosspoint.index, { gain_db: value / 10 })
            }
            KanteButton {
                focusPolicy: Qt.NoFocus
                size: KanteButton.Size.Small
                emphasis: KanteButton.Emphasis.Quiet
                text: "✕"
                Accessible.name: qsTr("Remove crosspoint %1").arg(crosspoint.index + 1)
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Remove this crosspoint")
                onClicked: routing.remove(crosspoint.index)
            }
        }
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        size: KanteButton.Size.Small
        enabled: routing.zones.length > 0 && routing.shown.length < routing.maxRoutes
        text: qsTr("+ Crosspoint")
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Send one more file channel to a zone channel")
        onClicked: routing.add()
    }
}
