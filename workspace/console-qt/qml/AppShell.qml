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
 * the state of the show, the current page's title row and the lock, the
 * pages (Mix, Show, ...) one at a time, and the page bar at the foot with
 * the page's panic and "Panic all" (narrow or a colour band, as chosen in
 * the File menu; in the band the panics go up). The pages are `pageEntries`
 * (Main.qml), each an AppPage. Switching pages stops nothing: the run is in
 * lmix-core.
 *
 * A page can go to a window of its own (PageWindow, e.g. on a second
 * screen) and come back; the bar marks it and brings its window to the
 * front. All windows share this window's Core (one connection to
 * lmix-core) and its tile picture. Which pages are out and where their
 * windows were stays in the settings (PageArrangement).
 *
 *   lmix [File] | MIX 1080p30 tools  show.lmix … ● Rec  lmix-core [Keys] 20:14 [Lock]
 *   ┌ page ────────────────────────────────────────────────────────────┐
 *   └──────────────────────────────────────────────────────────────────┘
 *   older format · refusal
 *   [Mix][Show] …                         Ctrl+1…n  [PGM]  [FTB] [Panic]
 */
Kirigami.ApplicationWindow {
    id: root

    /** The pages in the order of the bar: PageEntry { pageId; page: SomePage {} }. */
    property list<QtObject> pageEntries
    component PageEntry: QtObject {
        required property string pageId
        property Component page
    }
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
        /** KantePageBar.Density of the page bar: Normal, Compact or a
         *  band only, for more room on the pages. */
        property int pageBar: KantePageBar.Density.Normal
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
        /** Pages in windows of their own and their places (PageArrangement). */
        property string windows: "{}"
    }

    /** Which pages are out and where their windows were. */
    readonly property alias pageArrangement: arrangement
    PageArrangement {
        id: arrangement
        text: pageSettings.windows
    }

    Binding {
        target: KanteStyle
        property: "kind"
        value: root.backend.style >= 0 ? root.backend.style : settings.style
    }

    KanteScope {
        target: root.contentItem
    }

    /** Remove after asking, with a toast to bring it back (Removals). */
    function askRemove(kind, item) {
        // In the window the operator works in: its question, its Undo.
        const win = operatorWindow()
        const here = win !== root && win.removals ? win.removals : removals
        here.ask(kind, item)
    }
    /** An output's new settings, with a toast to bring the old back. */
    function changeOutput(output, action) {
        removals.change(output, action)
    }

    /** Every input live in one picture; LiveView shows its cell of it. */
    readonly property alias tilesAtlas: tilesAtlas
    TilesAtlas {
        id: tilesAtlas
        core: root.backend
    }

    /** The show state (ShowState.qml), null until the first arrives. */
    property var show: null
    ShowState {
        id: showState
    }
    Connections {
        target: root.backend
        function onStatePatched(patch) {
            showState.patch(JSON.parse(patch))
            root.show = showState
        }
    }
    readonly property var inputs: show ? show.inputs : []
    /** Peaks in dBFS, undefined while none was measured: `peak(key)` of an
     *  input id, "program", "phones", "bus:<id>" or "zone:<name>",
     *  `channel(id, i)` of an input's channel. Read in a binding, they
     *  follow every measurement. Numbers from Rust, no object per
     *  measurement: parsed as JSON 10
     *  times a second, the levels fed the JS garbage collector, 20 % of the
     *  GUI thread with 31 inputs (29.10.2026). */
    readonly property QtObject levels: QtObject {
        function peak(key) {
            void root.backend.levelsTick
            const db = root.backend.peak(key)
            return isNaN(db) ? undefined : db
        }
        function channel(id, index) {
            void root.backend.levelsTick
            const db = root.backend.channel(id, index)
            return isNaN(db) ? undefined : db
        }
    }
    readonly property var recent: backend.recent.length > 0 ? JSON.parse(backend.recent) : []

    /** Send an lmix action (the API's JSON, as an object). */
    function act(action) {
        backend.act(JSON.stringify(action))
    }

    function inputById(id) {
        return show ? show.inputById(id) || null : null
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

    /** The pages made from `pageEntries`, in their order: in the stack
     *  here, or each in a window of its own (`pageWindows`). */
    property var pageItems: []
    /** The windows of detached pages (PageWindow), by page id. */
    property var pageWindows: ({})
    /** Page windows given back, until they are gone (remake). */
    property var leavingWindows: []
    /** lmix quits: windows that close now keep their pages out. */
    property bool quitting: false
    readonly property var pageOrder: {
        var ids = []
        for (var i = 0; i < pageEntries.length; i++) {
            ids.push(pageEntries[i].pageId)
        }
        return ids
    }
    property string currentPage: "mix"
    readonly property var currentItem: pageItems.find(p => p && p.pageId === currentPage && !isDetached(p.pageId)) || null

    function isDetached(id) {
        return pageWindows[id] !== undefined
    }
    /** A detached page with its own program picture is on screen: the
     *  bar then shows no second one. */
    readonly property bool programElsewhere: pageItems.some(p => p && p.hasProgram && isDetached(p.pageId)
        && pageWindows[p.pageId].visibility !== Window.Minimized && pageWindows[p.pageId].visibility !== Window.Hidden)

    Component {
        id: pageWindowComponent
        PageWindow {}
    }

    /** Make page `index`: in its own window when the settings say it is out. */
    function makePage(index) {
        const entry = pageEntries[index]
        if (!arrangement.isDetached(entry.pageId)) {
            return entry.page.createObject(stack, { shell: root })
        }
        const win = pageWindowComponent.createObject(null, { shell: root, pageId: entry.pageId })
        const page = entry.page.createObject(win.pageArea, { shell: root })
        win.page = page
        win.dockRequested.connect(() => root.dockPage(entry.pageId))
        win.placed.connect(() => root.rememberPlace(entry.pageId))
        const windows = Object.assign({}, pageWindows)
        windows[entry.pageId] = win
        pageWindows = windows
        placeWindow(win, arrangement.places[entry.pageId] || {})
        return page
    }

    /** Put `win` where `place` says, on a screen that is there (on Wayland
     *  the desktop places windows; size and screen still count). The first
     *  time on another screen than this window's, if there is one. */
    function placeWindow(win, place) {
        const screens = Qt.application.screens
        const size = { width: Math.max(win.minimumWidth, root.width), height: Math.max(win.minimumHeight, root.height) }
        const fallback = place.screen === undefined ? arrangement.otherScreen(screens, root.screen) : root.screen
        const at = arrangement.fit(place, screens, fallback, size)
        if (at.screen) {
            win.screen = at.screen
        }
        win.x = at.x
        win.y = at.y
        win.width = at.width
        win.height = at.height
        if (at.visibility === "maximized") {
            win.showMaximized()
        } else if (at.visibility === "fullscreen") {
            win.showFullScreen()
        } else {
            win.show()
        }
    }

    function rememberPlace(id) {
        const win = pageWindows[id]
        if (win !== undefined && !quitting) {
            pageSettings.windows = arrangement.changed(id, true, arrangement.placeOf(win, win.visibility))
        }
    }

    /** Page `id` made anew where the settings now say: in the stack or in
     *  a window of its own. What the page held only on screen (a
     *  selection) goes; what it keeps in the settings stays. The old page
     *  goes deferred; no picture runs twice, as a LiveView asks for its
     *  picture only once its video item is ready in the new window. */
    function remake(id) {
        const index = pageOrder.indexOf(id)
        const old = pageItems[index]
        const win = pageWindows[id]
        if (win !== undefined) {
            const windows = Object.assign({}, pageWindows)
            delete windows[id]
            pageWindows = windows
        }
        if (old) {
            old.destroy()
        }
        const items = pageItems.slice()
        items[index] = makePage(index)
        pageItems = items
        if (win !== undefined) {
            win.page = null
            // Held until it is gone: the garbage collector would take it
            // down with the old page still in it.
            leavingWindows.push(win)
            win.Component.destruction.connect(() => {
                const at = leavingWindows.indexOf(win)
                if (at >= 0) {
                    leavingWindows.splice(at, 1)
                }
            })
            win.release(old)
        }
    }

    /** Page `id` into a window of its own; the main window keeps at least
     *  one page. */
    function detachPage(id) {
        if (!arrangement.canDetach(pageOrder, id)) {
            return
        }
        if (currentPage === id) {
            showPage(arrangement.dockedAfter(pageOrder, id))
        }
        pageSettings.windows = arrangement.changed(id, true, {})
        remake(id)
        showPage(id)
    }

    /** Page `id` back into the main window, shown. */
    function dockPage(id) {
        // Not while lmix quits: the pages stay out for the next start.
        if (!isDetached(id) || quitting) {
            return
        }
        rememberPlace(id)
        pageSettings.windows = arrangement.changed(id, false, {})
        remake(id)
        showPage(id)
        root.raise()
        root.requestActivate()
    }

    // The current page's tools sit in the header, a detached page's in its
    // window's; the others go back to their page.
    onPageItemsChanged: placeTools()
    function placeTools() {
        for (var i = 0; i < pageItems.length; i++) {
            const page = pageItems[i]
            if (!page) {
                continue
            }
            const win = pageWindows[page.pageId]
            page.toolsItem.parent = win !== undefined ? win.toolsSlot
                : page === currentItem ? pageToolsSlot : page
        }
    }
    /** Page `id` to the front: in the main window, or its own window. */
    function showPage(id) {
        if (pageOrder.indexOf(id) < 0) {
            return
        }
        const win = pageWindows[id]
        if (win !== undefined) {
            if (win.visibility === Window.Minimized) {
                win.showNormal()
            }
            win.raise()
            win.requestActivate()
            if (win.page) {
                win.page.takeFocus()
            }
            return
        }
        currentPage = id
        pageSettings.current = id
        // From a page's own window (Ctrl+n, a link to a page): here now.
        if (pageWindowActive) {
            root.raise()
            root.requestActivate()
        }
    }
    onCurrentItemChanged: {
        placeTools()
        if (currentItem) {
            Qt.callLater(currentItem.takeFocus)
        }
    }

    /** The window the operator works in: a page's own window while it is
     *  in front, else this one. Shell dialogs, removals and messages go there. */
    function operatorWindow() {
        for (const id in pageWindows) {
            if (pageWindows[id].active) {
                return pageWindows[id]
            }
        }
        return root
    }
    readonly property bool pageWindowActive: Object.keys(pageWindows).some(id => pageWindows[id].active)
    /** Where the last shell dialog opened (openHere): the keyboard goes
     *  back there when it closes. */
    property var dialogWindow: null
    /** Open one of the window's popups in the window the operator works in. */
    function openHere(popup) {
        const win = operatorWindow()
        dialogWindow = win
        popup.parent = win.overlay
        popup.open()
    }
    /** The same for a platform dialog (files, folders). */
    function openFileHere(dialog) {
        const win = operatorWindow()
        dialogWindow = win
        dialog.parentWindow = win
        dialog.open()
    }
    onRestoreFocus: {
        const win = dialogWindow
        dialogWindow = null
        for (const id in pageWindows) {
            if (pageWindows[id] === win) {
                win.requestActivate()
                if (win.page) {
                    win.page.takeFocus()
                }
                return
            }
        }
        root.requestActivate()
        if (currentItem) {
            currentItem.takeFocus()
        }
    }
    // Closing the main window ends lmix, also with pages out: they open
    // again where they were at the next start.
    onClosing: {
        quitting = true
        for (const id in pageWindows) {
            pageWindows[id].close()
            pageWindows[id].empty()
        }
    }

    // Ctrl+1…n: the pages in the order of the bar, from every window.
    Instantiator {
        model: root.pageOrder.length
        delegate: Shortcut {
            required property int index
            sequence: "Ctrl+" + (index + 1)
            context: Qt.ApplicationShortcut
            enabled: !root.keyTaken(sequence)
            onActivated: root.showPage(root.pageOrder[index])
        }
    }

    // --- shortcuts of the show ---

    readonly property var shortcuts: show && show.shortcuts ? show.shortcuts : []
    /** Shortcuts whose key runs them: one for all shows gives way to the
     *  show's with the same key (AUT.3). */
    readonly property var keyedShortcuts: shortcuts.filter(s => s.key && !s.key_shadowed)
    readonly property var shortcutKeys: keyedShortcuts.map(s => s.key)
    /** A show's shortcut has `sequence`: it wins over the window's own. Qt
     *  reads "ctrl+z" as "Ctrl+Z" and "Shift+Ctrl+S" as "Ctrl+Shift+S". */
    function keyTaken(sequence) {
        const wanted = keyForm(sequence)
        return shortcutKeys.some(k => keyForm(k) === wanted)
    }
    /** A key sequence without case and with its modifiers in one order. */
    function keyForm(sequence) {
        const parts = String(sequence).toLowerCase().split("+")
        const key = parts.pop()
        return parts.sort().concat([key]).join("+")
    }

    // The shortcuts (AUT.1–3) while an lmix window is in front (this one or
    // a page's own); they win over the pages' own keys.
    Instantiator {
        model: root.keyedShortcuts
        delegate: Shortcut {
            required property var modelData
            sequence: modelData.key
            context: Qt.ApplicationShortcut
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
        openHere(shortcutsDialog)
    }

    /** The keys overview with the keys of `page` (the current page, or a
     *  detached one from its window). */
    property var keysPage: null
    function openKeys(page) {
        keysPage = page
        openHere(keysDialog)
    }

    // --- file commands: the File menu and their keys ---

    readonly property var fileKeys: ({ newShow: "Ctrl+N", open: "Ctrl+O", save: "Ctrl+S", saveAs: "Ctrl+Shift+S", shortcuts: "Ctrl+K" })

    /** A File key as the menu shows it; none while a show's shortcut has the key. */
    function fileKeyLabel(sequence) {
        return keyTaken(sequence) ? "" : Texts.keyLabel(sequence)
    }

    function fileCommand(name) {
        if (!backend.connected) {
            return
        }
        switch (name) {
        case "new": openHere(newShowDialog); break
        case "open": openFileHere(openDialog); break
        case "save": root.show && root.show.path ? root.act({ action: "save_show" }) : openFileHere(saveDialog); break
        case "saveAs": openFileHere(saveDialog); break
        case "shortcuts": openShortcuts(); break
        }
    }

    // A show's own shortcut on the same key wins, as for the pages.
    Instantiator {
        model: [["new", root.fileKeys.newShow], ["open", root.fileKeys.open], ["save", root.fileKeys.save],
                ["saveAs", root.fileKeys.saveAs], ["shortcuts", root.fileKeys.shortcuts]]
        delegate: Shortcut {
            required property var modelData
            sequence: modelData[1]
            context: Qt.ApplicationShortcut
            enabled: !root.keyTaken(sequence)
            onActivated: root.fileCommand(modelData[0])
        }
    }

    /** Opens one of the window's dialogs by name, for the Setup page. */
    function openShellDialog(name) {
        switch (name) {
        case "newShow": openHere(newShowDialog); break
        case "exportPackage": openFileHere(exportDialog); break
        case "importPackage": openFileHere(importDialog); break
        case "exportSettings": openFileHere(exportSettingsDialog); break
        case "importSettings": openFileHere(importSettingsDialog); break
        case "resetSettings": openHere(resetSettingsDialog); break
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
            // In the window of the list it was asked from.
            shortcutEditDialog.parent = shortcutsDialog.parent
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
        // The pages, each where it was: here or in its own window. Not
        // every page out: the first stays here then.
        if (!pageOrder.some(id => !arrangement.isDetached(id)) && pageOrder.length > 0) {
            pageSettings.windows = arrangement.changed(pageOrder[0], false, {})
        }
        var items = []
        for (var i = 0; i < pageEntries.length; i++) {
            items.push(makePage(i))
        }
        pageItems = items
        // `--page` for this run, else the page open last; a detached one
        // comes to the front, this window shows the first page here.
        const start = backend.startPage !== "" ? backend.startPage : pageSettings.current
        showPage(start)
        if (!currentItem) {
            const docked = pageOrder.find(id => !isDetached(id))
            if (docked !== undefined) {
                currentPage = docked
            }
        }
        // Start options (UI.12): `--window maximized|fullscreen`.
        if (backend.windowMode === "maximized") {
            root.showMaximized()
        } else if (backend.windowMode === "fullscreen") {
            root.showFullScreen()
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
    FactoryResetDialog {
        id: resetSettingsDialog
        onClosed: root.restoreFocus()
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.act
        onBackupRequested: openFileHere(exportSettingsDialog)
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
            importFolderDialog.parentWindow = parentWindow
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

    KeysDialog {
        id: keysDialog
        onClosed: root.restoreFocus()
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        always: [
            { what: qsTr("Pages"), key: Texts.keyLabel("Ctrl+1…" + root.pageOrder.length) },
        ].concat([
            { what: qsTr("New show"), key: root.fileKeys.newShow },
            { what: qsTr("Open show"), key: root.fileKeys.open },
            { what: qsTr("Save show"), key: root.fileKeys.save },
            { what: qsTr("Save show as"), key: root.fileKeys.saveAs },
            { what: qsTr("Shortcuts"), key: root.fileKeys.shortcuts }
        ].filter(k => !root.keyTaken(k.key)).map(k => ({ what: k.what, key: Texts.keyLabel(k.key) })), [
            { what: qsTr("Panic all"), gesture: qsTr("button in the page bar or at the foot of a page's window") }
        ])
        pageTitle: root.keysPage ? root.keysPage.title : ""
        pageKeys: root.keysPage ? root.keysPage.pageKeys : []
        own: root.keyedShortcuts.map(s => ({ what: s.label, key: Texts.keyLabel(s.key) }))
        onShortcutsRequested: root.openShortcuts()
    }

    // --- the window ---

    readonly property var runningOutputs: show ? show.outputs.filter(o => o.status && o.status.state === "running") : []
    readonly property bool recording: runningOutputs.some(o => o.spec.kind === "record")
    readonly property bool streaming: runningOutputs.some(o => o.spec.kind === "srt" || o.spec.kind === "rtmp")
    /** lmix-core's machine and cue timer (Core.machine, RUN.8), or null. */
    readonly property var machine: Texts.machine(backend.machine)
    readonly property var machineWarning: Texts.machineWarning(machine)

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
                        onClicked: {
                            addPageItems()
                            fileMenu.popup(fileButton, 0, fileButton.height)
                        }
                        // The pages' entries go to the exchange, after the package entries;
                        // at each opening, when the menu and the pages are complete: a page
                        // made anew (detached, docked) brings new ones, its old ones left
                        // the menu with it.
                        function addPageItems() {
                            var at = fileMenu.count
                            for (var k = 0; k < fileMenu.count; k++) {
                                if (fileMenu.itemAt(k) === importPackageItem) {
                                    at = k + 1
                                }
                            }
                            for (var p = 0; p < root.pageItems.length; p++) {
                                var items = root.pageItems[p] ? root.pageItems[p].fileItems : []
                                for (var i = 0; i < items.length; i++) {
                                    var there = -1
                                    for (var m = at; m < fileMenu.count; m++) {
                                        if (fileMenu.itemAt(m) === items[i]) {
                                            there = m
                                        }
                                    }
                                    if (there >= 0) {
                                        at = there + 1
                                    } else {
                                        fileMenu.insertItem(at++, items[i])
                                    }
                                }
                            }
                        }
                        QQC2.Menu {
                            id: fileMenu
                            KantePopupSkin { popup: fileMenu }
                            // Sub menus get their row from here: icon, value.
                            delegate: KanteMenuItem {
                                iconRoom: true
                                icon.name: subMenu && subMenu.iconName ? subMenu.iconName : ""
                                valueText: subMenu && subMenu.valueText ? subMenu.valueText : ""
                            }
                            KanteMenuSection { text: qsTr("Show"); rule: false }
                            KanteMenuItem {
                                text: qsTr("New")
                                icon.name: "document-new"
                                shortcutText: root.fileKeyLabel(root.fileKeys.newShow)
                                onTriggered: root.fileCommand("new")
                            }
                            KanteMenuItem {
                                text: qsTr("Open…")
                                icon.name: "document-open"
                                shortcutText: root.fileKeyLabel(root.fileKeys.open)
                                onTriggered: root.fileCommand("open")
                            }
                            ChoiceMenu {
                                title: qsTr("Recent")
                                readonly property string iconName: "document-open-recent"
                                enabled: root.recent.length > 0
                                choices: root.recent.map(path => ({ value: path, label: path }))
                                onPicked: value => root.act({ action: "open_show", path: value })
                            }
                            KanteMenuItem {
                                text: qsTr("Save")
                                icon.name: "document-save"
                                shortcutText: root.fileKeyLabel(root.fileKeys.save)
                                onTriggered: root.fileCommand("save")
                            }
                            KanteMenuItem {
                                text: qsTr("Save as…")
                                icon.name: "document-save-as"
                                shortcutText: root.fileKeyLabel(root.fileKeys.saveAs)
                                onTriggered: root.fileCommand("saveAs")
                            }
                            KanteMenuSection { id: exchangeSection; text: qsTr("Exchange") }
                            KanteMenuItem {
                                text: qsTr("Export package…")
                                icon.name: "document-export"
                                onTriggered: root.openFileHere(exportDialog)
                            }
                            KanteMenuItem {
                                id: importPackageItem
                                text: qsTr("Import package…")
                                icon.name: "document-import"
                                onTriggered: root.openFileHere(importDialog)
                            }
                            KanteMenuSection { text: qsTr("Settings") }
                            KanteMenuItem {
                                text: qsTr("Shortcuts…")
                                icon.name: "configure-shortcuts"
                                shortcutText: root.fileKeyLabel(root.fileKeys.shortcuts)
                                onTriggered: root.fileCommand("shortcuts")
                            }
                            ChoiceMenu {
                                title: qsTr("Style")
                                readonly property string iconName: "draw-brush"
                                readonly property string valueText: choices.filter(c => c.value === current).map(c => c.label).join("")
                                current: KanteStyle.kind
                                choices: [
                                    { value: KanteStyle.Kind.Kante, label: qsTr("Kante") },
                                    { value: KanteStyle.Kind.KanteLight, label: qsTr("Kante Light") },
                                    { value: KanteStyle.Kind.System, label: qsTr("System") }
                                ]
                                onPicked: value => root.setStyle(value)
                            }
                            ChoiceMenu {
                                title: qsTr("Page bar")
                                readonly property string iconName: "view-split-top-bottom"
                                readonly property string valueText: choices.filter(c => c.value === current).map(c => c.label).join("")
                                current: settings.pageBar
                                choices: [
                                    { value: KantePageBar.Density.Normal, label: qsTr("Normal") },
                                    { value: KantePageBar.Density.Compact, label: qsTr("Narrow") },
                                    { value: KantePageBar.Density.Band, label: qsTr("Colour band") }
                                ]
                                onPicked: value => settings.pageBar = value
                            }
                            // Settings of this machine (SHW.8): rarely, so one level down;
                            // factory settings last and red.
                            QQC2.Menu {
                                title: qsTr("This machine")
                                readonly property string iconName: "configure"
                                id: machineMenu
                                KantePopupSkin { popup: machineMenu }
                                KanteMenuItem {
                                    text: qsTr("Export settings…")
                                    icon.name: "document-export"
                                    onTriggered: root.openFileHere(exportSettingsDialog)
                                }
                                KanteMenuItem {
                                    text: qsTr("Import settings…")
                                    icon.name: "document-import"
                                    onTriggered: root.openFileHere(importSettingsDialog)
                                }
                                QQC2.MenuSeparator {}
                                KanteMenuItem {
                                    text: qsTr("Factory settings…")
                                    icon.name: "edit-reset"
                                    destructive: true
                                    onTriggered: root.openHere(resetSettingsDialog)
                                }
                            }
                        }
                    }
                    QQC2.Label {
                        visible: root.currentItem !== null
                        text: "|"
                        color: KanteStyle.mutedTextColor
                        Accessible.ignored: true
                    }
                    // The current page's title row (AppPage): title, subtitle, tools.
                    KanteHeading {
                        visible: root.currentItem !== null
                        level: 2
                        pageTitle: true
                        text: root.currentItem ? root.currentItem.title : ""
                    }
                    QQC2.Label {
                        visible: root.currentItem !== null && root.currentItem.subtitle.length > 0
                        text: root.currentItem ? root.currentItem.subtitle : ""
                        font.family: KanteStyle.monoFamily
                        color: KanteStyle.mutedTextColor
                        elide: Text.ElideRight
                        Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                    }
                    RowLayout {
                        id: pageToolsSlot
                        spacing: Kirigami.Units.smallSpacing
                    }
                    // The current page to a window of its own (a second screen).
                    KanteToolButton {
                        objectName: "detachButton"
                        visible: root.currentItem !== null
                        enabled: root.currentItem !== null && arrangement.canDetach(root.pageOrder, root.currentItem.pageId)
                        focusPolicy: Qt.NoFocus
                        icon.name: "tab-detach"
                        display: QQC2.AbstractButton.IconOnly
                        text: qsTr("Own window")
                        Accessible.name: qsTr("Page in its own window")
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: enabled
                            ? qsTr("Put this page into a window of its own, e.g. on a second screen; closing that window brings it back")
                            : qsTr("The main window keeps one page")
                        onClicked: root.detachPage(root.currentItem.pageId)
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
                    // The machine's load, and a warning when cue times may
                    // not hold (RUN.8); the tooltip says why.
                    QQC2.Label {
                        visible: root.machine !== null && root.machineWarning.level === 0
                        text: Texts.machineLoad(root.machine)
                        font.family: KanteStyle.monoFamily
                        color: KanteStyle.mutedTextColor
                        HoverHandler { id: loadHover }
                        QQC2.ToolTip.visible: loadHover.hovered
                        QQC2.ToolTip.text: Texts.cueTimer(root.machine) + "\n" + root.machineWarning.detail
                    }
                    KantePill {
                        visible: root.machineWarning.level > 0
                        status: root.machineWarning.level > 1 ? KantePill.State.Failed : KantePill.State.Locked
                        text: root.machineWarning.text
                        Accessible.role: Accessible.StaticText
                        Accessible.name: root.machineWarning.text
                        Accessible.description: root.machineWarning.detail
                        HoverHandler { id: warnHover }
                        QQC2.ToolTip.visible: warnHover.hovered
                        QQC2.ToolTip.text: root.machineWarning.detail
                    }
                    // Steady while connected: a breathing pill would redraw
                    // the whole window at the display's rate, all the time.
                    KantePill {
                        status: root.backend.connected ? KantePill.State.Done : KantePill.State.Failed
                        text: root.backend.connected ? qsTr("lmix-core") : qsTr("lmix-core offline")
                    }
                    KanteButton {
                        focusPolicy: Qt.NoFocus
                        emphasis: KanteButton.Emphasis.Quiet
                        text: qsTr("Keys")
                        onClicked: root.openKeys(root.currentItem)
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
                    // Global: the lock refuses what would break the show (closing
                    // inputs, loading a show, stopping outputs, …) on every page.
                    KanteButton {
                        readonly property bool locked: root.show !== null && root.show.locked === true
                        focusPolicy: Qt.NoFocus
                        size: KanteButton.Size.Small
                        text: locked ? qsTr("Locked") : qsTr("Lock")
                        emphasis: locked ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal
                        enabled: root.backend.connected
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: qsTr("While locked, closing inputs, loading a show, stopping outputs and changing format, sample rate or multiview are refused.")
                        onClicked: root.act({ action: "set_lock", on: !locked })
                    }
                    // Up here only while the page bar is a band (it has no room).
                    PanicButtons {
                        visible: settings.pageBar === KantePageBar.Density.Band
                        shell: root
                        page: root.currentItem
                    }
                }
            }

            // Banners for what lasts (rule 2, docs/DESIGN-DIALOGE.md);
            // everything else is a toast.
            KanteBanner {
                Layout.fillWidth: true
                visible: !root.backend.connected
                danger: true
                title: qsTr("lmix-core offline")
                text: qsTr("The Console tries again every second. The engine and its outputs keep running.")
            }
            // An older show file, converted on opening (SHW.7).
            KanteBanner {
                Layout.fillWidth: true
                visible: root.show !== null && !!root.show.older_format
                title: qsTr("Older show format")
                text: root.show && root.show.older_format
                    ? qsTr("This show file is in the older format %1 and was converted on opening. Saving it keeps the old file beside it (.v%1.bak).").arg(root.show.older_format)
                    : ""
            }

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.margins: Kirigami.Units.largeSpacing

                StackLayout {
                    id: stack
                    anchors.fill: parent
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

                MessageToasts {
                    id: toasts
                    anchors.right: parent.right
                    anchors.bottom: parent.bottom
                    z: 10
                    core: root.backend
                    // Messages show in the window the operator works in.
                    listening: !root.pageWindowActive
                }
                Removals {
                    id: removals
                    core: root.backend
                    act: root.act
                    show: root.show
                    column: toasts.column
                    undoKeyFree: !root.keyTaken("Ctrl+Z")
                }
            }

            // The page bar (KantePageBar): the pages, then program small
            // where a page has none of its own (not in the band).
            KantePageBar {
                id: pageBar
                Layout.fillWidth: true
                density: settings.pageBar
                // A page in its own window is marked; a click brings that
                // window to the front (the main window keeps its page).
                readonly property var pages: root.pageItems.filter(p => p)
                model: pages.map(p => root.isDetached(p.pageId) ? qsTr("%1 ↗", "page in its own window").arg(p.title) : p.title)
                tooltips: pages.map((p, i) => root.isDetached(p.pageId)
                    ? qsTr("%1 in its own window: to the front (Ctrl+%2)").arg(p.title).arg(i + 1)
                    : qsTr("%1 (Ctrl+%2)").arg(p.title).arg(i + 1))
                busy: pages.map(p => p.busy)
                busyKinds: pages.map(p => p.busyKind)
                kindNames: [qsTr("Running"), qsTr("Info"), qsTr("Warning"), qsTr("Error")]
                onActivated: index => {
                    root.showPage(pages[index].pageId)
                    // The bar set its index itself; it shows this window's page.
                    currentIndex = pages.indexOf(root.currentItem)
                }
                // A Binding, not a property binding: the bar sets its index
                // itself on a click, which would cut a plain binding; this
                // writes it again on every page change (Ctrl+n, --page).
                Binding on currentIndex {
                    value: pageBar.pages.indexOf(root.currentItem)
                }

                QQC2.Label {
                    text: Texts.keyLabel("Ctrl+1…" + root.pageOrder.length)
                    font.family: KanteStyle.monoFamily
                    color: KanteStyle.mutedTextColor
                }
                Rectangle {
                    visible: root.currentItem !== null && !root.currentItem.hasProgram && !root.programElsewhere
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
                PanicButtons {
                    shell: root
                    page: root.currentItem
                }
            }
        }
    }
}
