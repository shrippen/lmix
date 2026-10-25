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
    /** The engine can apply looks (GL mixer). */
    property bool looksAvailable: false
    /** `buses` of the show state, for the channel strip's sends. */
    property var buses: []
    property var peakDb: undefined
    /** Peak per channel, dBFS: left, right (AUD.1); undefined before the first. */
    property var channelsDb: undefined
    /** Another tile is dragged over this one: it would land here. */
    property bool dropTarget: false
    /** Overlay buttons and the audio strip under the title; else picture
     *  and title alone. */
    property bool detail: false
    /** Height without the picture: the bar sizes its cells by it. The same
     *  for every tile, so a detail row stays when an input has no use for it. */
    readonly property real chrome: column.implicitHeight + barHeight + 2 * Kirigami.Units.smallSpacing
    signal replaceRequested()
    signal triggersRequested()
    signal ptzRequested()
    signal settingsRequested()
    signal collapseRequested()
    /** Remove asked from the menu: the page removes it and offers an undo. */
    signal removeRequested()
    /** Dynamic slots (1–4) that name this input (AUT.5). */
    property var dynamicSlots: []
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
                        readonly property bool stereo: tile.channelsDb !== undefined && tile.channelsDb.length >= 2
                        level: stereo ? tile.channelsDb[0] : tile.peakDb
                        thickness: stereo ? KanteStyle.unit(4) : KanteStyle.unit(8)
                        holdMs: 2000
                        scale: !stereo
                        Accessible.name: stereo ? qsTr("Level left") : qsTr("Level")
                    }
                    KanteAudioMeter {
                        Layout.fillWidth: true
                        visible: tile.channelsDb !== undefined && tile.channelsDb.length >= 2
                        level: visible ? tile.channelsDb[1] : undefined
                        thickness: KanteStyle.unit(4)
                        holdMs: 2000
                        scale: true
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

    // A button of the menu's quick row: it closes the menu, then acts.
    component QuickButton: KanteToolButton {
        Layout.fillWidth: true
        Layout.preferredWidth: 1
        // Its whole text: the row widens the menu rather than cut "Preview".
        Layout.minimumWidth: implicitWidth
        focusPolicy: Qt.NoFocus
        /** The menu it closes (an inline component sees no ids of the file). */
        required property QQC2.Menu owner
        signal run()
        onClicked: {
            owner.close()
            run()
        }
    }

    // The input's menu: a head that says which input it is, the four common
    // actions as buttons, then sections; values in the rows, sub menus only
    // where there is a choice.
    QQC2.Menu {
        id: menu
        KantePopupSkin { popup: menu }
        delegate: KanteMenuItem {
            iconRoom: true
            icon.name: subMenu && subMenu.iconName ? subMenu.iconName : ""
            valueText: subMenu && subMenu.valueText ? subMenu.valueText : ""
        }

        ColumnLayout {
            width: parent ? parent.width : implicitWidth
            spacing: Kirigami.Units.smallSpacing
            RowLayout {
                Layout.fillWidth: true
                Layout.margins: Kirigami.Units.largeSpacing
                Layout.bottomMargin: 0
                spacing: Kirigami.Units.largeSpacing
                KanteSwatch {
                    visible: tile.input.color !== null
                    swatchColor: KanteStyle.labelColor(tile.input.color)
                    size: KanteSwatch.Size.Small
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    spacing: 0
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: tile.input.number + " · " + tile.input.name
                        font.bold: true
                        elide: Text.ElideRight
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: Texts.label("sources", tile.input.source.kind)
                        color: KanteStyle.mutedTextColor
                        font.pointSize: Kirigami.Theme.smallFont.pointSize
                        elide: Text.ElideRight
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                spacing: Kirigami.Units.smallSpacing
                QuickButton {
                    owner: menu
                    text: qsTr("Cut")
                    icon.name: "go-next"
                    enabled: !tile.onProgram
                    onRun: tile.act({ action: "cut_direct", input: tile.ref() })
                }
                QuickButton {
                    owner: menu
                    text: qsTr("Preview")
                    icon.name: "view-visible"
                    enabled: !tile.onPreview && tile.hasPicture
                    onRun: tile.act({ action: "preview_input", input: tile.ref() })
                }
                QuickButton {
                    owner: menu
                    text: qsTr("Name")
                    icon.name: "edit-rename"
                    onRun: renameDialog.open()
                }
                QuickButton {
                    owner: menu
                    text: qsTr("Options")
                    icon.name: "configure"
                    onRun: tile.settingsRequested()
                }
            }
        }

        KanteMenuSection { text: qsTr("Picture") }
        KanteMenuItem {
            text: qsTr("Replace source…")
            icon.name: "exchange-positions"
            onTriggered: tile.replaceRequested()
        }
        KanteMenuItem {
            text: tile.looksAvailable ? qsTr("Colour correction…") : qsTr("Colour correction (needs the GL mixer)")
            icon.name: "color-management"
            enabled: tile.looksAvailable && tile.hasPicture
            onTriggered: lookDialog.open()
        }
        KanteMenuItem {
            text: qsTr("PTZ…")
            icon.name: "transform-move"
            enabled: tile.hasPicture
            onTriggered: tile.ptzRequested()
        }
        ChoiceMenu {
            title: qsTr("When the signal is lost")
            readonly property string iconName: "network-disconnect"
            readonly property string valueText: choices.filter(c => c.value === current).map(c => c.label).join("")
            choices: [{ value: "hold", label: qsTr("Hold last frame") }, { value: "black", label: qsTr("Black") }]
            current: tile.input.signal_loss
            onPicked: value => tile.act({ action: "set_signal_loss", input: tile.ref(), mode: value })
        }
        // INP.10: loop, or stop on the last frame (on air without a restart).
        ChoiceMenu {
            title: qsTr("At the end")
            readonly property string iconName: "media-playback-stop"
            readonly property string valueText: enabled ? choices.filter(c => c.value === current).map(c => c.label).join("") : ""
            enabled: tile.playable
            choices: [{ value: "loop", label: qsTr("Loop") }, { value: "hold", label: qsTr("Stop on the last frame") }]
            current: tile.input.at_end || "loop"
            onPicked: value => tile.act({ action: "set_at_end", input: tile.ref(), at_end: value })
        }

        KanteMenuSection { text: qsTr("Audio") }
        KanteMenuItem {
            text: qsTr("Channel strip…")
            icon.name: "view-media-equalizer"
            enabled: tile.hasAudio
            onTriggered: stripDialog.open()
        }
        KanteMenuItem {
            text: qsTr("Audio follows video")
            icon.name: "audio-volume-high"
            enabled: tile.hasAudio
            checkable: true
            checked: tile.audio.follow_video
            onTriggered: tile.act({ action: "set_audio_follow", input: tile.ref(), follow: !tile.audio.follow_video })
        }
        ChoiceMenu {
            title: qsTr("Listen")
            readonly property string iconName: "audio-headphones"
            readonly property string valueText: choices.filter(c => c.value === current).map(c => c.label).join("")
            enabled: tile.hasAudio
            choices: [
                { value: "off", label: qsTr("Off") },
                { value: "pfl", label: qsTr("Before the fader (PFL)") },
                { value: "afl", label: qsTr("After the fader (AFL)") }
            ]
            current: tile.input.listen
            onPicked: value => tile.act({ action: "set_listen", input: tile.ref(), listen: value })
        }

        KanteMenuSection { text: qsTr("Playback") }
        KanteMenuItem {
            text: qsTr("GO: %1").arg(Texts.label("presses", tile.input.go))
            icon.name: "media-playback-start"
            enabled: tile.input.go !== "none"
            onTriggered: tile.act({ action: "go_input", input: tile.ref() })
        }
        ChoiceMenu {
            title: qsTr("Transport")
            readonly property string iconName: "media-seek-forward"
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
        // A playlist's items, to play one (UI.4).
        ChoiceMenu {
            title: qsTr("Play item")
            readonly property string iconName: "view-media-playlist"
            enabled: tile.input.source.kind === "playlist"
            choices: tile.input.source.kind === "playlist"
                ? tile.input.source.paths.map((p, i) => ({ value: i + 1, label: (i + 1) + "  " + p.split("/").pop() }))
                : []
            onPicked: value => tile.act({ action: "play_item", input: tile.ref(), item: value })
        }

        KanteMenuSection { text: qsTr("Arrange") }
        // The colour tag in the row, "none" first.
        Item {
            readonly property var tags: JSON.parse(tile.core.choices("colors"))
            width: parent ? parent.width : implicitWidth
            implicitWidth: swatches.implicitWidth + 2 * Kirigami.Units.largeSpacing
            implicitHeight: swatches.implicitHeight + Kirigami.Units.smallSpacing * 2
            KanteSwatchPicker {
                id: swatches
                x: Kirigami.Units.largeSpacing
                anchors.verticalCenter: parent.verticalCenter
                size: KanteSwatch.Size.Small
                colors: parent.tags.map(c => KanteStyle.labelColor(c))
                names: Texts.choices("colors", tile.core.choices("colors")).map(c => c.label)
                noneText: qsTr("None")
                currentIndex: parent.tags.indexOf(tile.input.color)
                Accessible.name: qsTr("Colour tag")
                onPicked: index => {
                    tile.act({ action: "set_input_color", input: tile.ref(), color: index < 0 ? null : parent.tags[index] })
                    menu.close()
                }
            }
        }
        KanteMenuItem {
            text: qsTr("Collapse")
            icon.name: "view-list-compact"
            onTriggered: tile.collapseRequested()
        }
        KanteMenuItem {
            text: tile.input.triggers && tile.input.triggers.length > 0 ? qsTr("Triggers (%1)…").arg(tile.input.triggers.length) : qsTr("Triggers…")
            icon.name: "chronometer"
            onTriggered: tile.triggersRequested()
        }
        ChoiceMenu {
            title: qsTr("Dynamic input")
            readonly property string iconName: "randomize"
            readonly property string valueText: tile.dynamicSlots.join(" ")
            choices: [1, 2, 3, 4].map(n => ({ value: n, label: tile.dynamicSlots.indexOf(n) >= 0
                ? qsTr("Clear dynamic %1").arg(n) : qsTr("Make it dynamic %1").arg(n) }))
            onPicked: value => tile.act({ action: "set_dynamic_input", slot: value,
                input: tile.dynamicSlots.indexOf(value) >= 0 ? null : tile.ref() })
        }
        ChoiceMenu {
            title: qsTr("Copy link")
            readonly property string iconName: "edit-copy"
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
        // Removed at once; the page's toast brings it back (restore_input).
        KanteMenuItem {
            text: qsTr("Remove")
            icon.name: "edit-delete"
            destructive: true
            onTriggered: tile.removeRequested()
        }
    }

    // QML has no clipboard API; a hidden text edit copies.
    TextEdit {
        id: clipboard
        visible: false
    }

    LookDialog {
        id: lookDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: tile.act
        input: tile.input
    }

    ChannelStripDialog {
        id: stripDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: tile.act
        input: tile.input
        buses: tile.buses
    }

    KanteDialog {
        id: renameDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: qsTr("Rename input %1").arg(tile.input.number)
        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
        acceptText: qsTr("Rename")
        acceptEnabled: nameField.text.trim().length > 0
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
