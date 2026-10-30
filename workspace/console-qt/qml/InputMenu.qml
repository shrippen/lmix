import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The menu of an input (right click on its tile, or on its button when
 * collapsed: docs/DESIGN-DIALOGE.md M04): a head that says which input it
 * is, the four common actions as buttons, then sections; values in the
 * rows, sub menus only where there is a choice. One for the whole page:
 * `openFor()` picks the input, the page answers the requests.
 */
QQC2.Menu {
    id: menu

    required property var core
    required property var act
    /** lmix `Input`s and the state of the show. */
    required property var inputs
    required property var show
    /** Id of the input the menu is open for. */
    property string inputId: ""
    /** It is collapsed: Expand instead of Collapse. */
    property bool collapsed: false

    readonly property var input: StatePatch.byId(inputs, inputId)
        || ({ id: "", number: 0, name: "", source: { kind: "pattern" }, audio: { on: false, follow_video: false, volume: 0 }, has_audio: false, listen: "off", signal_loss: "hold", color: null, go: "none" })
    readonly property bool onProgram: show !== null && show.program === inputId
    readonly property bool onPreview: show !== null && show.preview === inputId
    readonly property bool looksAvailable: show !== null && show.looks_available === true
    /** Dynamic slots (1–4) that name this input (AUT.5). */
    readonly property var dynamicSlots: show && show.dynamic_inputs
        ? [1, 2, 3, 4].filter(n => show.dynamic_inputs[n - 1] === inputId) : []
    readonly property var audio: input.audio
    readonly property bool hasAudio: input.has_audio
    readonly property bool playable: ["file", "url", "playlist"].indexOf(input.source.kind) >= 0
    readonly property bool hasPicture: input.source.kind !== "audio_device"

    signal replaceRequested()
    signal triggersRequested()
    signal ptzRequested()
    signal settingsRequested()
    signal renameRequested()
    signal lookRequested()
    signal stripRequested()
    signal collapseRequested()
    signal expandRequested()
    /** The page removes it and offers an undo. */
    signal removeRequested()

    // QML has no clipboard API; a hidden text edit copies. A property, not
    // a child: an item in a menu becomes an entry.
    readonly property TextEdit clipboard: TextEdit { visible: false }

    function ref() { return input.number }
    /** Open at the pointer for input `id`. */
    function openFor(id, isCollapsed) {
        inputId = id
        collapsed = isCollapsed === true
        popup()
    }
    // Removed while open: nothing left to act on.
    onInputChanged: {
        if (visible && input.id === "") {
            close()
        }
    }

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
                visible: menu.input.color !== null
                swatchColor: KanteStyle.labelColor(menu.input.color)
                size: KanteSwatch.Size.Small
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                QQC2.Label {
                    Layout.fillWidth: true
                    text: menu.input.number + " · " + menu.input.name
                    font.bold: true
                    elide: Text.ElideRight
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: Texts.label("sources", menu.input.source.kind)
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
                enabled: !menu.onProgram
                onRun: menu.act({ action: "cut_direct", input: menu.ref() })
            }
            QuickButton {
                owner: menu
                text: qsTr("Preview")
                icon.name: "view-visible"
                enabled: !menu.onPreview && menu.hasPicture
                onRun: menu.act({ action: "preview_input", input: menu.ref() })
            }
            QuickButton {
                owner: menu
                text: qsTr("Name")
                icon.name: "edit-rename"
                onRun: menu.renameRequested()
            }
            QuickButton {
                owner: menu
                text: qsTr("Options")
                icon.name: "configure"
                onRun: menu.settingsRequested()
            }
        }
    }

    KanteMenuSection { text: qsTr("Picture") }
    KanteMenuItem {
        text: qsTr("Replace source…")
        icon.name: "exchange-positions"
        onTriggered: menu.replaceRequested()
    }
    KanteMenuItem {
        text: menu.looksAvailable ? qsTr("Colour correction…") : qsTr("Colour correction (needs the GL mixer)")
        icon.name: "color-management"
        enabled: menu.looksAvailable && menu.hasPicture
        onTriggered: menu.lookRequested()
    }
    KanteMenuItem {
        text: qsTr("PTZ…")
        icon.name: "transform-move"
        enabled: menu.hasPicture
        onTriggered: menu.ptzRequested()
    }
    ChoiceMenu {
        title: qsTr("When the signal is lost")
        readonly property string iconName: "network-disconnect"
        readonly property string valueText: choices.filter(c => c.value === current).map(c => c.label).join("")
        choices: [{ value: "hold", label: qsTr("Hold last frame") }, { value: "black", label: qsTr("Black") }]
        current: menu.input.signal_loss
        onPicked: value => menu.act({ action: "set_signal_loss", input: menu.ref(), mode: value })
    }
    // INP.10: loop, or stop on the last frame (on air without a restart).
    ChoiceMenu {
        title: qsTr("At the end")
        readonly property string iconName: "media-playback-stop"
        readonly property string valueText: enabled ? choices.filter(c => c.value === current).map(c => c.label).join("") : ""
        enabled: menu.playable
        choices: [{ value: "loop", label: qsTr("Loop") }, { value: "hold", label: qsTr("Stop on the last frame") }]
        current: menu.input.at_end || "loop"
        onPicked: value => menu.act({ action: "set_at_end", input: menu.ref(), at_end: value })
    }

    KanteMenuSection { text: qsTr("Audio") }
    KanteMenuItem {
        text: qsTr("Channel strip…")
        icon.name: "view-media-equalizer"
        enabled: menu.hasAudio
        onTriggered: menu.stripRequested()
    }
    KanteMenuItem {
        text: qsTr("Audio follows video")
        icon.name: "audio-volume-high"
        enabled: menu.hasAudio
        checkable: true
        checked: menu.audio.follow_video
        onTriggered: menu.act({ action: "set_audio_follow", input: menu.ref(), follow: !menu.audio.follow_video })
    }
    ChoiceMenu {
        title: qsTr("Listen")
        readonly property string iconName: "audio-headphones"
        readonly property string valueText: choices.filter(c => c.value === current).map(c => c.label).join("")
        enabled: menu.hasAudio
        choices: [
            { value: "off", label: qsTr("Off") },
            { value: "pfl", label: qsTr("Before the fader (PFL)") },
            { value: "afl", label: qsTr("After the fader (AFL)") }
        ]
        current: menu.input.listen
        onPicked: value => menu.act({ action: "set_listen", input: menu.ref(), listen: value })
    }

    KanteMenuSection { text: qsTr("Playback") }
    KanteMenuItem {
        text: qsTr("GO: %1").arg(Texts.label("presses", menu.input.go))
        icon.name: "media-playback-start"
        enabled: menu.input.go !== "none"
        onTriggered: menu.act({ action: "go_input", input: menu.ref() })
    }
    ChoiceMenu {
        title: qsTr("Transport")
        readonly property string iconName: "media-seek-forward"
        enabled: menu.playable
        choices: [
            { value: "play", label: qsTr("Play") },
            { value: "pause", label: qsTr("Pause") },
            { value: "restart", label: qsTr("Restart") },
            { value: "previous", label: qsTr("Previous") },
            { value: "next", label: qsTr("Next") }
        ]
        onPicked: value => menu.act({ action: "transport_input", input: menu.ref(), transport: value })
    }
    // A playlist's items, to play one (UI.4).
    ChoiceMenu {
        title: qsTr("Play item")
        readonly property string iconName: "view-media-playlist"
        enabled: menu.input.source.kind === "playlist"
        choices: menu.input.source.kind === "playlist"
            ? menu.input.source.paths.map((p, i) => ({ value: i + 1, label: (i + 1) + "  " + p.split("/").pop() }))
            : []
        onPicked: value => menu.act({ action: "play_item", input: menu.ref(), item: value })
    }

    KanteMenuSection { text: qsTr("Arrange") }
    // The colour tag in the row, "none" first.
    Item {
        readonly property var tags: JSON.parse(menu.core.choices("colors"))
        width: parent ? parent.width : implicitWidth
        implicitWidth: swatches.implicitWidth + 2 * Kirigami.Units.largeSpacing
        implicitHeight: swatches.implicitHeight + Kirigami.Units.smallSpacing * 2
        KanteSwatchPicker {
            id: swatches
            x: Kirigami.Units.largeSpacing
            anchors.verticalCenter: parent.verticalCenter
            size: KanteSwatch.Size.Small
            colors: parent.tags.map(c => KanteStyle.labelColor(c))
            names: Texts.choices("colors", menu.core.choices("colors")).map(c => c.label)
            noneText: qsTr("None")
            currentIndex: parent.tags.indexOf(menu.input.color)
            Accessible.name: qsTr("Colour tag")
            onPicked: index => {
                menu.act({ action: "set_input_color", input: menu.ref(), color: index < 0 ? null : parent.tags[index] })
                menu.close()
            }
        }
    }
    KanteMenuItem {
        text: menu.collapsed ? qsTr("Expand") : qsTr("Collapse")
        icon.name: menu.collapsed ? "view-list-details" : "view-list-compact"
        onTriggered: menu.collapsed ? menu.expandRequested() : menu.collapseRequested()
    }
    KanteMenuItem {
        text: menu.input.triggers && menu.input.triggers.length > 0 ? qsTr("Triggers (%1)…").arg(menu.input.triggers.length) : qsTr("Triggers…")
        icon.name: "chronometer"
        onTriggered: menu.triggersRequested()
    }
    ChoiceMenu {
        title: qsTr("Dynamic input")
        readonly property string iconName: "randomize"
        readonly property string valueText: menu.dynamicSlots.join(" ")
        choices: [1, 2, 3, 4].map(n => ({ value: n, label: menu.dynamicSlots.indexOf(n) >= 0
            ? qsTr("Clear dynamic %1").arg(n) : qsTr("Make it dynamic %1").arg(n) }))
        onPicked: value => menu.act({ action: "set_dynamic_input", slot: value,
            input: menu.dynamicSlots.indexOf(value) >= 0 ? null : menu.ref() })
    }
    ChoiceMenu {
        title: qsTr("Copy link")
        readonly property string iconName: "edit-copy"
        enabled: menu.input.ingest !== null && menu.input.ingest !== undefined
        choices: menu.input.ingest ? [
            { value: menu.input.ingest.whip_url, label: qsTr("WHIP (OBS, encoder): %1").arg(menu.input.ingest.whip_url) },
            { value: menu.input.ingest.browser_url, label: qsTr("Browser page (guest): %1").arg(menu.input.ingest.browser_url) }
        ] : []
        onPicked: value => {
            menu.clipboard.text = value
            menu.clipboard.selectAll()
            menu.clipboard.copy()
        }
    }
    QQC2.MenuSeparator {}
    // Removed at once; the page's toast brings it back (restore_input).
    KanteMenuItem {
        text: qsTr("Remove")
        icon.name: "edit-delete"
        destructive: true
        onTriggered: menu.removeRequested()
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
}
