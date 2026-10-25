import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtCore
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

/**
 * The Mix page: switches inputs live (LASTENHEFT.md). The window around
 * it is AppShell (docs/PLAN-SEITEN.md).
 */
AppPage {
    id: root

    /** The window (AppShell): show, actions, helpers. */
    required property var shell
    readonly property var show: shell.show
    readonly property var inputs: shell.inputs
    readonly property var backend: shell.backend
    readonly property var shortcutKeys: shell.shortcutKeys
    function act(action) { shell.act(action) }
    function inputById(id) { return shell.inputById(id) }
    function localPath(url) { return shell.localPath(url) }
    function openShortcuts() { shell.openShortcuts() }

    pageId: "mix"
    title: qsTr("Mix")
    subtitle: show && show.format
        ? qsTr("%1p%2").arg(show.format.height).arg(Math.round(Number(show.format.framerate.split("/")[0]) / Number(show.format.framerate.split("/")[1] || 1)))
        : ""
    hasProgram: true
    pageKeys: [
        { what: qsTr("Input to preview"), key: "1…9" },
        { what: qsTr("Quick play of preview"), key: qsTr("double click preview") }
    ]
    panicText: show && show.fade_to_black ? qsTr("Fade up") : qsTr("Fade to black")
    panicTip: qsTr("Program to black and back (SWI.9); the show and outputs go on")
    onPanicRequested: act({ action: "fade_to_black" })

    Settings {
        id: viewSettings
        category: "view"
        /** Input tiles: 0 small, 1 medium, 2 large (UI.3). */
        property int tileSize: 1
        /** Tiles with overlay buttons and the audio strip (detail) or
         *  picture and title alone (compact). */
        property bool detail: false
        /** Picture height of the monitors, px; the width follows the
         *  program format. Set with the handle under them. */
        property real monitorHeight: Kirigami.Units.gridUnit * 15
        /** Width of the audio column at the right edge, px. */
        property real mixerWidth: Kirigami.Units.gridUnit * 12
        /** Only the switching essentials (UI.7). */
        property bool simple: false
        /** Ids of collapsed inputs, as JSON (UI.3). */
        property string collapsed: "[]"
        /** Inputs as rows of buttons, program above preview, instead of
         *  tiles (like a hardware switcher). */
        property bool buttons: false
        /** The narrow audio mixer at the right edge. */
        property bool mixer: true
    }
    readonly property var collapsedIds: {
        try {
            const ids = JSON.parse(viewSettings.collapsed)
            return Array.isArray(ids) ? ids : []
        } catch (e) {
            return []
        }
    }
    function setCollapsed(id, on) {
        // Ids of inputs gone from the show are dropped on the way.
        const ids = collapsedIds.filter(i => i !== id && inputById(i) !== null)
        if (on)
            ids.push(id)
        viewSettings.collapsed = JSON.stringify(ids)
    }
    function syncInputs() {
        inputModel.sync(show ? show.inputs.filter(i => collapsedIds.indexOf(i.id) < 0).map(i => i.id) : [])
    }
    onCollapsedIdsChanged: syncInputs()
    /** Simple mode: `--mode` for this run, else the saved choice. */
    readonly property bool simple: backend.uiMode === "simple" || (backend.uiMode === "" && viewSettings.simple)
    /** Smallest tile width per size, grid units: the tiles of a row share
     *  what is left over, so they fill it (UI.3). */
    readonly property real tileMinWidth: Kirigami.Units.gridUnit * [8, 10.5, 14.5][Math.max(0, Math.min(2, viewSettings.tileSize))]
    /** Width over height of the program format: monitors and tiles show it
     *  without bars. */
    readonly property real aspect: show && show.format && show.format.height > 0
        ? show.format.width / show.format.height : 16 / 9


    readonly property var levels: backend.levels.length > 0 ? JSON.parse(backend.levels) : ({})
    readonly property var positions: backend.positions.length > 0 ? JSON.parse(backend.positions) : ({})

    KeyedModel { id: inputModel }
    KeyedModel { id: outputModel }
    KeyedModel { id: transitionModel }
    // Reads `show` itself: `inputs` may not have followed it yet here.
    onShowChanged: {
        syncInputs()
        outputModel.sync(show ? show.outputs.map(o => o.id) : [])
        transitionModel.sync(show ? show.transitions.map(t => String(t.slot)) : [])
    }

    function outputById(id) {
        return show ? show.outputs.find(o => o.id === id) || null : null
    }

    function transitionBySlot(slot) {
        return show ? show.transitions.find(t => String(t.slot) === slot) || null : null
    }

    readonly property var previewInput: show && show.preview ? inputById(show.preview) : null
    readonly property var programInput: show && show.program ? inputById(show.program) : null

    /** Monitor caption: an input's number and name, or a layout's. */
    function busLabel(id) {
        if (!show || !id)
            return ""
        var input = inputById(id)
        if (input)
            return input.number + " " + input.name
        var layout = (show.layouts || []).find(l => l.id === id)
        return layout ? qsTr("Layout %1 %2").arg(layout.number).arg(layout.name) : ""
    }

    // Keys 1–9: that input into preview (not while a text field has focus:
    // fields take printable keys first).
    Instantiator {
        model: 9
        delegate: Shortcut {
            required property int index
            sequence: String(index + 1)
            enabled: root.visible && root.shortcutKeys.indexOf(String(index + 1)) < 0
            onActivated: {
                var input = root.inputs[index]
                if (input && input.id !== root.show.program && input.id !== root.show.preview) {
                    root.act({ action: "preview_input", input: input.number })
                }
            }
        }
    }

    FileDialog {
        id: appendDialog
        title: qsTr("Inputs from a show")
        nameFilters: [qsTr("lmix shows (*.lmix)"), qsTr("All files (*)")]
        onAccepted: {
            appendShowDialog.path = root.localPath(selectedFile)
            appendShowDialog.open()
        }
    }

    AppendShowDialog {
        id: appendShowDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        coreUrl: backend.coreUrl
    }

    FolderDialog {
        id: relocateDialog
        title: qsTr("Folder with the missing files")
        onAccepted: root.act({ action: "relocate_media", folder: root.localPath(selectedFolder) })
    }

    AddInputDialog {
        id: addInputDialog
        parent: QQC2.Overlay.overlay
        core: backend
    }

    TriggersDialog {
        id: triggersDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        inputs: root.inputs
        outputs: root.show ? root.show.outputs : []
    }

    TitlesDialog {
        id: titlesDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        core: backend
        graphics: root.show && root.show.graphics ? root.show.graphics : []
        datasets: root.show && root.show.datasets ? root.show.datasets : []
        sources: root.show && root.show.data_sources ? root.show.data_sources : []
        templates: backend.templates.length > 0 ? JSON.parse(backend.templates) : []
        coreUrl: backend.coreUrl
        inputs: root.inputs
        outputs: root.show ? root.show.outputs : []
    }

    InputSettingsDialog {
        id: inputSettingsDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        core: backend
        inputs: root.inputs
        looksAvailable: root.show !== null && root.show.looks_available === true
        onReplaceRequested: input => {
            addInputDialog.replacing = input
            addInputDialog.open()
        }
        onTriggersRequested: input => {
            triggersDialog.graphics = null
            triggersDialog.input = input
            triggersDialog.open()
        }
        onPtzRequested: input => {
            ptzDialog.inputId = input.id
            ptzDialog.open()
        }
    }

    PtzDialog {
        id: ptzDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        inputs: root.inputs
    }

    DataSourcesDialog {
        id: dataSourcesDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        coreUrl: backend.coreUrl
        sources: root.show && root.show.data_sources ? root.show.data_sources : []
    }

    LayoutDialog {
        id: layoutDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        inputs: root.inputs
    }

    FormatDialog {
        id: formatDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        format: root.show && root.show.format ? root.show.format : ({ width: 1920, height: 1080, framerate: "30/1" })
    }

    AuxBusDialog {
        id: auxDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        buses: root.show ? root.show.aux_buses : []
        master: root.show && root.show.master ? root.show.master : ({ volume: 100, mute: false })
        monitorVolume: root.show && root.show.monitor_volume !== undefined ? root.show.monitor_volume : 100
        levels: root.levels
    }

    AddOutputDialog {
        id: addOutputDialog
        parent: QQC2.Overlay.overlay
        core: backend
        inputs: root.inputs
    }

    fileItems: [
        QQC2.MenuItem {
            text: qsTr("Inputs from a show…")
            onTriggered: appendDialog.open()
        },
        QQC2.MenuItem {
            text: qsTr("Program format…")
            onTriggered: formatDialog.open()
        },
        // OUT.10: program left or right, meters on the tiles;
        // the mixer is built anew, like for the format.
        QQC2.MenuItem {
            readonly property var layout: root.show && root.show.multiview ? root.show.multiview : ({ program: "right", meters: "shown" })
            text: qsTr("Multiview: program on the left")
            checkable: true
            checked: layout.program === "left"
            onTriggered: root.act({ action: "set_multiview", layout: { program: checked ? "left" : "right", meters: layout.meters } })
        },
        QQC2.MenuItem {
            readonly property var layout: root.show && root.show.multiview ? root.show.multiview : ({ program: "right", meters: "shown" })
            text: qsTr("Multiview: level meters")
            checkable: true
            checked: layout.meters === "shown"
            onTriggered: root.act({ action: "set_multiview", layout: { program: layout.program, meters: checked ? "shown" : "hidden" } })
        },
        // AUD.17: 48 kHz by default; 44.1 kHz for music productions.
        QQC2.MenuItem {
            text: qsTr("Sound at 44.1 kHz")
            checkable: true
            checked: root.show && root.show.sample_rate === 44100
            onTriggered: root.act({ action: "set_sample_rate", hz: checked ? 44100 : 48000 })
        }
    ]

    tools: [
        QQC2.Label {
            text: Texts.latency(backend.latency)
            font.family: KanteStyle.monoFamily
            color: KanteStyle.mutedTextColor
        },
        KanteButton {
            visible: !root.simple
            text: qsTr("Latency")
            busy: backend.measuring
            enabled: backend.connected && !backend.measuring
            onClicked: backend.measureLatency()
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Measure capture to program for a few seconds")
        }
    ]

    // The mixer area: monitors, inputs, layouts, outputs; the sound at the
    // right edge (click dummy), hidden in simple mode or from View.
    RowLayout {
        anchors.fill: parent
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            id: mixColumn
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Kirigami.Units.largeSpacing
            /** Height kept free under the monitors for the rows below and a row
             *  of tiles; in detail the tiles are higher by their extra rows, and
             *  wrapped input actions by their second row. */
            readonly property real inputsRoom: Kirigami.Units.gridUnit * 16 + Math.max(0, bar.chrome - Kirigami.Units.gridUnit * 2)
                + Math.max(0, inputActions.height - addButton.height)

            // Preview and program in the program format, each centred in
            // its half; the transitions always in the middle of the page.
            RowLayout {
                id: monitorRow
                Layout.fillWidth: true
                // A nested layout fills by default: the inputs below get the rest.
                Layout.fillHeight: false
                spacing: Kirigami.Units.largeSpacing

                /** Room for one monitor's picture: its half of the row. */
                readonly property real halfWidth: Math.max(0, (width - panelScroll.width - 2 * spacing) / 2 - 2 * Kirigami.Units.smallSpacing)
                /** Tallest picture: the half's width, and room left for the inputs below. */
                readonly property real maxPictureHeight: Math.max(monitorHandle.from,
                    Math.min(halfWidth / root.aspect, mixColumn.height - mixColumn.inputsRoom))
                readonly property real pictureHeight: Math.max(monitorHandle.from, Math.min(viewSettings.monitorHeight, maxPictureHeight))

                Item {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    implicitHeight: previewMonitor.implicitHeight
                    Monitor {
                        id: previewMonitor
                        anchors.horizontalCenter: parent.horizontalCenter
                        label: qsTr("Preview")
                        detail: root.busLabel(root.show ? root.show.preview : null)
                        tally: KanteStyle.positiveTextColor
                        // The mixer's preview output: an input or a layout, as a
                        // take brings it; it does not change pipelines on a click.
                        source: root.show && root.show.preview ? "preview" : ""
                        input: root.previewInput
                        // Quick play of preview (SWI.8).
                        onDoubleClicked: if (backend.connected && root.show && root.show.preview)
                            root.act({ action: "quick_play" })
                    }
                }

                // As high as it needs while there is room (the overlay grid in
                // reach without scrolling, #137); scrolls when the window is low.
                QQC2.ScrollView {
                    id: panelScroll
                    Layout.fillWidth: false
                    Layout.fillHeight: true
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                    Layout.preferredHeight: Math.max(Kirigami.Units.gridUnit * 6,
                        Math.min(transitionPanel.implicitHeight, mixColumn.height - mixColumn.inputsRoom))
                    contentWidth: availableWidth
                    contentHeight: transitionPanel.implicitHeight
                    QQC2.ScrollBar.horizontal.policy: QQC2.ScrollBar.AlwaysOff

                    TransitionPanel {
                        id: transitionPanel
                        width: panelScroll.availableWidth
                        core: backend
                        show: root.show
                        act: root.act
                        transitions: transitionModel
                        transitionBySlot: root.transitionBySlot
                    }
                }

                Item {
                    Layout.fillWidth: true
                    Layout.preferredWidth: 1
                    Layout.fillHeight: true
                    implicitHeight: programMonitor.implicitHeight
                    Monitor {
                        id: programMonitor
                        anchors.horizontalCenter: parent.horizontalCenter
                        label: qsTr("Program")
                        detail: root.busLabel(root.show ? root.show.program : null)
                        tally: KanteStyle.negativeTextColor
                        source: "program"
                        peakDb: root.levels.program
                        input: root.programInput
                    }
                }
            }

            // Drag: the monitors' height, the width follows the format; double click: back.
            KanteSplitHandle {
                id: monitorHandle
                Layout.fillWidth: true
                Layout.topMargin: -Kirigami.Units.largeSpacing / 2
                Layout.bottomMargin: -Kirigami.Units.largeSpacing / 2
                value: monitorRow.pictureHeight
                from: Kirigami.Units.gridUnit * 4
                to: monitorRow.maxPictureHeight
                defaultValue: Math.min(Kirigami.Units.gridUnit * 15, to)
                accessibleName: qsTr("Height of the monitors")
                onMoved: v => viewSettings.monitorHeight = v
            }

            RowLayout {
                Layout.fillWidth: true
                KanteSectionLabel { text: qsTr("Inputs") }
                KanteSegmented {
                    activeFocusOnTab: false
                    model: [qsTr("Tiles"), qsTr("Buttons")]
                    currentIndex: viewSettings.buttons ? 1 : 0
                    tooltips: [qsTr("A live tile per input"), qsTr("A row for program and one for preview, a button per input")]
                    onActivated: index => viewSettings.buttons = index === 1
                }
                KanteSegmented {
                    visible: !viewSettings.buttons
                    activeFocusOnTab: false
                    model: [qsTr("Compact"), qsTr("Detail")]
                    currentIndex: viewSettings.detail ? 1 : 0
                    tooltips: [qsTr("Picture and name"), qsTr("Also overlay buttons and the audio strip")]
                    onActivated: index => viewSettings.detail = index === 1
                }
                KanteSegmented {
                    visible: !viewSettings.buttons
                    activeFocusOnTab: false
                    model: [qsTr("S", "small tiles"), qsTr("M", "medium tiles"), qsTr("L", "large tiles")]
                    currentIndex: viewSettings.tileSize
                    tooltips: [qsTr("Small tiles"), qsTr("Medium tiles"), qsTr("Large tiles")]
                    onActivated: index => viewSettings.tileSize = index
                }
                KanteButton {
                    id: viewButton
                    text: qsTr("View")
                    emphasis: KanteButton.Emphasis.Quiet
                    onClicked: viewMenu.popup(viewButton, 0, viewButton.height)
                    QQC2.Menu {
                        id: viewMenu
                        KantePopupSkin { popup: viewMenu }
                        QQC2.MenuItem {
                            text: qsTr("Audio at the edge")
                            checkable: true
                            checked: viewSettings.mixer
                            enabled: !root.simple
                            onTriggered: viewSettings.mixer = !viewSettings.mixer
                        }
                        QQC2.MenuItem {
                            text: qsTr("Simple mode")
                            checkable: true
                            checked: root.simple
                            enabled: backend.uiMode === ""
                            onTriggered: viewSettings.simple = !viewSettings.simple
                        }
                    }
                }
                // The actions at the right; in a narrow window they wrap into
                // a second row instead of pushing the audio column out.
                Flow {
                    id: inputActions
                    Layout.fillWidth: true
                    Layout.preferredHeight: implicitHeight
                    layoutDirection: Qt.RightToLeft
                    spacing: Kirigami.Units.smallSpacing
                    KanteButton {
                        id: addButton
                        text: qsTr("+ Input")
                        enabled: backend.connected
                        onClicked: {
                            addInputDialog.replacing = null
                            addInputDialog.open()
                        }
                    }
                    KanteButton {
                        visible: !root.simple
                        text: qsTr("Data…")
                        enabled: backend.connected
                        onClicked: dataSourcesDialog.open()
                    }
                    KanteButton {
                        visible: !root.simple
                        text: qsTr("Titles…")
                        enabled: backend.connected
                        onClicked: titlesDialog.open()
                    }
                    KanteButton {
                        visible: !root.simple
                        text: qsTr("Buses…")
                        enabled: backend.connected
                        onClicked: auxDialog.open()
                    }
                    KanteButton {
                        visible: !root.simple
                        text: qsTr("Shortcuts…")
                        enabled: backend.connected
                        onClicked: root.openShortcuts()
                    }
                    KanteButton {
                        readonly property bool locked: root.show !== null && root.show.locked === true
                        text: locked ? qsTr("Locked") : qsTr("Lock")
                        emphasis: locked ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal
                        enabled: backend.connected
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: qsTr("While locked, closing inputs, loading a show and stopping outputs are refused.")
                        onClicked: root.act({ action: "set_lock", on: !locked })
                    }
                    KanteButton {
                        text: qsTr("Bring back %1").arg(root.show ? root.show.restorable : "")
                        visible: root.show !== null && !!root.show.restorable
                        onClicked: root.act({ action: "restore_input" })
                    }
                    KanteButton {
                        text: qsTr("Clear solo")
                        visible: root.inputs.some(i => i.listen !== "off")
                        onClicked: root.act({ action: "clear_listen" })
                    }
                    KanteButton {
                        visible: root.inputs.some(i => i.missing && i.missing.length > 0)
                        text: qsTr("Find missing files…")
                        emphasis: KanteButton.Emphasis.Primary
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: qsTr("Inputs marked \"Missing\" lack media files: pick the folder they are in now. Inputs on program are left alone.")
                        onClicked: relocateDialog.open()
                    }
                }
            }

            // Buttons: program row cuts the input straight on, preview row puts
            // it into preview (SWI.1, SWI.2).
            ColumnLayout {
                visible: viewSettings.buttons
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing
                Repeater {
                    model: [
                        { label: qsTr("Program"), bus: "program", color: KanteStyle.negativeTextColor },
                        { label: qsTr("Preview"), bus: "preview", color: KanteStyle.positiveTextColor }
                    ]
                    delegate: RowLayout {
                        id: busRow
                        required property var modelData
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        KanteSectionLabel {
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                            text: busRow.modelData.label
                            color: busRow.modelData.color
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Kirigami.Units.smallSpacing
                            Repeater {
                                model: root.inputs
                                delegate: KanteButton {
                                    required property var modelData
                                    readonly property bool on: root.show !== null && root.show[busRow.modelData.bus] === modelData.id
                                    width: Kirigami.Units.gridUnit * 6
                                    height: Kirigami.Units.gridUnit * 2.5
                                    text: modelData.number + " " + modelData.name
                                    emphasis: on ? (busRow.modelData.bus === "program" ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Primary)
                                        : KanteButton.Emphasis.Normal
                                    enabled: root.backend.connected
                                    Accessible.name: busRow.modelData.label + " " + modelData.number + " " + modelData.name
                                    onClicked: root.act(busRow.modelData.bus === "program"
                                        ? { action: "cut_direct", input: modelData.number }
                                        : { action: "preview_input", input: modelData.number })
                                }
                            }
                        }
                    }
                }
            }

            GridView {
                id: bar
                visible: !viewSettings.buttons
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.minimumHeight: Math.min(cellHeight, Kirigami.Units.gridUnit * 6)
                // As many tiles per row as fit at the size's smallest width; they
                // share the rest. The picture in the program format, the title
                // (and in detail the overlay and audio rows) under it.
                readonly property int columns: Math.max(1, Math.floor((width + Kirigami.Units.smallSpacing) / (root.tileMinWidth + Kirigami.Units.smallSpacing)))
                /** Height of a tile without its picture: the tiles report it. */
                property real chrome: Kirigami.Units.gridUnit * 2
                // Later, not while the grid lays out: a tile made by setting cellHeight
                // would set it again (a binding loop, seen with a restored input).
                function reportChrome(value) {
                    chrome = value
                }
                cellWidth: Math.floor(width / columns)
                cellHeight: Math.ceil((cellWidth - Kirigami.Units.smallSpacing - 2 * Kirigami.Units.smallSpacing) / root.aspect + chrome + Kirigami.Units.smallSpacing)
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: inputModel
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                /** The tile being dragged to a new place, and where it would land. */
                property var dragged: null
                property int dropIndex: -1
                function indexAtScene(point) {
                    var p = bar.mapFromItem(null, point.x, point.y)
                    return bar.indexAt(p.x + bar.contentX, p.y + bar.contentY)
                }

                delegate: InputTile {
                    id: tileDelegate
                    required property string key
                    required property int index
                    dropTarget: bar.dragged !== null && bar.dropIndex === index && bar.dragged.id !== key
                    onDragMoved: point => {
                        bar.dragged = tileDelegate.input
                        bar.dropIndex = bar.indexAtScene(point)
                    }
                    onDragEnded: point => {
                        // Collapsed inputs have no tile: the model's key, not the index into the inputs.
                        var index = bar.indexAtScene(point)
                        var target = index >= 0 && index < inputModel.count ? root.inputById(inputModel.get(index).key) : null
                        if (bar.dragged && target && target.id !== bar.dragged.id) {
                            root.act({ action: "move_input", input: bar.dragged.number, number: target.number })
                        }
                        bar.dragged = null
                        bar.dropIndex = -1
                    }
                    width: bar.cellWidth - Kirigami.Units.smallSpacing
                    height: bar.cellHeight - Kirigami.Units.smallSpacing
                    detail: viewSettings.detail
                    onChromeChanged: Qt.callLater(bar.reportChrome, chrome)
                    Component.onCompleted: Qt.callLater(bar.reportChrome, chrome)
                    core: backend
                    input: root.inputById(key) || ({ id: key, number: 0, name: "", source: { kind: "pattern" }, status: "starting", audio: { on: false, follow_video: false, volume: 0 }, has_audio: false, listen: "off", signal_loss: "hold", color: null })
                    act: root.act
                    onProgram: root.show !== null && root.show.program === key
                    onPreview: root.show !== null && root.show.preview === key
                    overlays: root.show && root.show.overlays ? root.show.overlays : []
                    overlayPreview: root.show && root.show.overlay_preview ? root.show.overlay_preview : []
                    looksAvailable: root.show !== null && root.show.looks_available === true
                    peakDb: root.levels[key]
                    channelsDb: root.levels["ch:" + key]
                    onTriggersRequested: {
                        triggersDialog.graphics = null
                        triggersDialog.input = tileDelegate.input
                        triggersDialog.open()
                    }
                    onReplaceRequested: {
                        addInputDialog.replacing = tileDelegate.input
                        addInputDialog.open()
                    }
                    dynamicSlots: root.show && root.show.dynamic_inputs
                        ? [1, 2, 3, 4].filter(n => root.show.dynamic_inputs[n - 1] === key) : []
                    onSettingsRequested: {
                        inputSettingsDialog.inputId = tileDelegate.input.id
                        inputSettingsDialog.open()
                    }
                    onPtzRequested: {
                        ptzDialog.inputId = tileDelegate.input.id
                        ptzDialog.open()
                    }
                    onCollapseRequested: root.setCollapsed(tileDelegate.input.id, true)
                }
            }

            CollapsedBar {
                Layout.fillWidth: true
                core: backend
                show: root.show
                act: root.act
                inputs: root.inputs
                collapsed: root.collapsedIds
                onExpandRequested: id => root.setCollapsed(id, false)
            }

            LayoutBar {
                visible: !root.simple
                Layout.fillWidth: true
                core: backend
                show: root.show
                act: root.act
                onEditRequested: layout => {
                    layoutDialog.layout = layout
                    layoutDialog.open()
                }
            }

            OutputBar {
                Layout.fillWidth: true
                core: backend
                show: root.show
                act: root.act
                outputs: outputModel
                outputById: root.outputById
                confirmStop: root.shell.confirmStop
                onConfirmStopChosen: on => root.shell.confirmStop = on
                onAddRequested: addOutputDialog.open()
            }
        }

        // Drag: the audio column's width; double click: back.
        KanteSplitHandle {
            visible: audioColumn.visible
            Layout.fillHeight: true
            Layout.leftMargin: -Kirigami.Units.largeSpacing / 2
            Layout.rightMargin: -Kirigami.Units.largeSpacing / 2
            orientation: Qt.Horizontal
            inverted: true
            value: audioColumn.width
            from: Kirigami.Units.gridUnit * 5
            to: Math.max(from, root.width / 2)
            defaultValue: Kirigami.Units.gridUnit * 12
            accessibleName: qsTr("Width of the audio column")
            onMoved: v => viewSettings.mixerWidth = v
        }

        NarrowMixer {
            id: audioColumn
            visible: viewSettings.mixer && !root.simple
            Layout.fillHeight: true
            Layout.preferredWidth: Math.max(Kirigami.Units.gridUnit * 5, Math.min(viewSettings.mixerWidth, root.width / 2))
            core: backend
            show: root.show
            act: root.act
            levels: root.levels
            onAudioPageRequested: root.shell.showPage("audio")
        }
    }

    component Monitor: KanteCard {
        id: monitor

        property string label
        property string detail
        property color tally
        property alias source: view.source
        property var peakDb: undefined
        /** The input shown, for its transport; null for layouts. */
        property var input: null
        /** A double click into the picture. */
        signal doubleClicked()

        // The picture in the program format, as high as the handle says.
        readonly property real pictureHeight: monitorRow.pictureHeight
        width: pictureHeight * root.aspect + 2 * Kirigami.Units.smallSpacing
        implicitHeight: monitorColumn.implicitHeight + monitor.barHeight + 2 * Kirigami.Units.smallSpacing
        height: implicitHeight
        barColor: tally
        Accessible.role: Accessible.Graphic
        Accessible.name: label + (detail ? ": " + detail : "")

        ColumnLayout {
            id: monitorColumn
            anchors.fill: parent
            anchors.margins: Kirigami.Units.smallSpacing
            anchors.topMargin: monitor.barHeight + Kirigami.Units.smallSpacing

            RowLayout {
                KanteSectionLabel {
                    text: monitor.label
                    color: monitor.tally
                }
                KanteSectionLabel {
                    Layout.fillWidth: true
                    text: monitor.detail
                    elide: Text.ElideRight
                }
            }
            LiveView {
                id: view
                Layout.fillWidth: true
                Layout.preferredHeight: monitor.pictureHeight
                core: root.backend
                TapHandler {
                    onDoubleTapped: monitor.doubleClicked()
                }
            }
            TransportBar {
                Layout.fillWidth: true
                input: monitor.input
                act: root.act
                position: monitor.input && root.positions[monitor.input.id] ? root.positions[monitor.input.id] : null
            }
            KanteAudioMeter {
                Layout.fillWidth: true
                visible: monitor.peakDb !== undefined
                level: monitor.peakDb
                scale: true
            }
        }
    }
}
