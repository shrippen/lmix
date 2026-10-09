import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One input of the input bar: live picture, number and name, audio strip.
 * Program and preview are a bar in the tally colour plus the word, never
 * colour alone. Click: preview; double click: straight to program; right
 * click: everything else.
 */
KanteCard {
    id: tile

    required property var core
    required property var input
    required property var act
    property bool onProgram: false
    property bool onPreview: false
    /** What each overlay channel shows (input ids), channel 1 first. */
    property var overlays: []
    property var peakDb: undefined
    /** Another tile is dragged over this one: it would land here. */
    property bool dropTarget: false
    signal replaceRequested()
    /** Drag to reorder: where the pointer is, in the window. */
    signal dragMoved(point scenePosition)
    signal dragEnded(point scenePosition)

    readonly property var audio: input.audio
    readonly property bool hasAudio: input.has_audio
    readonly property bool playable: ["file", "url", "playlist"].indexOf(input.source.kind) >= 0
    readonly property bool hasPicture: input.source.kind !== "audio_device"

    function ref() { return input.number }
    /** Overlay channels (1-based) this input is on. */
    readonly property var overlayChannels: overlays
        .map((id, i) => id === input.id ? i + 1 : 0)
        .filter(channel => channel > 0)

    barColor: onProgram ? KanteStyle.negativeTextColor
        : (onPreview ? KanteStyle.positiveTextColor : "transparent")
    borderColor: dropTarget ? KanteStyle.focusColor : "transparent"

    Accessible.role: Accessible.Button
    Accessible.name: qsTr("Input %1 %2").arg(input.number).arg(input.name)
        + (onProgram ? ", " + qsTr("on program") : (onPreview ? ", " + qsTr("in preview") : ""))
    Accessible.description: qsTr("Click: preview. Double click: program. Right click: more.")
    opacity: drag.active ? 0.5 : 1

    DragHandler {
        id: drag
        target: null
        onCentroidChanged: if (active) tile.dragMoved(centroid.scenePosition)
        onActiveChanged: if (!active) tile.dragEnded(centroid.scenePosition)
    }

    ColumnLayout {
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
                        menu.popup()
                    } else {
                        tile.act({ action: "preview_input", input: tile.ref() })
                    }
                }
                onDoubleClicked: mouse => {
                    if (mouse.button === Qt.LeftButton) {
                        tile.act({ action: "cut_direct", input: tile.ref() })
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            KanteSectionLabel {
                text: tile.input.number
                font.family: KanteStyle.monoFamily
            }
            KanteSectionLabel {
                text: Texts.sourceBadge(tile.input.source.kind)
                color: KanteStyle.mutedTextColor
            }
            KanteSectionLabel {
                Layout.fillWidth: true
                text: tile.input.name
                elide: Text.ElideRight
            }
            KanteSwatch {
                visible: tile.input.color !== null
                size: KanteSwatch.Size.Dense
                swatchColor: KanteStyle.labelColor(tile.input.color)
            }
            KanteSectionLabel {
                visible: tile.input.color !== null
                text: Texts.label("colors", tile.input.color)
                color: KanteStyle.mutedTextColor
            }
            KanteSectionLabel {
                visible: tile.onProgram || tile.onPreview
                text: tile.onProgram ? qsTr("PGM") : qsTr("PRV")
                color: tile.barColor
            }
            KanteSectionLabel {
                visible: tile.overlayChannels.length > 0
                text: qsTr("OVL %1").arg(tile.overlayChannels.join(" "))
                color: KanteStyle.negativeTextColor
            }
            KanteSectionLabel {
                visible: tile.input.status !== "online"
                text: tile.input.status === "offline" ? qsTr("offline") : qsTr("starting")
                color: tile.input.status === "offline" ? KanteStyle.negativeTextColor : KanteStyle.mutedTextColor
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: tile.hasPicture && tile.overlays.length > 0
            spacing: Kirigami.Units.smallSpacing

            KanteSectionLabel {
                text: qsTr("Overlay")
                color: KanteStyle.mutedTextColor
            }
            Repeater {
                model: tile.overlays.length
                KanteToolButton {
                    required property int index
                    readonly property int channel: index + 1
                    readonly property bool on: tile.overlays[index] === tile.input.id
                    text: channel
                    checkable: true
                    checked: on
                    Accessible.name: qsTr("Overlay %1").arg(channel)
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: on ? qsTr("Take off overlay %1").arg(channel)
                        : qsTr("Show on overlay %1").arg(channel)
                    onClicked: tile.act(on ? { action: "overlay_out", channel: channel }
                        : { action: "overlay_in", channel: channel, input: tile.ref() })
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: tile.hasAudio
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
                KanteLevelMeter {
                    Layout.fillWidth: true
                    level: tile.peakDb
                    scale: true
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
                text: tile.input.listen === "afl" ? "AFL" : "PFL"
                checkable: true
                checked: tile.input.listen !== "off"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Listen on the monitor bus (right click: before or after the fader)")
                onClicked: tile.act({ action: "set_listen", input: tile.ref(), listen: tile.input.listen === "off" ? "pfl" : "off" })
            }
        }
    }

    QQC2.Menu {
        id: menu
        KantePopupSkin { popup: menu }

        QQC2.MenuItem {
            text: qsTr("Cut to program")
            enabled: !tile.onProgram
            onTriggered: tile.act({ action: "cut_direct", input: tile.ref() })
        }
        QQC2.MenuItem {
            text: qsTr("Rename…")
            onTriggered: renameDialog.open()
        }
        QQC2.MenuItem {
            text: qsTr("Replace source…")
            onTriggered: tile.replaceRequested()
        }
        ChoiceMenu {
            title: qsTr("Colour tag")
            choices: [{ value: null, label: qsTr("None") }].concat(Texts.choices("colors", tile.core.choices("colors")))
            current: tile.input.color
            onPicked: value => tile.act({ action: "set_input_color", input: tile.ref(), color: value })
        }
        QQC2.Menu {
            id: lossMenu
            title: qsTr("When the signal is lost")
            KantePopupSkin { popup: lossMenu }
            QQC2.MenuItem {
                text: qsTr("Hold last frame")
                checkable: true
                checked: tile.input.signal_loss === "hold"
                onTriggered: tile.act({ action: "set_signal_loss", input: tile.ref(), mode: "hold" })
            }
            QQC2.MenuItem {
                text: qsTr("Black")
                checkable: true
                checked: tile.input.signal_loss === "black"
                onTriggered: tile.act({ action: "set_signal_loss", input: tile.ref(), mode: "black" })
            }
        }
        QQC2.Menu {
            id: audioMenu
            title: qsTr("Audio")
            enabled: tile.hasAudio
            KantePopupSkin { popup: audioMenu }
            QQC2.MenuItem {
                text: qsTr("Audio follows video")
                checkable: true
                checked: tile.audio.follow_video
                onTriggered: tile.act({ action: "set_audio_follow", input: tile.ref(), follow: !tile.audio.follow_video })
            }
            QQC2.MenuItem {
                text: qsTr("Listen before the fader (PFL)")
                checkable: true
                checked: tile.input.listen === "pfl"
                onTriggered: tile.act({ action: "set_listen", input: tile.ref(), listen: checked ? "pfl" : "off" })
            }
            QQC2.MenuItem {
                text: qsTr("Listen after the fader (AFL)")
                checkable: true
                checked: tile.input.listen === "afl"
                onTriggered: tile.act({ action: "set_listen", input: tile.ref(), listen: checked ? "afl" : "off" })
            }
        }
        ChoiceMenu {
            title: qsTr("Playback")
            enabled: tile.playable
            choices: [
                { value: "play", label: qsTr("Play") },
                { value: "pause", label: qsTr("Pause") },
                { value: "restart", label: qsTr("Restart") },
                { value: "previous", label: qsTr("Previous") },
                { value: "next", label: qsTr("Next") }
            ]
            onPicked: value => tile.act({ action: "transport_input", input: tile.ref(), transport: value })
        }
        ChoiceMenu {
            title: qsTr("Send to this input (copy link)")
            enabled: tile.input.ingest !== null && tile.input.ingest !== undefined
            choices: tile.input.ingest ? [
                { value: tile.input.ingest.whip_url, label: qsTr("WHIP (OBS, encoder): %1").arg(tile.input.ingest.whip_url) },
                { value: tile.input.ingest.browser_url, label: qsTr("Browser page (guest): %1").arg(tile.input.ingest.browser_url) }
            ] : []
            onPicked: value => {
                clipboard.text = value
                clipboard.selectAll()
                clipboard.copy()
            }
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            text: qsTr("Remove")
            onTriggered: tile.act({ action: "remove_input", input: tile.ref() })
        }
    }

    // QML has no clipboard API; a hidden text edit copies.
    TextEdit {
        id: clipboard
        visible: false
    }

    KanteDialog {
        id: renameDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: qsTr("Rename input %1").arg(tile.input.number)
        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
        onOpened: {
            nameField.text = tile.input.name
            nameField.forceActiveFocus()
            nameField.selectAll()
        }
        onAccepted: {
            if (nameField.text.trim().length > 0) {
                tile.act({ action: "rename_input", input: tile.ref(), name: nameField.text.trim() })
            }
        }
        KanteTextField {
            id: nameField
            width: Kirigami.Units.gridUnit * 18
            onAccepted: renameDialog.accept()
        }
    }
}
