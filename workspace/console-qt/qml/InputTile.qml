import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One input of the input bar: live picture in the program format, number
 * and name; in detail also the overlay buttons and the audio strip.
 * Program and preview are a bar in the tally colour plus the word, never
 * colour alone. Click and double click do what the input is set to
 * (INP.8, INP.9; by default preview, and GO: quick play); right click:
 * everything else.
 */
KanteCard {
    id: tile
    // Small tiles (UI.3) never draw into their neighbours.
    clip: true

    required property var core
    required property var input
    required property var act
    property bool onProgram: false
    property bool onPreview: false
    /** What each overlay channel shows (input ids), channel 1 first. */
    property var overlays: []
    /** What each overlay channel gets with the next take (OVL.3). */
    property var overlayPreview: []
    property var peakDb: undefined
    /** Peak per channel, dBFS (AUD.1); undefined before the first, the
     *  right one also for mono. */
    property var leftDb: undefined
    property var rightDb: undefined
    /** Another tile is dragged over this one: it would land here. */
    property bool dropTarget: false
    /** Overlay buttons and the audio strip under the title; else picture
     *  and title alone. */
    property bool detail: false
    /** Height without the picture: the bar sizes its cells by it. The same
     *  for every tile, so a detail row stays when an input has no use for it. */
    readonly property real chrome: column.implicitHeight + barHeight + 2 * Kirigami.Units.smallSpacing
    /** `chrome` of a tile that is made and not being destroyed. On the way
     *  it is smaller; a bar sized by that made its last row fit, built it,
     *  grew and dropped it again, once a second (29.10.2026). */
    signal chromeSettled(real value)
    property bool made: false
    onChromeChanged: if (made) chromeSettled(chrome)
    Component.onCompleted: {
        made = true
        chromeSettled(chrome)
    }
    Component.onDestruction: made = false
    /** Right click: the page opens the input's menu (InputMenu). */
    signal menuRequested()
    /** Dynamic slots (1–4) that name this input (AUT.5). */
    property var dynamicSlots: []
    /** Drag to reorder: where the pointer is, in the window. */
    signal dragMoved(point scenePosition)
    signal dragEnded(point scenePosition)

    readonly property var audio: input.audio
    readonly property bool hasAudio: input.has_audio
    readonly property bool hasPicture: input.source.kind !== "audio_device"

    function ref() { return input.number }
    /** Overlay channels (1-based) this input is on. */
    readonly property var overlayChannels: overlays
        .map((id, i) => id === input.id ? i + 1 : 0)
        .filter(channel => channel > 0)
    /** Overlay channels it goes on with the next take. */
    readonly property var preparedChannels: overlayPreview
        .map((id, i) => id === input.id ? i + 1 : 0)
        .filter(channel => channel > 0)

    /** On program or in preview: the title bar is filled in the tally colour. */
    readonly property bool tallyFill: onProgram || onPreview
    /** A title text's colour: its own, or the ink that reads best on the
     *  tally fill (inkOn: in Breeze white reads at 2.9:1 on its green). */
    function titleInk(own) {
        return tallyFill ? KanteStyle.inkOn(barColor) : own
    }

    barColor: onProgram ? KanteStyle.negativeTextColor
        : (onPreview ? KanteStyle.positiveTextColor : "transparent")
    borderColor: dropTarget ? KanteStyle.focusColor : "transparent"

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Input %1 %2").arg(input.number).arg(input.name)
        + (onProgram ? ", " + qsTr("on program") : (onPreview ? ", " + qsTr("in preview") : ""))
    Accessible.description: qsTr("Click: %1. Double click (GO): %2. Right click: more.")
        .arg(Texts.label("presses", input.click)).arg(Texts.label("presses", input.go))
    opacity: drag.active ? 0.5 : 1

    DragHandler {
        id: drag
        target: null
        onCentroidChanged: if (active) tile.dragMoved(centroid.scenePosition)
        onActiveChanged: if (!active) tile.dragEnded(centroid.scenePosition)
    }

    ColumnLayout {
        id: column
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        anchors.topMargin: tile.barHeight + Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            LiveView {
                anchors.fill: parent
                visible: tile.hasPicture
                core: tile.core
                source: tile.hasPicture ? tile.input.id : ""
                cell: tile.input.slot ?? -1
            }
            QQC2.Label {
                anchors.centerIn: parent
                visible: !tile.hasPicture
                text: qsTr("Audio only")
                color: KanteStyle.mutedTextColor
            }
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: mouse => {
                    if (mouse.button === Qt.RightButton) {
                        tile.menuRequested()
                    } else {
                        tile.act({ action: "click_input", input: tile.ref() })
                    }
                }
                onDoubleClicked: mouse => {
                    if (mouse.button === Qt.LeftButton) {
                        tile.act({ action: "go_input", input: tile.ref() })
                    }
                }
            }
        }

        // Title bar: filled in the tally colour on program (red) and in
        // preview (green), as on hardware multiviewers; its text then in
        // the ink that reads best on that fill (KanteStyle.inkOn). Size
        // and margins stay the same either way, so a take does not move
        // the tiles.
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: titleRow.implicitHeight + Kirigami.Units.smallSpacing
            color: tile.tallyFill ? tile.barColor : "transparent"
            RowLayout {
                id: titleRow
                anchors.fill: parent
                anchors.leftMargin: Kirigami.Units.smallSpacing
                anchors.rightMargin: anchors.leftMargin
                KanteSectionLabel {
                    id: numberLabel
                    text: tile.input.number
                    font.family: KanteStyle.monoFamily
                    // Kante's own colour, the tally ink only on the fill.
                    Binding { target: numberLabel; property: "color"; value: KanteStyle.inkOn(tile.barColor); when: tile.tallyFill }
                }
                KanteSectionLabel {
                    text: Texts.sourceBadge(tile.input.source.kind)
                    color: tile.titleInk(KanteStyle.mutedTextColor)
                }
                KanteSectionLabel {
                    id: nameLabel
                    Layout.fillWidth: true
                    text: tile.input.name
                    elide: Text.ElideRight
                    Binding { target: nameLabel; property: "color"; value: KanteStyle.inkOn(tile.barColor); when: tile.tallyFill }
                }
                // Media files not found (SHW.4).
                KanteSectionLabel {
                    readonly property var missing: tile.input.missing || []
                    visible: missing.length > 0
                    text: qsTr("Missing")
                    color: tile.titleInk(KanteStyle.negativeTextColor)
                    Accessible.name: qsTr("Missing files: %1").arg(missing.join(", "))
                    QQC2.ToolTip.visible: missingHover.containsMouse
                    QQC2.ToolTip.text: qsTr("Not found:") + "\n" + missing.join("\n")
                    MouseArea { id: missingHover; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
                }
                KanteSwatch {
                    visible: tile.input.color !== null
                    size: KanteSwatch.Size.Dense
                    swatchColor: KanteStyle.labelColor(tile.input.color)
                }
                KanteSectionLabel {
                    visible: tile.input.color !== null
                    text: Texts.label("colors", tile.input.color)
                    color: tile.titleInk(KanteStyle.mutedTextColor)
                }
                KanteSectionLabel {
                    visible: tile.onProgram || tile.onPreview
                    text: tile.onProgram ? qsTr("PGM") : qsTr("PRV")
                    color: KanteStyle.inkOn(tile.barColor)
                }
                KanteSectionLabel {
                    visible: tile.dynamicSlots.length > 0
                    text: tile.dynamicSlots.map(n => "D" + n).join(" ")
                    color: tile.titleInk(KanteStyle.infoColor)
                    Accessible.name: qsTr("Dynamic input %1").arg(tile.dynamicSlots.join(", "))
                }
                KanteSectionLabel {
                    visible: tile.overlayChannels.length > 0
                    text: qsTr("OVL %1").arg(tile.overlayChannels.join(" "))
                    color: tile.titleInk(KanteStyle.negativeTextColor)
                }
                KanteSectionLabel {
                    visible: tile.preparedChannels.length > 0
                    text: qsTr("next OVL %1").arg(tile.preparedChannels.join(" "))
                    color: tile.titleInk(KanteStyle.positiveTextColor)
                    Accessible.name: qsTr("Goes on overlay %1 with the next take").arg(tile.preparedChannels.join(", "))
                }
                KanteSectionLabel {
                    visible: tile.input.status !== "online"
                    text: tile.input.status === "offline" ? qsTr("offline") : qsTr("starting")
                    color: tile.titleInk(tile.input.status === "offline" ? KanteStyle.negativeTextColor : KanteStyle.mutedTextColor)
                }
            }
        }

        // Eight channels (OVL.1) in one row, sharing the tile's width.
        RowLayout {
            Layout.fillWidth: true
            visible: tile.detail && tile.overlays.length > 0
            // Audio only: the row keeps its room, so all tiles are as high.
            opacity: tile.hasPicture ? 1 : 0
            enabled: tile.hasPicture
            spacing: KanteStyle.unit(2)

            Repeater {
                model: tile.overlays.length
                KanteToolButton {
                    id: overlayButton
                    required property int index
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.minimumWidth: 0
                    // Narrow tiles: the style's padding would leave no room for the digit.
                    leftPadding: 0
                    rightPadding: 0
                    readonly property int channel: index + 1
                    readonly property bool on: tile.overlays[index] === tile.input.id
                    readonly property bool prepared: tile.overlayPreview[index] === tile.input.id
                    text: prepared ? channel + "→" : channel
                    checkable: true
                    checked: on
                    Accessible.name: qsTr("Overlay %1").arg(channel)
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: (on ? qsTr("Take off overlay %1").arg(channel)
                        : qsTr("Show on overlay %1").arg(channel))
                        + ". " + (prepared ? qsTr("Goes on with the next take.") : qsTr("Right click: with the next take."))
                    onClicked: tile.act(on ? { action: "overlay_out", channel: channel }
                        : { action: "overlay_in", channel: channel, input: tile.ref() })

                    // OVL.3: prepared now, on air with the next take of preview.
                    TapHandler {
                        acceptedButtons: Qt.RightButton
                        onTapped: tile.act({ action: "overlay_preview", channel: overlayButton.channel,
                            input: overlayButton.prepared ? null : tile.ref() })
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: tile.detail
            opacity: tile.hasAudio ? 1 : 0
            enabled: tile.hasAudio
            spacing: Kirigami.Units.smallSpacing

            KanteToolButton {
                icon.name: tile.audio.on ? "audio-volume-high" : "audio-volume-muted"
                checkable: true
                checked: !tile.audio.on
                display: QQC2.AbstractButton.IconOnly
                text: tile.audio.on ? qsTr("Mute") : qsTr("Unmute")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: text
                onClicked: tile.act({ action: "set_audio", input: tile.ref(), on: !tile.audio.on })
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                // Left above right, a hairline apart; one bar while the
                // channels are unknown. Muted, the meters stay but read
                // pale (AUD.12): the source still sends, program hears none.
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: KanteStyle.unit(1)
                    opacity: tile.audio.on ? 1 : 0.4
                    KanteAudioMeter {
                        Layout.fillWidth: true
                        readonly property bool stereo: tile.rightDb !== undefined
                        level: stereo ? tile.leftDb : tile.peakDb
                        thickness: stereo ? KanteStyle.unit(4) : KanteStyle.unit(8)
                        holdMs: 2000
                        showScale: !stereo
                        Accessible.name: stereo ? qsTr("Level left") : qsTr("Level")
                    }
                    KanteAudioMeter {
                        Layout.fillWidth: true
                        visible: tile.rightDb !== undefined
                        level: tile.rightDb
                        thickness: KanteStyle.unit(4)
                        holdMs: 2000
                        showScale: true
                        Accessible.name: qsTr("Level right")
                    }
                }
                QQC2.Slider {
                    id: volume
                    Layout.fillWidth: true
                    from: 0
                    to: 100
                    stepSize: 1
                    value: tile.audio.volume
                    onMoved: tile.act({ action: "set_volume", input: tile.ref(), volume: Math.round(value) })
                    KanteSliderSkin { control: volume }
                    QQC2.ToolTip.visible: hovered || pressed
                    QQC2.ToolTip.text: qsTr("Volume %1").arg(Math.round(value))
                }
            }
            KanteToolButton {
                // After the fader by default (AUD.4); before it from the menu.
                text: tile.input.listen === "pfl" ? "PFL" : "AFL"
                checkable: true
                checked: tile.input.listen !== "off"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Listen on the headphones after the fader (right click: before it)")
                onClicked: tile.act({ action: "set_listen", input: tile.ref(), listen: tile.input.listen === "off" ? "afl" : "off" })
            }
        }
    }
}
