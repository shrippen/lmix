import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The sound at a glance at the edge of the Mix page (PLAN-SEITEN, click
 * dummy): a narrow strip per input with sound (meter, fader, mute) and the
 * master, and the way to the Audio page for the rest (EQ, sends, buses).
 *
 *   AUDIO          Audio page ›
 *   ▮│ ▮│ ▮│   ▮│
 *   ▮┼ ▮│ ▮┼   ▮┼      meter and fader per input, master last
 *   [M][M][M]  [M]
 *    1  2  3   MST
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

    // Strips by id: a new state (every fader step brings one) keeps the
    // delegates, so a fader drag goes on.
    KeyedModel { id: strips }
    Component.onCompleted: strips.sync(soundInputs.map(i => i.id))
    onSoundInputsChanged: strips.sync(soundInputs.map(i => i.id))

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

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Kirigami.Units.smallSpacing

            ListView {
                id: list
                Layout.fillWidth: true
                Layout.fillHeight: true
                orientation: ListView.Horizontal
                spacing: Kirigami.Units.smallSpacing
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: strips
                QQC2.ScrollBar.horizontal: QQC2.ScrollBar {}
                delegate: MiniStrip {
                    required property string key
                    readonly property var input: mixer.soundInputs.find(i => i.id === key)
                        || ({ id: key, number: 0, name: "", audio: { on: false, volume: 0 } })
                    height: ListView.view ? ListView.view.height : 0
                    label: String(input.number)
                    name: input.name
                    level: mixer.levels[input.id]
                    volume: input.audio.volume
                    on: input.audio.on
                    onVolumeMoved: v => mixer.act({ action: "set_volume", input: input.number, volume: v })
                    onToggled: mixer.act({ action: "set_audio", input: input.number, on: !input.audio.on })
                }

                KanteEmptyState {
                    anchors.centerIn: parent
                    width: parent.width
                    visible: list.count === 0
                    text: qsTr("No sound")
                }
            }

            Rectangle {
                Layout.fillHeight: true
                implicitWidth: 1
                color: KanteStyle.ruleColor
            }

            MiniStrip {
                Layout.fillHeight: true
                label: qsTr("MST", "master, short")
                name: qsTr("Master")
                showScale: true
                level: mixer.levels.program
                volume: mixer.master.volume
                on: !mixer.master.mute
                onVolumeMoved: v => mixer.act({ action: "set_master", volume: v, mute: mixer.master.mute })
                onToggled: mixer.act({ action: "set_master", volume: mixer.master.volume, mute: !mixer.master.mute })
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

        // A nested layout fills by default; a strip keeps its width.
        width: Kirigami.Units.gridUnit * (showScale ? 3.5 : 2.5)
        Layout.fillWidth: false
        Layout.preferredWidth: width
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
