import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtCore
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

Kirigami.ApplicationWindow {
    id: root

    title: show && show.path ? "lmix — " + show.path : "lmix"
    width: Kirigami.Units.gridUnit * 80
    height: Kirigami.Units.gridUnit * 50
    color: KanteStyle.backgroundColor

    Core {
        id: backend
    }

    Settings {
        id: settings
        category: "appearance"
        /** KanteStyle.Kind; Kante unless chosen otherwise. */
        property int style: KanteStyle.Kind.Kante
    }

    Settings {
        id: keySettings
        category: "keys"
        /** Shortcut keys also work while the Console is not in front. */
        property bool global: false
    }

    Settings {
        id: viewSettings
        category: "view"
        /** Input tiles: 0 small, 1 medium, 2 large (UI.3). */
        property int tileSize: 1
        /** Rows of input tiles before the bar scrolls (UI.3). */
        property int rows: 2
        /** Only the switching essentials (UI.7). */
        property bool simple: false
        /** Ids of collapsed inputs, as JSON (UI.3). */
        property string collapsed: "[]"
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
    readonly property var tileUnits: [[10, 9.5], [12, 11], [16, 14]][Math.max(0, Math.min(2, viewSettings.tileSize))]

    Settings {
        id: safetySettings
        category: "safety"
        /** Ask before a running output (recording, stream) stops (UI.10). */
        property bool confirmStop: true
    }

    Binding {
        target: KanteStyle
        property: "kind"
        value: backend.style >= 0 ? backend.style : settings.style
    }

    KanteScope {
        target: root.contentItem
    }

    readonly property var show: backend.state.length > 0 ? JSON.parse(backend.state) : null
    readonly property var inputs: show ? show.inputs : []
    readonly property var levels: backend.levels.length > 0 ? JSON.parse(backend.levels) : ({})
    readonly property var recent: backend.recent.length > 0 ? JSON.parse(backend.recent) : []
    readonly property var positions: backend.positions.length > 0 ? JSON.parse(backend.positions) : ({})

    /** Send an lmix action (the API's JSON, as an object). */
    function act(action) {
        backend.act(JSON.stringify(action))
    }

    function inputById(id) {
        return inputs.find(i => i.id === id) || null
    }

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

    function localPath(url) {
        return decodeURIComponent(url.toString().replace(/^file:\/\//, ""))
    }

    readonly property var shortcuts: show && show.shortcuts ? show.shortcuts : []
    readonly property var shortcutKeys: shortcuts.filter(s => s.key).map(s => s.key)

    // The show's shortcuts (AUT.1–3); they win over the keys below.
    Instantiator {
        model: root.shortcuts.filter(s => s.key)
        delegate: Shortcut {
            required property var modelData
            sequence: modelData.key
            onActivated: root.act({ action: "run_shortcut", target: modelData.id })
        }
    }

    // Global shortcuts (AUT.12): the desktop binds the keys (it may ask
    // once) and reports presses even while another window is in front.
    readonly property string globalShortcutList: JSON.stringify(keySettings.global
        ? shortcuts.filter(s => s.key).map(s => ({ id: s.id, label: s.label, key: s.key })) : [])
    onGlobalShortcutListChanged: backend.setGlobalShortcuts(globalShortcutList)
    Component.onCompleted: {
        backend.setGlobalShortcuts(globalShortcutList)
        // Start options (UI.12): `lmix-console --window maximized|fullscreen`.
        if (backend.windowMode === "maximized")
            root.showMaximized()
        else if (backend.windowMode === "fullscreen")
            root.showFullScreen()
    }
    Connections {
        target: backend
        function onGlobalShortcut(id) {
            root.act({ action: "run_shortcut", target: id })
        }
    }

    // Keys 1–9: that input into preview (not while a text field has focus:
    // fields take printable keys first).
    Instantiator {
        model: 9
        delegate: Shortcut {
            required property int index
            sequence: String(index + 1)
            enabled: root.shortcutKeys.indexOf(String(index + 1)) < 0
            onActivated: {
                var input = root.inputs[index]
                if (input && input.id !== root.show.program && input.id !== root.show.preview) {
                    root.act({ action: "preview_input", input: input.number })
                }
            }
        }
    }

    FileDialog {
        id: openDialog
        title: qsTr("Open show")
        nameFilters: [qsTr("lmix shows (*.lmix)"), qsTr("All files (*)")]
        onAccepted: root.act({ action: "open_show", path: root.localPath(selectedFile) })
    }

    FileDialog {
        id: saveDialog
        title: qsTr("Save show")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "lmix"
        nameFilters: [qsTr("lmix shows (*.lmix)")]
        onAccepted: root.act({ action: "save_show", path: root.localPath(selectedFile) })
    }

    // Show packages (SHW.6): show, media, data files, own templates.
    FileDialog {
        id: exportDialog
        title: qsTr("Export show package")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "lmixpkg"
        nameFilters: [qsTr("lmix packages (*.lmixpkg)")]
        onAccepted: root.act({ action: "export_package", path: root.localPath(selectedFile) })
    }
    FileDialog {
        id: importDialog
        title: qsTr("Import show package")
        nameFilters: [qsTr("lmix packages (*.lmixpkg)"), qsTr("All files (*)")]
        onAccepted: {
            importFolderDialog.packagePath = root.localPath(selectedFile)
            importFolderDialog.open()
        }
    }
    FolderDialog {
        id: importFolderDialog
        property string packagePath
        title: qsTr("Empty folder for the package's show and media")
        onAccepted: root.act({ action: "import_package", path: packagePath, folder: root.localPath(selectedFolder) })
    }

    AddInputDialog {
        id: addInputDialog
        parent: QQC2.Overlay.overlay
        core: backend
    }

    ShortcutsDialog {
        id: shortcutsDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        core: backend
        shortcuts: root.shortcuts
        globalKeys: keySettings.global
        globalStatus: backend.globalKeys
        onGlobalKeysToggled: on => keySettings.global = on
        midiTbar: root.show ? root.show.midi_tbar : null
        onEditRequested: shortcut => {
            shortcutEditDialog.shortcut = shortcut
            shortcutEditDialog.open()
        }
    }

    ShortcutEditDialog {
        id: shortcutEditDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        core: backend
        inputs: root.inputs
        outputs: root.show ? root.show.outputs : []
    }

    TriggersDialog {
        id: triggersDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        inputs: root.inputs
        outputs: root.show ? root.show.outputs : []
    }

    CueListsDialog {
        id: cueListsDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        inputs: root.inputs
        outputs: root.show ? root.show.outputs : []
        lists: root.show && root.show.cue_lists ? root.show.cue_lists : []
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

    AuxBusDialog {
        id: auxDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        buses: root.show ? root.show.aux_buses : []
        levels: root.levels
    }

    AddOutputDialog {
        id: addOutputDialog
        parent: QQC2.Overlay.overlay
        core: backend
        inputs: root.inputs
    }

    pageStack.initialPage: Kirigami.Page {
        title: "lmix"
        padding: Kirigami.Units.largeSpacing

        actions: [
            Kirigami.Action {
                displayComponent: KantePill {
                    status: backend.connected ? KantePill.State.Running : KantePill.State.Failed
                    text: backend.connected ? qsTr("lmix-core") : qsTr("lmix-core offline")
                }
            }
        ]

        ColumnLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing

            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                KanteButton {
                    text: qsTr("New")
                    enabled: backend.connected
                    onClicked: root.act({ action: "new_show" })
                }
                KanteButton {
                    text: qsTr("Open…")
                    enabled: backend.connected
                    onClicked: openDialog.open()
                }
                KanteButton {
                    id: recentButton
                    text: qsTr("Recent")
                    enabled: backend.connected && root.recent.length > 0
                    onClicked: recentMenu.popup(recentButton, 0, recentButton.height)
                    ChoiceMenu {
                        id: recentMenu
                        choices: root.recent.map(path => ({ value: path, label: path }))
                        onPicked: value => root.act({ action: "open_show", path: value })
                    }
                }
                KanteButton {
                    text: qsTr("Save")
                    enabled: backend.connected
                    onClicked: root.show && root.show.path ? root.act({ action: "save_show" }) : saveDialog.open()
                }
                KanteButton {
                    text: qsTr("Save as…")
                    enabled: backend.connected
                    onClicked: saveDialog.open()
                }
                KanteButton {
                    id: packageButton
                    text: qsTr("Package")
                    enabled: backend.connected
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("The show with its media in one file, to move it to another machine")
                    onClicked: packageMenu.popup(packageButton, 0, packageButton.height)
                    QQC2.Menu {
                        id: packageMenu
                        KantePopupSkin { popup: packageMenu }
                        QQC2.MenuItem {
                            text: qsTr("Export…")
                            onTriggered: exportDialog.open()
                        }
                        QQC2.MenuItem {
                            text: qsTr("Import…")
                            onTriggered: importDialog.open()
                        }
                    }
                }
                Item { Layout.fillWidth: true }
                KanteSegmented {
                    readonly property var kinds: [KanteStyle.Kind.Kante, KanteStyle.Kind.KanteLight, KanteStyle.Kind.System]
                    model: [qsTr("Kante"), qsTr("Kante Light"), qsTr("System")]
                    currentIndex: kinds.indexOf(KanteStyle.kind)
                    tooltips: [qsTr("Kante: Gruvbox dark, Leinen when the system theme is light"), qsTr("Kante Light: only the shapes of Kante, on the system colours"), qsTr("System style (Breeze)")]
                    onActivated: index => {
                        settings.style = kinds[index]
                        backend.style = -1
                    }
                }
                QQC2.Label {
                    text: Texts.latency(backend.latency)
                    font.family: KanteStyle.monoFamily
                    color: KanteStyle.mutedTextColor
                }
                KanteButton {
                    visible: !root.simple
                    text: qsTr("Latency")
                    busy: backend.measuring
                    enabled: backend.connected && !backend.measuring
                    onClicked: backend.measureLatency()
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Measure capture to program for a few seconds")
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                spacing: Kirigami.Units.largeSpacing

                Monitor {
                    label: qsTr("Preview")
                    detail: root.busLabel(root.show ? root.show.preview : null)
                    tally: KanteStyle.positiveTextColor
                    // As the input arrives, not the tile's small picture.
                    source: root.previewInput ? "full:" + root.previewInput.id : ""
                    input: root.previewInput
                }

                TransitionPanel {
                    Layout.alignment: Qt.AlignVCenter
                    Layout.fillWidth: false
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 8
                    core: backend
                    show: root.show
                    act: root.act
                    transitions: transitionModel
                    transitionBySlot: root.transitionBySlot
                }

                Monitor {
                    label: qsTr("Program")
                    detail: root.busLabel(root.show ? root.show.program : null)
                    tally: KanteStyle.negativeTextColor
                    source: "program"
                    peakDb: root.levels.program
                    input: root.programInput
                }
            }

            RowLayout {
                Layout.fillWidth: true
                KanteSectionLabel { text: qsTr("Inputs") }
                KanteButton {
                    id: viewButton
                    text: qsTr("View")
                    emphasis: KanteButton.Emphasis.Quiet
                    onClicked: viewMenu.popup(viewButton, 0, viewButton.height)
                    QQC2.Menu {
                        id: viewMenu
                        KantePopupSkin { popup: viewMenu }
                        ChoiceMenu {
                            title: qsTr("Tile size")
                            current: viewSettings.tileSize
                            choices: [{ value: 0, label: qsTr("Small") }, { value: 1, label: qsTr("Medium") }, { value: 2, label: qsTr("Large") }]
                            onPicked: value => viewSettings.tileSize = value
                        }
                        ChoiceMenu {
                            title: qsTr("Rows")
                            current: viewSettings.rows
                            choices: [1, 2, 3, 4].map(n => ({ value: n, label: String(n) }))
                            onPicked: value => viewSettings.rows = value
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
                Item { Layout.fillWidth: true }
                KanteButton {
                    text: qsTr("Clear solo")
                    visible: root.inputs.some(i => i.listen !== "off")
                    onClicked: root.act({ action: "clear_listen" })
                }
                KanteButton {
                    text: qsTr("Bring back %1").arg(root.show ? root.show.restorable : "")
                    visible: root.show !== null && !!root.show.restorable
                    onClicked: root.act({ action: "restore_input" })
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
                    visible: !root.simple
                    text: qsTr("Shortcuts…")
                    enabled: backend.connected
                    onClicked: shortcutsDialog.open()
                }
                KanteButton {
                    visible: !root.simple
                    text: qsTr("Cues…")
                    enabled: backend.connected
                    onClicked: cueListsDialog.open()
                }
                KanteButton {
                    visible: !root.simple
                    text: qsTr("Aux…")
                    enabled: backend.connected
                    onClicked: auxDialog.open()
                }
                KanteButton {
                    visible: !root.simple
                    text: qsTr("Titles…")
                    enabled: backend.connected
                    onClicked: titlesDialog.open()
                }
                KanteButton {
                    visible: !root.simple
                    text: qsTr("Data…")
                    enabled: backend.connected
                    onClicked: dataSourcesDialog.open()
                }
                KanteButton {
                    text: qsTr("+ Input")
                    enabled: backend.connected
                    onClicked: {
                        addInputDialog.replacing = null
                        addInputDialog.open()
                    }
                }
            }

            GridView {
                id: bar
                Layout.fillWidth: true
                Layout.preferredHeight: cellHeight * Math.max(1, Math.min(viewSettings.rows, Math.ceil(count / Math.max(1, Math.floor(width / cellWidth)))))
                cellWidth: Kirigami.Units.gridUnit * root.tileUnits[0] + Kirigami.Units.smallSpacing
                cellHeight: Kirigami.Units.gridUnit * root.tileUnits[1]
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
                    core: backend
                    input: root.inputById(key) || ({ id: key, number: 0, name: "", source: { kind: "pattern" }, status: "starting", audio: { on: false, follow_video: false, volume: 0 }, has_audio: false, listen: "off", signal_loss: "hold", color: null })
                    act: root.act
                    onProgram: root.show !== null && root.show.program === key
                    onPreview: root.show !== null && root.show.preview === key
                    overlays: root.show && root.show.overlays ? root.show.overlays : []
                    looksAvailable: root.show !== null && root.show.looks_available === true
                    peakDb: root.levels[key]
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
                inputs: root.inputs.filter(i => root.collapsedIds.indexOf(i.id) >= 0)
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
                confirmStop: safetySettings.confirmStop
                onConfirmStopChanged: safetySettings.confirmStop = confirmStop
                onAddRequested: addOutputDialog.open()
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: backend.message.length > 0
                text: Texts.message(backend.message)
                color: KanteStyle.negativeTextColor
                wrapMode: Text.Wrap
            }
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

        Layout.fillWidth: true
        Layout.fillHeight: true
        Layout.preferredWidth: 1
        barColor: tally
        Accessible.role: Accessible.Graphic
        Accessible.name: label + (detail ? ": " + detail : "")

        ColumnLayout {
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
                Layout.fillHeight: true
                core: backend
            }
            TransportBar {
                Layout.fillWidth: true
                input: monitor.input
                act: root.act
                position: monitor.input && root.positions[monitor.input.id] ? root.positions[monitor.input.id] : null
            }
            KanteLevelMeter {
                Layout.fillWidth: true
                visible: monitor.peakDb !== undefined
                level: monitor.peakDb
                scale: true
            }
        }
    }
}
