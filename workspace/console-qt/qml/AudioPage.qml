import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

/**
 * The Audio page (docs/PLAN-SEITEN.md): every input with sound as a
 * channel strip (fader, meter, mute, solo, audio follows video, tally),
 * the strip of the selected input to edit beside them (ChannelStripPanel),
 * master, headphones and the show's buses, and its zones. Its own panic
 * mutes the master; video, show and outputs go on.
 */
AppPage {
    id: root

    /** The window (AppShell): show, actions, helpers. */
    required property var shell
    readonly property var show: shell.show
    readonly property var levels: shell.backend.levels.length > 0 ? JSON.parse(shell.backend.levels) : ({})
    readonly property var master: show && show.master ? show.master : ({ volume: 100, mute: false })
    readonly property var soundInputs: show ? show.inputs.filter(i => i.has_audio) : []
    // Strips by id: a new state (every fader step brings one) keeps the
    // delegates, so a fader drag goes on.
    KeyedModel { id: stripModel }
    Component.onCompleted: stripModel.sync(soundInputs.map(i => i.id))
    onSoundInputsChanged: stripModel.sync(soundInputs.map(i => i.id))
    // A bus being listened to puts the headphones on the solos too.
    readonly property bool soloing: soundInputs.some(i => i.listen !== "off")
        || (show && show.buses ? show.buses : []).some(b => b.listen)
    function act(action) { shell.act(action) }

    /** 0 inputs, 1 sends, 2 buses, 3 zones. */
    property int part: 0
    property string selectedId: ""
    readonly property var selected: soundInputs.find(i => i.id === selectedId) || soundInputs[0] || null
    // A new state brings new objects: load only for another input, or
    // edits in progress would be thrown away.
    property string loadedId: ""
    onSelectedChanged: {
        if (selected && selected.id !== loadedId) {
            loadedId = selected.id
            stripPanel.input = selected
            stripPanel.load()
        }
    }

    pageId: "audio"
    title: qsTr("Audio")
    subtitle: show ? qsTr("%1 kHz · buses: %2").arg(show.sample_rate === 44100 ? "44.1" : "48").arg((show.buses || []).length) : ""
    busy: soloing
    busyKind: KantePageBar.Kind.Warning
    panicText: master.mute ? qsTr("Audio on") : qsTr("Audio off")
    panicTip: qsTr("Mute the master of program sound and back; video, show and outputs go on")
    onPanicRequested: act({ action: "set_master", volume: master.volume, mute: !master.mute })

    tools: [
        KanteSegmented {
            activeFocusOnTab: false
            model: [qsTr("Inputs"), qsTr("Sends"), qsTr("Buses"), qsTr("Zones")]
            currentIndex: root.part
            onActivated: index => root.part = index
        },
        KanteButton {
            visible: root.soloing
            focusPolicy: Qt.NoFocus
            text: qsTr("Clear solo")
            onClicked: root.act({ action: "clear_listen" })
        }
    ]

    RowLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.largeSpacing

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: root.part

            // Inputs: one strip each.
            ListView {
                id: strips
                orientation: ListView.Horizontal
                spacing: Kirigami.Units.smallSpacing
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: stripModel
                QQC2.ScrollBar.horizontal: QQC2.ScrollBar {}
                delegate: Strip {}

                KanteEmptyState {
                    anchors.centerIn: parent
                    visible: strips.count === 0
                    text: qsTr("No input with sound yet")
                }
            }

            // Sends (AUD.13): inputs × headphones and buses, a level per cell.
            QQC2.ScrollView {
                id: sendScroll
                contentWidth: availableWidth
                readonly property var buses: root.show && root.show.buses ? root.show.buses : []
                GridLayout {
                    width: sendScroll.availableWidth
                    columns: sendScroll.buses.length + 2
                    columnSpacing: Kirigami.Units.smallSpacing
                    rowSpacing: Kirigami.Units.smallSpacing
                    Item { implicitWidth: Kirigami.Units.gridUnit * 8 }
                    KanteSectionLabel {
                        Layout.fillWidth: true
                        text: qsTr("Headphones")
                    }
                    Repeater {
                        model: sendScroll.buses
                        delegate: KanteSectionLabel {
                            required property var modelData
                            Layout.fillWidth: true
                            text: modelData.name
                            elide: Text.ElideRight
                        }
                    }
                    Repeater {
                        // By id, like the strips: a send drag goes on.
                        model: stripModel
                        delegate: SendRow {}
                    }
                }
            }

            BusPanel {
                act: root.act
                core: root.shell.backend
                buses: root.show && root.show.buses ? root.show.buses : []
                master: root.master
                phones: root.show && root.show.phones ? root.show.phones : ({ volume: 100, mute: false, pre_fader: false })
                levels: root.levels
            }

            // Zones: where each plays on this machine; edited in the dialog.
            ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: root.show && root.show.zones ? root.show.zones : []
                    delegate: RowLayout {
                        required property var modelData
                        Layout.fillWidth: true
                        QQC2.Label {
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                            text: modelData.name
                            elide: Text.ElideRight
                        }
                        QQC2.Label {
                            text: qsTr("%1 ch.").arg(modelData.channels)
                            font.family: KanteStyle.monoFamily
                            color: KanteStyle.mutedTextColor
                        }
                        QQC2.Label {
                            Layout.fillWidth: true
                            text: modelData.outputs.length === 0 ? qsTr("plays nowhere on this machine")
                                : modelData.outputs.map(o => (o.device !== "" ? o.device : qsTr("default output")) + " " + o.channels.join("/")).join(" · ")
                            color: modelData.outputs.length === 0 ? KanteStyle.warningColor : KanteStyle.textColor
                            elide: Text.ElideRight
                        }
                    }
                }
                QQC2.Label {
                    visible: !root.show || !root.show.zones || root.show.zones.length === 0
                    text: qsTr("No zones yet")
                    color: KanteStyle.mutedTextColor
                }
                KanteButton {
                    text: qsTr("Zones…")
                    onClicked: zonesDialog.open()
                }
                Item { Layout.fillHeight: true }
            }
        }

        // The selected input's strip: trim, pan, delay, sends, EQ, dynamics.
        KanteCard {
            visible: root.part === 0 && root.selected !== null
            Layout.preferredWidth: Kirigami.Units.gridUnit * 20
            Layout.fillHeight: true
            QQC2.ScrollView {
                id: stripScroll
                anchors.fill: parent
                anchors.margins: Kirigami.Units.smallSpacing
                contentWidth: availableWidth
                ColumnLayout {
                    width: stripScroll.availableWidth
                    KanteSectionLabel {
                        text: root.selected ? qsTr("Channel strip · %1 %2").arg(root.selected.number).arg(root.selected.name) : ""
                    }
                    ChannelStripPanel {
                        id: stripPanel
                        Layout.fillWidth: true
                        act: root.act
                        buses: root.show && root.show.buses ? root.show.buses : []
                    }
                }
            }
        }
    }

    ZonesDialog {
        id: zonesDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        zones: root.show && root.show.zones ? root.show.zones : []
    }

    /** One input's sends: its name, then a level to the headphones and per
     *  bus (0–100). The cells go into the grid around it, so it is a list
     *  of items. */
    component SendRow: Repeater {
        id: sendRow
        required property string key
        readonly property var input: root.soundInputs.find(i => i.id === key)
            || ({ id: key, number: 0, name: "", sends: {}, phones: { on: true, level: 100 } })
        model: sendScroll.buses.length + 2
        delegate: Loader {
            required property int index
            Layout.fillWidth: index > 0
            sourceComponent: index === 0 ? nameCell : levelCell
            Component {
                id: nameCell
                QQC2.Label {
                    text: sendRow.input.number + " " + sendRow.input.name
                    elide: Text.ElideRight
                }
            }
            Component {
                id: levelCell
                QQC2.Slider {
                    id: send
                    // null: the headphones.
                    readonly property var bus: index > 1 ? sendScroll.buses[index - 2] : null
                    from: 0
                    to: 100
                    stepSize: 1
                    value: bus === null ? (sendRow.input.phones ? sendRow.input.phones.level : 100)
                        : (sendRow.input.sends && sendRow.input.sends[bus.id] !== undefined ? sendRow.input.sends[bus.id] : 0)
                    Accessible.name: qsTr("Send of %1 to %2").arg(sendRow.input.name).arg(bus === null ? qsTr("Headphones") : bus.name)
                    onMoved: root.act(bus === null
                        ? { action: "set_phones_send", input: sendRow.input.number, on: sendRow.input.phones ? sendRow.input.phones.on : true, level: Math.round(value) }
                        : { action: "set_send", input: sendRow.input.number, bus: bus.id, level: Math.round(value) })
                    QQC2.ToolTip.visible: hovered || pressed
                    QQC2.ToolTip.text: value.toFixed(0)
                    KanteSliderSkin { control: send }
                }
            }
        }
    }

    /** One input's channel strip. */
    component Strip: KanteCard {
        id: strip
        required property string key
        readonly property var input: root.soundInputs.find(i => i.id === key)
            || ({ id: key, number: 0, name: "", audio: { on: false, follow_video: false, volume: 0 }, listen: "off" })
        readonly property bool onProgram: root.show !== null && root.show.program === input.id
        readonly property bool onPreview: root.show !== null && root.show.preview === input.id
        readonly property bool chosen: root.selected !== null && root.selected.id === input.id
        width: Kirigami.Units.gridUnit * 6
        height: ListView.view ? ListView.view.height - Kirigami.Units.gridUnit : 0
        barColor: onProgram ? KanteStyle.negativeTextColor : (onPreview ? KanteStyle.positiveTextColor : (chosen ? KanteStyle.focusColor : "transparent"))
        Accessible.name: qsTr("Channel strip %1").arg(input.name)

        ColumnLayout {
            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            anchors.topMargin: strip.barHeight + Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.smallSpacing

            KanteButton {
                Layout.fillWidth: true
                emphasis: strip.chosen ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Quiet
                size: KanteButton.Size.Small
                text: strip.input.number + " " + strip.input.name
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Edit this channel strip")
                onClicked: root.selectedId = strip.input.id
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: strip.onProgram ? qsTr("PGM") : (strip.onPreview ? qsTr("PVW") : " ")
                horizontalAlignment: Text.AlignHCenter
                font.family: KanteStyle.monoFamily
                color: strip.onProgram ? KanteStyle.negativeTextColor : KanteStyle.positiveTextColor
            }
            KanteButton {
                Layout.fillWidth: true
                size: KanteButton.Size.Small
                emphasis: strip.input.audio.follow_video ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                text: qsTr("AFV")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Audio follows video: sound on with program, off without (AUD.5)")
                onClicked: root.act({ action: "set_audio_follow", input: strip.input.number, follow: !strip.input.audio.follow_video })
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                KanteAudioMeter {
                    Layout.fillHeight: true
                    vertical: true
                    scale: true
                    level: root.levels[strip.input.id]
                }
                QQC2.Slider {
                    id: fader
                    Layout.fillHeight: true
                    orientation: Qt.Vertical
                    from: 0
                    to: 100
                    stepSize: 1
                    value: strip.input.audio.volume
                    Accessible.name: qsTr("Volume of %1").arg(strip.input.name)
                    onMoved: root.act({ action: "set_volume", input: strip.input.number, volume: Math.round(value) })
                    KanteSliderSkin { control: fader }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                // Peak in dBFS, coloured by the meter's zones (AUD.12).
                QQC2.Label {
                    readonly property var level: root.levels[strip.input.id]
                    Layout.fillWidth: true
                    text: level === undefined || level <= -60 ? "−∞" : level.toFixed(1)
                    font.family: KanteStyle.monoFamily
                    color: level === undefined ? KanteStyle.mutedTextColor
                        : level > -6 ? KanteStyle.negativeTextColor
                        : level > -18 ? KanteStyle.warningColor : KanteStyle.positiveTextColor
                    Accessible.name: qsTr("Peak %1 dB").arg(text)
                }
                QQC2.Label {
                    text: fader.value.toFixed(0)
                    font.family: KanteStyle.monoFamily
                    Accessible.name: qsTr("Volume %1").arg(text)
                }
            }
            RowLayout {
                Layout.fillWidth: true
                KanteButton {
                    Layout.fillWidth: true
                    size: KanteButton.Size.Small
                    emphasis: strip.input.audio.on ? KanteButton.Emphasis.Normal : KanteButton.Emphasis.Destructive
                    text: strip.input.audio.on ? qsTr("On") : qsTr("Muted")
                    onClicked: root.act({ action: "set_audio", input: strip.input.number, on: !strip.input.audio.on })
                }
                KanteButton {
                    Layout.fillWidth: true
                    size: KanteButton.Size.Small
                    emphasis: strip.input.listen !== "off" ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                    text: strip.input.listen === "pfl" ? qsTr("PFL") : qsTr("AFL")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Listen on the headphones after the fader (AFL)")
                    onClicked: root.act({ action: "set_listen", input: strip.input.number, listen: strip.input.listen === "off" ? "afl" : "off" })
                }
            }
        }
    }
}
