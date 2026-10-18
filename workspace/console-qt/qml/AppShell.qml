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

    Component.onCompleted: {
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

    // This machine's settings (SHW.8): shortcuts for all shows, layout
    // templates, targets, subscribers; no secrets.
    FileDialog {
        id: exportSettingsDialog
        title: qsTr("Export settings")
        fileMode: FileDialog.SaveFile
        defaultSuffix: "json"
        nameFilters: [qsTr("lmix settings (*.json)")]
        onAccepted: root.act({ action: "export_settings", path: root.localPath(selectedFile) })
    }
    FileDialog {
        id: importSettingsDialog
        title: qsTr("Import settings")
        nameFilters: [qsTr("lmix settings (*.json)"), qsTr("All files (*)")]
        onAccepted: root.act({ action: "import_settings", path: root.localPath(selectedFile) })
    }
    KanteDialog {
        id: resetSettingsDialog
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

    // SHW.3: a new show starts in the format chosen here.
    FormatDialog {
        id: newShowDialog
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

            RowLayout {
                Layout.fillWidth: true
                spacing: Kirigami.Units.smallSpacing

                KanteButton {
                    text: qsTr("New")
                    enabled: root.backend.connected
                    onClicked: newShowDialog.open()
                }
                KanteButton {
                    text: qsTr("Open…")
                    enabled: root.backend.connected
                    onClicked: openDialog.open()
                }
                KanteButton {
                    id: recentButton
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
                    text: qsTr("Save")
                    enabled: root.backend.connected
                    onClicked: root.show && root.show.path ? root.act({ action: "save_show" }) : saveDialog.open()
                }
                KanteButton {
                    text: qsTr("Save as…")
                    enabled: root.backend.connected
                    onClicked: saveDialog.open()
                }
                KanteButton {
                    id: packageButton
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
