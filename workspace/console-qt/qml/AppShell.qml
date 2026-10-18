import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import QtCore
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

/**
 * The window both programs share (mixer, show; DECISIONS 2026-10-18):
 * lmix-core, the style, the show file and this machine's settings, the
 * older-format notice and refusals. What a program adds goes into
 * `content` (the default), `fileTools` (after Recent), `tools` (right of
 * the style) and `packageItems` (the Package menu, after Import).
 *
 *   [New][Open][Recent] fileTools [Save][Save as][Package] … [Style] tools
 *   ┌ content ─────────────────────────────────────────────────────────┐
 *   └──────────────────────────────────────────────────────────────────┘
 *   older format · refusal
 */
Kirigami.ApplicationWindow {
    id: root

    /** The program's name in the title bar, e.g. "lmix Mixer". */
    required property string appName
    default property alias content: area.data
    property alias fileTools: fileToolsRow.data
    property alias tools: toolsRow.data
    /** Menu items put into the Package menu after Import. */
    property list<QtObject> packageItems
    /** Bind the keyed shortcuts at the desktop too (AUT.12): the
     *  program's own setting (keys/global). Each program binds its own, so
     *  with both on a press runs twice; the shortcuts dialog says so. */
    property bool globalShortcuts: false
    /** The switch in the shortcuts dialog was used. */
    signal globalShortcutsToggled(bool on)
    /** A shell dialog or menu closed: the program puts the keyboard focus
     *  back where its keys work (the show view: space GO, Escape panic). */
    signal restoreFocus()

    readonly property Core backend: Core {}

    title: show && show.path ? appName + " — " + show.path : appName
    width: Kirigami.Units.gridUnit * 80
    height: Kirigami.Units.gridUnit * 50
    color: KanteStyle.backgroundColor

    Settings {
        id: settings
        category: "appearance"
        /** KanteStyle.Kind; Kante unless chosen otherwise. */
        property int style: KanteStyle.Kind.Kante
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

    readonly property var shortcuts: show && show.shortcuts ? show.shortcuts : []
    /** Shortcuts whose key runs them: one for all shows gives way to the
     *  show's with the same key (AUT.3). */
    readonly property var keyedShortcuts: shortcuts.filter(s => s.key && !s.key_shadowed)
    readonly property var shortcutKeys: keyedShortcuts.map(s => s.key)

    // The shortcuts (AUT.1–3) while this window is in front; they win over
    // the program's own keys.
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
    Connections {
        target: root.backend
        function onGlobalShortcut(id) {
            root.act({ action: "run_shortcut", target: id })
        }
    }

    function openShortcuts() {
        shortcutsDialog.open()
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
        onGlobalKeysToggled: on => root.globalShortcutsToggled(on)
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
        // Start options (UI.12): `--window maximized|fullscreen`.
        if (backend.windowMode === "maximized")
            root.showMaximized()
        else if (backend.windowMode === "fullscreen")
            root.showFullScreen()
        // After Export, Import and the separator.
        for (var i = 0; i < packageItems.length; i++)
            packageMenu.insertItem(3 + i, packageItems[i])
    }

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

    pageStack.initialPage: Kirigami.Page {
        title: root.appName
        padding: Kirigami.Units.largeSpacing

        actions: [
            Kirigami.Action {
                displayComponent: KantePill {
                    status: root.backend.connected ? KantePill.State.Running : KantePill.State.Failed
                    text: root.backend.connected ? qsTr("lmix-core") : qsTr("lmix-core offline")
                }
            }
        ]

        ColumnLayout {
            anchors.fill: parent
            spacing: Kirigami.Units.largeSpacing

            // The buttons take no focus: in the show, space and Escape stay
            // with the show view.
            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                KanteButton {
                    focusPolicy: Qt.NoFocus
                    text: qsTr("New")
                    enabled: root.backend.connected
                    onClicked: newShowDialog.open()
                }
                KanteButton {
                    focusPolicy: Qt.NoFocus
                    text: qsTr("Open…")
                    enabled: root.backend.connected
                    onClicked: openDialog.open()
                }
                KanteButton {
                    id: recentButton
                    focusPolicy: Qt.NoFocus
                    text: qsTr("Recent")
                    enabled: root.backend.connected && root.recent.length > 0
                    onClicked: recentMenu.popup(recentButton, 0, recentButton.height)
                    ChoiceMenu {
                        id: recentMenu
                        choices: root.recent.map(path => ({ value: path, label: path }))
                        onPicked: value => root.act({ action: "open_show", path: value })
                    }
                }
                RowLayout {
                    id: fileToolsRow
                    visible: children.length > 0
                    spacing: Kirigami.Units.smallSpacing
                }
                KanteButton {
                    focusPolicy: Qt.NoFocus
                    text: qsTr("Save")
                    enabled: root.backend.connected
                    onClicked: root.show && root.show.path ? root.act({ action: "save_show" }) : saveDialog.open()
                }
                KanteButton {
                    focusPolicy: Qt.NoFocus
                    text: qsTr("Save as…")
                    enabled: root.backend.connected
                    onClicked: saveDialog.open()
                }
                KanteButton {
                    id: packageButton
                    focusPolicy: Qt.NoFocus
                    text: qsTr("Package")
                    enabled: root.backend.connected
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
                    }
                }
                Item { Layout.fillWidth: true }
                KanteSegmented {
                    activeFocusOnTab: false
                    readonly property var kinds: [KanteStyle.Kind.Kante, KanteStyle.Kind.KanteLight, KanteStyle.Kind.System]
                    model: [qsTr("Kante"), qsTr("Kante Light"), qsTr("System")]
                    currentIndex: kinds.indexOf(KanteStyle.kind)
                    tooltips: [qsTr("Kante: Gruvbox dark, Leinen when the system theme is light"), qsTr("Kante Light: only the shapes of Kante, on the system colours"), qsTr("System style (Breeze)")]
                    onActivated: index => {
                        settings.style = kinds[index]
                        root.backend.style = -1
                    }
                }
                RowLayout {
                    id: toolsRow
                    visible: children.length > 0
                    spacing: Kirigami.Units.smallSpacing
                }
            }

            // The program's part. Height from the page, not from the
            // children: the monitors take what is left.
            Item {
                id: area
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.preferredHeight: 0
            }

            // An older show file, converted on opening (SHW.7).
            QQC2.Label {
                Layout.fillWidth: true
                visible: root.show !== null && !!root.show.older_format
                text: root.show && root.show.older_format
                    ? qsTr("This show file is in the older format %1 and was converted on opening. Saving it keeps the old file beside it (.v%1.bak).").arg(root.show.older_format)
                    : ""
                color: KanteStyle.infoColor
                wrapMode: Text.Wrap
            }

            QQC2.Label {
                Layout.fillWidth: true
                visible: root.backend.message.length > 0
                text: Texts.message(root.backend.message)
                color: KanteStyle.negativeTextColor
                wrapMode: Text.Wrap
            }
        }
    }
}
