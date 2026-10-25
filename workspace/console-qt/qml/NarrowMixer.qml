import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The sound at a glance at the edge of the Mix page (PLAN-SEITEN, click
 * dummy): a narrow strip per input with sound (meter, fader, mute) and the
 * master, and the way to the Audio page for the rest (EQ, sends, buses).
 * More strips than fit across go on in a new row, and the faders get
 * shorter; up to four rows share the height, from the fifth it scrolls.
 *
 *   AUDIO          Audio page ›
 *   ▮│ ▮│ ▮│ ▮│
 *   ▮┼ ▮│ ▮┼ ▮┼      meter and fader per input
 *   [M][M][M][M]
 *    1  2  3  4
 *   ▮│ ▮│            the next row, master last
 *   [M][M]
 *    5 MST
 */
KanteCard {
    id: mixer

    required property var core
    required property var show
    required property var act
    /** Peaks per input id and `program` (Core.levels as an object). */
    property var levels: ({})
    signal audioPageRequested()

    readonly property var soundInputs: show ? show.inputs.filter(i => i.has_audio) : []
    readonly property var master: show && show.master ? show.master : ({ volume: 100, mute: false })
    /** The master's key among the strips (input ids never start with a colon). */
    readonly property string masterKey: ":master"
    /** Rows that share the height before the column scrolls. */
    readonly property int shownRows: 4

    // Strips by id, the master last: a new state (every fader step brings
    // one) keeps the delegates, so a fader drag goes on.
    KeyedModel { id: strips }
    function syncStrips() {
        strips.sync(soundInputs.map(i => i.id).concat([masterKey]))
    }
    Component.onCompleted: syncStrips()
    onSoundInputsChanged: syncStrips()

    Accessible.name: qsTr("Audio mixer")

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            KanteSectionLabel {
                Layout.fillWidth: true
                text: qsTr("Audio")
            }
            KanteButton {
                emphasis: KanteButton.Emphasis.Quiet
                size: KanteButton.Size.Small
                focusPolicy: Qt.NoFocus
                text: qsTr("Audio page ›")
                Accessible.name: qsTr("Open the Audio page")
                onClicked: mixer.audioPageRequested()
            }
        }

        KanteEmptyState {
            Layout.fillWidth: true
            visible: mixer.soundInputs.length === 0
            text: qsTr("No sound")
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: strips
            QQC2.ScrollBar.vertical: QQC2.ScrollBar {}

            readonly property int perRow: Math.max(1, Math.floor(width / (Kirigami.Units.gridUnit * 2.5)))
            // The model's count: the view's own changes while it lays out (a binding loop).
            readonly property int rows: Math.ceil(strips.count / perRow)
            cellWidth: Math.floor(width / perRow)
            // Up to four rows fill the height; more scroll, at the height of four.
            cellHeight: Math.max(Kirigami.Units.gridUnit * 6,
                Math.floor(height / Math.max(1, Math.min(rows, mixer.shownRows))))

            delegate: MiniStrip {
                required property string key
                readonly property bool isMaster: key === mixer.masterKey
                readonly property var input: isMaster ? null : (mixer.soundInputs.find(i => i.id === key)
                    || ({ id: key, number: 0, name: "", audio: { on: false, volume: 0 } }))
                width: grid.cellWidth - Kirigami.Units.smallSpacing
                height: grid.cellHeight - Kirigami.Units.smallSpacing
                label: isMaster ? qsTr("MST", "master, short") : String(input.number)
                name: isMaster ? qsTr("Master") : input.name
                showScale: isMaster && width >= Kirigami.Units.gridUnit * 3.5
                level: isMaster ? mixer.levels.program : mixer.levels[input.id]
                volume: isMaster ? mixer.master.volume : input.audio.volume
                on: isMaster ? !mixer.master.mute : input.audio.on
                onVolumeMoved: v => mixer.act(isMaster ? { action: "set_master", volume: v, mute: mixer.master.mute }
                    : { action: "set_volume", input: input.number, volume: v })
                onToggled: mixer.act(isMaster ? { action: "set_master", volume: mixer.master.volume, mute: !mixer.master.mute }
                    : { action: "set_audio", input: input.number, on: !input.audio.on })
            }
        }
    }

    /** One narrow strip: meter and fader side by side, mute below, the label last. */
    component MiniStrip: ColumnLayout {
        id: strip
        property string label: ""
        property string name: ""
        property var level: undefined
        property int volume: 0
        property bool on: true
        /** dB marks beside the meter (the master's, for every strip). */
        property bool showScale: false
        signal volumeMoved(int volume)
        signal toggled()

        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillHeight: true
            Layout.alignment: Qt.AlignHCenter
            spacing: 2
            KanteAudioMeter {
                Layout.fillHeight: true
                vertical: true
                scale: strip.showScale
                level: strip.level
            }
            QQC2.Slider {
                id: fader
                Layout.fillHeight: true
                orientation: Qt.Vertical
                from: 0
                to: 100
                stepSize: 1
                value: strip.volume
                Accessible.name: qsTr("Volume of %1").arg(strip.name)
                QQC2.ToolTip.visible: hovered || pressed
                QQC2.ToolTip.text: qsTr("%1: %2").arg(strip.name).arg(Math.round(value))
                onMoved: strip.volumeMoved(Math.round(value))
                KanteSliderSkin { control: fader }
            }
        }
        KanteButton {
            Layout.fillWidth: true
            size: KanteButton.Size.Small
            focusPolicy: Qt.NoFocus
            emphasis: strip.on ? KanteButton.Emphasis.Normal : KanteButton.Emphasis.Destructive
            text: qsTr("M", "mute, short")
            Accessible.name: strip.on ? qsTr("Mute %1").arg(strip.name) : qsTr("Unmute %1").arg(strip.name)
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: strip.on ? qsTr("%1 is on. Click: mute.").arg(strip.name) : qsTr("%1 is muted. Click: on.").arg(strip.name)
            onClicked: strip.toggled()
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: strip.label
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            font.family: KanteStyle.monoFamily
            color: KanteStyle.mutedTextColor
            Accessible.ignored: true
        }
    }
}
