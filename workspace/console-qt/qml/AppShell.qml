import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtCore
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

/**
 * The lmix window (docs/PLAN-SEITEN.md): a header with the file menu and
 * the state of the show, the pages (Mix, Show, ...) one at a time, and the
 * page bar at the foot with "Panic all". Pages are the children; each is
 * an AppPage. Switching pages stops nothing: the run is in lmix-core.
 *
 *   lmix [File] show.lmix …        ● Rec ● Stream  lmix-core  [Keys]  20:14
 *   ┌ page ────────────────────────────────────────────────────────────┐
 *   └──────────────────────────────────────────────────────────────────┘
 *   older format · refusal
 *   [Mix][Show] …                         Ctrl+1…n  [PGM]  [Panic all]
 */
Kirigami.ApplicationWindow {
    id: root

    default property alias pages: stack.data
    /** Shortcut keys also work while lmix is not in front (AUT.12). */
    readonly property bool globalShortcuts: keySettings.global
    /** A shell dialog or menu closed: the page puts the keyboard focus
     *  back where its keys work (the show view: space GO, Escape panic). */
    signal restoreFocus()

    readonly property Core backend: Core {}

    title: show && show.path ? "lmix — " + show.path : "lmix"
    width: Kirigami.Units.gridUnit * 80
    height: Kirigami.Units.gridUnit * 50
    color: KanteStyle.backgroundColor
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.None

    Settings {
        id: settings
        category: "appearance"
        /** KanteStyle.Kind; Kante unless chosen otherwise. */
        property int style: KanteStyle.Kind.Kante
    }

    Settings {
        id: keySettings
        category: "keys"
        /** On by default: GO and panic of the show work while another
         *  window is in front (owner, 18.10.2026, for lmix Show). */
        property bool global: true
    }

    Settings {
        id: safetySettings
        category: "safety"
        /** Ask before a running output (recording, stream) stops (UI.10);
         *  one setting for every page. */
        property bool confirmStop: true
    }
    property alias confirmStop: safetySettings.confirmStop

    Settings {
        id: pageSettings
        category: "pages"
        /** The page open last. */
        property string current: "mix"
    }

    Binding {
        target: KanteStyle
        property: "kind"
        value: root.backend.style >= 0 ? root.backend.style : settings.style
    }

    KanteScope {
        target: root.contentItem
    }

    readonly property var show: backend.state.length > 0 ? JSON.parse(backend.state) : null
    readonly property var inputs: show ? show.inputs : []
    readonly property var recent: backend.recent.length > 0 ? JSON.parse(backend.recent) : []

    /** Send an lmix action (the API's JSON, as an object). */
    function act(action) {
        backend.act(JSON.stringify(action))
    }

    function inputById(id) {
        return inputs.find(i => i.id === id) || null
    }

    function localPath(url) {
        return decodeURIComponent(url.toString().replace(/^file:\/\//, ""))
    }

    /** Panic all: the show's cues stop over the panic time, program fades
     *  to black. Recordings and streams go on. */
    function panicAll() {
        act({ action: "cue_panic" })
        // Set, not toggle: a second press before the new state comes back
        // must not fade program up again.
        act({ action: "set_fade_to_black", on: true })
    }

    // --- pages ---

    readonly property var pageItems: {
        var list = []
        for (var i = 0; i < stack.children.length; i++) {
            if (stack.children[i].pageId !== undefined) {
                list.push(stack.children[i])
            }
        }
        return list
    }
    property string currentPage: "mix"
    readonly property var currentItem: pageItems.find(p => p.pageId === currentPage) || null
    function showPage(id) {
        if (!pageItems.some(p => p.pageId === id)) {
            return
        }
        currentPage = id
        pageSettings.current = id
    }
    onCurrentItemChanged: {
        if (currentItem) {
            Qt.callLater(currentItem.takeFocus)
        }
    }
    onRestoreFocus: {
        root.requestActivate()
        if (currentItem) {
            currentItem.takeFocus()
        }
    }

    // Ctrl+1…n: the pages in the order of the bar.
    Instantiator {
        model: root.pageItems.length
        delegate: Shortcut {
            required property int index
            sequence: "Ctrl+" + (index + 1)
            enabled: root.shortcutKeys.indexOf(sequence) < 0
            onActivated: root.showPage(root.pageItems[index].pageId)
        }
    }

    // --- shortcuts of the show ---

    readonly property var shortcuts: show && show.shortcuts ? show.shortcuts : []
    /** Shortcuts whose key runs them: one for all shows gives way to the
     *  show's with the same key (AUT.3). */
    readonly property var keyedShortcuts: shortcuts.filter(s => s.key && !s.key_shadowed)
    readonly property var shortcutKeys: keyedShortcuts.map(s => s.key)

    // The shortcuts (AUT.1–3) while this window is in front; they win over
    // the pages' own keys.
    Instantiator {
        model: root.keyedShortcuts
        delegate: Shortcut {
            required property var modelData
            sequence: modelData.key
            onActivated: root.act({ action: "run_shortcut", target: modelData.id })
        }
    }

    // Global shortcuts (AUT.12): the desktop binds the keys (it may ask
    // once) and reports presses even while another window is in front.
    readonly property string globalShortcutList: JSON.stringify(globalShortcuts
        ? keyedShortcuts.map(s => ({ id: s.id, label: s.label, key: s.key })) : [])
    onGlobalShortcutListChanged: backend.setGlobalShortcuts(globalShortcutList)
    // Another window binds the keys: try again until it is gone, so GO and
    // panic at the desktop come back when that window closes.
    Timer {
        interval: 3000
        repeat: true
        running: root.globalShortcuts && root.backend.globalHeld
        onTriggered: root.backend.setGlobalShortcuts(root.globalShortcutList)
    }
    Connections {
        target: root.backend
        function onGlobalShortcut(id) {
            root.act({ action: "run_shortcut", target: id })
        }
    }

    function openShortcuts() {
        shortcutsDialog.open()
    }

    /** Opens one of the window's dialogs by name, for the Setup page. */
    function openShellDialog(name) {
        switch (name) {
        case "newShow": newShowDialog.open(); break
        case "exportPackage": exportDialog.open(); break
        case "importPackage": importDialog.open(); break
        case "exportSettings": exportSettingsDialog.open(); break
        case "importSettings": importSettingsDialog.open(); break
        case "resetSettings": resetSettingsDialog.open(); break
        }
    }

    function setGlobalShortcuts(on) {
        keySettings.global = on
    }

    function setStyle(kind) {
        settings.style = kind
        backend.style = -1
    }

    ShortcutsDialog {
        id: shortcutsDialog
        onClosed: root.restoreFocus()
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        core: root.backend
        shortcuts: root.shortcuts
        globalKeys: root.globalShortcuts
        globalStatus: root.backend.globalKeys
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
        core: root.backend
        inputs: root.inputs
        outputs: root.show ? root.show.outputs : []
    }

    Component.onCompleted: {
        backend.setGlobalShortcuts(globalShortcutList)
        // `--page` for this run, else the page open last.
        showPage(backend.startPage !== "" ? backend.startPage : pageSettings.current)
        if (!currentItem && pageItems.length > 0) {
            showPage(pageItems[0].pageId)
        }
        // Start options (UI.12): `--window maximized|fullscreen`.
        if (backend.windowMode === "maximized") {
            root.showMaximized()
        } else if (backend.windowMode === "fullscreen") {
            root.showFullScreen()
        }
        // The pages' entries go after the package entries.
        for (var p = 0; p < pageItems.length; p++) {
            var items = pageItems[p].fileItems
            for (var i = 0; i < items.length; i++) {
                fileMenu.addItem(items[i])
            }
        }
    }

    // --- file dialogs ---

    FileDialog {
        id: openDialog
        onRejected: root.restoreFocus()
        title: qsTr("Open show")
        nameFilters: [qsTr("lmix shows (*.lmix)"), qsTr("All files (*)")]
        onAccepted: {
            root.act({ action: "open_show", path: root.localPath(selectedFile) })
            root.restoreFocus()
        }
    }

    FileDialog {
        id: saveDialog
        onRejected: root.restoreFocus()
        title: qsTr("Save show")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "lmix"
        nameFilters: [qsTr("lmix shows (*.lmix)")]
        onAccepted: {
            root.act({ action: "save_show", path: root.localPath(selectedFile) })
            root.restoreFocus()
        }
    }

    // This machine's settings (SHW.8): shortcuts for all shows, layout
    // templates, targets, subscribers; no secrets.
    FileDialog {
        id: exportSettingsDialog
        onRejected: root.restoreFocus()
        title: qsTr("Export settings")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "json"
        nameFilters: [qsTr("lmix settings (*.json)")]
        onAccepted: {
            root.act({ action: "export_settings", path: root.localPath(selectedFile) })
            root.restoreFocus()
        }
    }
    FileDialog {
        id: importSettingsDialog
        onRejected: root.restoreFocus()
        title: qsTr("Import settings")
        nameFilters: [qsTr("lmix settings (*.json)"), qsTr("All files (*)")]
        onAccepted: {
            root.act({ action: "import_settings", path: root.localPath(selectedFile) })
            root.restoreFocus()
        }
    }
    KanteDialog {
        id: resetSettingsDialog
        onClosed: root.restoreFocus()
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        title: qsTr("Factory settings")
        modal: true
        width: Kirigami.Units.gridUnit * 24
        standardButtons: QQC2.Dialog.Cancel
        contentHeight: resetColumn.implicitHeight
        ColumnLayout {
            id: resetColumn
            width: parent.width
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: qsTr("Remove the shortcuts for all shows, the layout templates, the output targets with their secrets and the OSC subscribers of this machine? The show stays.")
            }
            KanteButton {
                Layout.alignment: Qt.AlignRight
                emphasis: KanteButton.Emphasis.Destructive
                text: qsTr("Remove all")
                onClicked: {
                    root.act({ action: "reset_settings" })
                    resetSettingsDialog.close()
                }
            }
        }
    }

    // Show packages (SHW.6): show, media, data files, own templates.
    FileDialog {
        id: exportDialog
        onRejected: root.restoreFocus()
        title: qsTr("Export show package")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "lmixpkg"
        nameFilters: [qsTr("lmix packages (*.lmixpkg)")]
        onAccepted: {
            root.act({ action: "export_package", path: root.localPath(selectedFile) })
            root.restoreFocus()
        }
    }
    FileDialog {
        id: importDialog
        onRejected: root.restoreFocus()
        title: qsTr("Import show package")
        nameFilters: [qsTr("lmix packages (*.lmixpkg)"), qsTr("All files (*)")]
        onAccepted: {
            importFolderDialog.packagePath = root.localPath(selectedFile)
            importFolderDialog.open()
        }
    }
    FolderDialog {
        id: importFolderDialog
        onRejected: root.restoreFocus()
        property string packagePath
        title: qsTr("Empty folder for the package's show and media")
        onAccepted: {
            root.act({ action: "import_package", path: packagePath, folder: root.localPath(selectedFolder) })
            root.restoreFocus()
        }
    }

    // SHW.3: a new show starts in the format chosen here.
    FormatDialog {
        id: newShowDialog
        onClosed: root.restoreFocus()
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        newShow: true
        format: root.show && root.show.format ? root.show.format : ({ width: 1920, height: 1080, framerate: "30/1" })
    }

    // --- keys overview ---

    KanteDialog {
        id: keysDialog
        onClosed: root.restoreFocus()
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        title: qsTr("Keys")
        modal: true
        width: Kirigami.Units.gridUnit * 26
        standardButtons: QQC2.Dialog.Close
        contentHeight: keysColumn.implicitHeight
        readonly property var always: [
            { what: qsTr("Pages"), key: qsTr("Ctrl+1…%1").arg(root.pageItems.length) },
            { what: qsTr("Panic all"), key: qsTr("button in the page bar") }
        ].concat(root.keyedShortcuts.map(s => ({ what: s.label, key: s.key })))
        ColumnLayout {
            id: keysColumn
            width: parent.width
            KanteSectionLabel { text: qsTr("Always") }
            Repeater {
                model: keysDialog.always
                delegate: KeyRow {}
            }
            KanteSectionLabel {
                text: root.currentItem ? qsTr("Only on %1").arg(root.currentItem.title) : ""
                visible: root.currentItem !== null && root.currentItem.pageKeys.length > 0
            }
            Repeater {
                model: root.currentItem ? root.currentItem.pageKeys : []
                delegate: KeyRow {}
            }
        }
    }
    component KeyRow: RowLayout {
        required property var modelData
        Layout.fillWidth: true
        QQC2.Label {
            Layout.fillWidth: true
            text: modelData.what
            elide: Text.ElideRight
        }
        QQC2.Label {
            text: modelData.key
            font.family: KanteStyle.monoFamily
            color: KanteStyle.neutralTextColor
        }
    }

    // --- the window ---

    readonly property var runningOutputs: show ? show.outputs.filter(o => o.status && o.status.state === "running") : []
    readonly property bool recording: runningOutputs.some(o => o.spec.kind === "record")
    readonly property bool streaming: runningOutputs.some(o => o.spec.kind === "srt" || o.spec.kind === "rtmp")

    pageStack.initialPage: Kirigami.Page {
        padding: 0
        globalToolBarStyle: Kirigami.ApplicationHeaderStyle.None

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            // The header: file menu and the state of the show. The buttons
            // take no focus: in the show, space and Escape stay with the
            // show view.
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: headerRow.implicitHeight + Kirigami.Units.smallSpacing * 2
                color: KanteStyle.sunkenColor
                RowLayout {
                    id: headerRow
                    anchors.fill: parent
                    anchors.leftMargin: Kirigami.Units.largeSpacing
                    anchors.rightMargin: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing

                    KanteHeading {
                        level: 2
                        pageTitle: true
                        text: "lmix"
                    }
                    KanteButton {
                        id: fileButton
                        focusPolicy: Qt.NoFocus
                        emphasis: KanteButton.Emphasis.Quiet
                        text: qsTr("File")
                        enabled: root.backend.connected
                        onClicked: fileMenu.popup(fileButton, 0, fileButton.height)
                        QQC2.Menu {
                            id: fileMenu
                            KantePopupSkin { popup: fileMenu }
                            QQC2.MenuItem {
                                text: qsTr("New")
                                onTriggered: newShowDialog.open()
                            }
                            QQC2.MenuItem {
                                text: qsTr("Open…")
                                onTriggered: openDialog.open()
                            }
                            ChoiceMenu {
                                title: qsTr("Recent")
                                enabled: root.recent.length > 0
                                choices: root.recent.map(path => ({ value: path, label: path }))
                                onPicked: value => root.act({ action: "open_show", path: value })
                            }
                            QQC2.MenuItem {
                                text: qsTr("Save")
                                onTriggered: root.show && root.show.path ? root.act({ action: "save_show" }) : saveDialog.open()
                            }
                            QQC2.MenuItem {
                                text: qsTr("Save as…")
                                onTriggered: saveDialog.open()
                            }
                            QQC2.MenuSeparator {}
                            QQC2.MenuItem {
                                text: qsTr("Export package…")
                                onTriggered: exportDialog.open()
                            }
                            QQC2.MenuItem {
                                text: qsTr("Import package…")
                                onTriggered: importDialog.open()
                            }
                            QQC2.MenuSeparator {}
                            QQC2.MenuItem {
                                text: qsTr("Export settings…")
                                onTriggered: exportSettingsDialog.open()
                            }
                            QQC2.MenuItem {
                                text: qsTr("Import settings…")
                                onTriggered: importSettingsDialog.open()
                            }
                            QQC2.MenuItem {
                                text: qsTr("Factory settings…")
                                onTriggered: resetSettingsDialog.open()
                            }
                            QQC2.MenuItem {
                                text: qsTr("Shortcuts…")
                                onTriggered: root.openShortcuts()
                            }
                            ChoiceMenu {
                                title: qsTr("Style")
                                readonly property var kinds: [KanteStyle.Kind.Kante, KanteStyle.Kind.KanteLight, KanteStyle.Kind.System]
                                current: KanteStyle.kind
                                choices: [
                                    { value: KanteStyle.Kind.Kante, label: qsTr("Kante") },
                                    { value: KanteStyle.Kind.KanteLight, label: qsTr("Kante Light") },
                                    { value: KanteStyle.Kind.System, label: qsTr("System") }
                                ]
                                onPicked: value => {
                                    settings.style = value
                                    root.backend.style = -1
                                }
                            }
                            QQC2.MenuSeparator {}
                        }
                    }
                    QQC2.Label {
                        Layout.fillWidth: true
                        text: root.show && root.show.path ? root.show.path : qsTr("Unsaved show")
                        color: KanteStyle.mutedTextColor
                        elide: Text.ElideMiddle
                    }
                    KantePill {
                        visible: root.recording
                        status: KantePill.State.Running
                        text: qsTr("Rec")
                    }
                    KantePill {
                        visible: root.streaming
                        status: KantePill.State.Running
                        text: qsTr("Stream")
                    }
                    KantePill {
                        status: root.backend.connected ? KantePill.State.Running : KantePill.State.Failed
                        text: root.backend.connected ? qsTr("lmix-core") : qsTr("lmix-core offline")
                    }
                    KanteButton {
                        focusPolicy: Qt.NoFocus
                        emphasis: KanteButton.Emphasis.Quiet
                        text: qsTr("Keys")
                        onClicked: keysDialog.open()
                    }
                    QQC2.Label {
                        id: clock
                        font.family: KanteStyle.monoFamily
                        color: KanteStyle.strongTextColor
                        text: Qt.formatTime(new Date(), "hh:mm:ss")
                        Timer {
                            interval: 1000
                            running: true
                            repeat: true
                            onTriggered: clock.text = Qt.formatTime(new Date(), "hh:mm:ss")
                        }
                    }
                }
            }

            StackLayout {
                id: stack
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.margins: Kirigami.Units.largeSpacing
                currentIndex: {
                    var n = 0
                    for (var i = 0; i < children.length; i++) {
                        if (children[i] === root.currentItem) {
                            return i
                        }
                    }
                    return 0
                }
            }

            // An older show file, converted on opening (SHW.7).
            QQC2.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: root.show !== null && !!root.show.older_format
                text: root.show && root.show.older_format
                    ? qsTr("This show file is in the older format %1 and was converted on opening. Saving it keeps the old file beside it (.v%1.bak).").arg(root.show.older_format)
                    : ""
                color: KanteStyle.infoColor
                wrapMode: Text.Wrap
            }

            QQC2.Label {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing
                Layout.rightMargin: Kirigami.Units.largeSpacing
                visible: root.backend.message.length > 0
                text: Texts.message(root.backend.message)
                color: KanteStyle.negativeTextColor
                wrapMode: Text.Wrap
            }

            // The page bar (KantePageBar): the pages, then program small
            // where a page has none of its own, and Panic all.
            KantePageBar {
                id: pageBar
                Layout.fillWidth: true
                model: root.pageItems.map(p => p.title)
                tooltips: root.pageItems.map((p, i) => qsTr("%1 (Ctrl+%2)").arg(p.title).arg(i + 1))
                busy: root.pageItems.map(p => p.busy)
                busyKinds: root.pageItems.map(p => p.busyKind)
                kindNames: [qsTr("Running"), qsTr("Info"), qsTr("Warning"), qsTr("Error")]
                onActivated: index => root.showPage(root.pageItems[index].pageId)
                // A Binding, not a property binding: the bar sets its index
                // itself on a click, which would cut a plain binding; this
                // writes it again on every page change (Ctrl+n, --page).
                Binding on currentIndex {
                    value: root.pageItems.indexOf(root.currentItem)
                }

                QQC2.Label {
                    text: qsTr("Ctrl+1…%1").arg(root.pageItems.length)
                    font.family: KanteStyle.monoFamily
                    color: KanteStyle.mutedTextColor
                }
                Rectangle {
                    visible: root.currentItem !== null && !root.currentItem.hasProgram
                    implicitHeight: Kirigami.Units.gridUnit * 2.25
                    implicitWidth: implicitHeight * 16 / 9
                    color: KanteStyle.stageColor
                    border.color: KanteStyle.negativeTextColor
                    border.width: 1
                    Accessible.role: Accessible.Graphic
                    Accessible.name: qsTr("Program")
                    LiveView {
                        anchors.fill: parent
                        anchors.margins: 1
                        core: root.backend
                        source: parent.visible ? "program" : ""
                    }
                }
                KanteButton {
                    focusPolicy: Qt.NoFocus
                    emphasis: KanteButton.Emphasis.Destructive
                    size: KanteButton.Size.Small
                    text: qsTr("Panic all")
                    enabled: root.backend.connected
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Stop every cue over the panic time and fade program to black; recordings and streams go on")
                    onClicked: root.panicAll()
                }
            }
        }
    }
}
