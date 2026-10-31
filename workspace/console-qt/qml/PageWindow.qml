import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * A page in a window of its own, e.g. on a second screen (docs/PLAN-SEITEN.md,
 * "Seiten lösbar"). AppShell makes the page in here and takes it back
 * (`dockRequested`: the button, or closing the window). The window holds no
 * connection and no picture of its own: the page talks to the shell's
 * Core, and its input tiles draw their cells of the shell's tile picture
 * (`tilesAtlas`, LiveView). Its toasts, removals and the shell's dialogs
 * opened from here show in this window (AppShell.operatorWindow).
 *
 *   SHOW  Cue lists: 2  tools …                 20:14  [Keys] [Back to the main window]
 *   ┌ page ───────────────────────────────────────────────────────────────┐
 *   └─────────────────────────────────────────────────────────────────────┘
 *                                                     [Stop show] [Panic]
 */
QQC2.ApplicationWindow {
    id: win

    /** The window (AppShell): show, backend, panicAll, … */
    required property var shell
    required property string pageId
    /** The page shown here (AppPage); AppShell sets it. */
    property Item page: null
    /** Where AppShell makes the page. */
    readonly property alias pageArea: area
    /** Where the page's tools go (AppShell.placeTools). */
    readonly property alias toolsSlot: toolsSlot
    /** LiveView takes its tile picture from its window: the shell's one. */
    readonly property var tilesAtlas: shell.tilesAtlas
    readonly property var removals: removalsLoader.item
    readonly property alias toasts: toasts
    /** Where it was and how it was shown (PageArrangement.placeOf), after
     *  it moved or changed size; AppShell keeps it. */
    signal placed()
    signal dockRequested()

    title: page ? qsTr("lmix — %1").arg(page.title) : "lmix"
    color: KanteStyle.backgroundColor
    minimumWidth: Kirigami.Units.gridUnit * 30
    minimumHeight: Kirigami.Units.gridUnit * 20

    /** The page went back: hide, then go. Its dialogs go first, while the
     *  window still stands: torn down with the window, a Kante dialog
     *  restored its background into an item already gone (SIGSEGV in
     *  QQmlBind, tst_app_shell). */
    function release() {
        visible = false
        removalsLoader.active = false
        destroy(100)
    }
    /** lmix quits: the page and its dialogs go now, before the engine
     *  takes the window down (the same crash at the end). */
    function empty() {
        if (page) {
            page.destroy()
            page = null
        }
        removalsLoader.active = false
    }

    onPageChanged: {
        if (page) {
            page.anchors.fill = area
        }
    }

    KanteScope {
        target: win.contentItem
    }

    // Closing it brings the page back, as the button does; not when lmix quits.
    onClosing: close => {
        if (!win.shell.quitting) {
            close.accepted = false
            win.dockRequested()
        }
    }

    // Moving or resizing comes in many steps: keep the place once it rests.
    Timer {
        id: settle
        interval: 500
        onTriggered: win.placed()
    }
    onXChanged: settle.restart()
    onYChanged: settle.restart()
    onWidthChanged: settle.restart()
    onHeightChanged: settle.restart()
    onVisibilityChanged: settle.restart()
    onScreenChanged: settle.restart()

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // The page's title row, as in the main window's header; the
        // buttons take no focus (space and Escape stay with the page).
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
                    text: win.page ? win.page.title : ""
                }
                QQC2.Label {
                    visible: text.length > 0
                    text: win.page ? win.page.subtitle : ""
                    font.family: KanteStyle.monoFamily
                    color: KanteStyle.mutedTextColor
                    elide: Text.ElideRight
                    Layout.maximumWidth: Kirigami.Units.gridUnit * 20
                }
                RowLayout {
                    id: toolsSlot
                    spacing: Kirigami.Units.smallSpacing
                }
                Item {
                    Layout.fillWidth: true
                }
                // Recording and stream stay in the main window: their pills
                // breathe, and that would redraw this window at the display's
                // rate too, all the time.
                KantePill {
                    visible: !win.shell.backend.connected
                    status: KantePill.State.Failed
                    text: qsTr("lmix-core offline")
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
                KanteButton {
                    focusPolicy: Qt.NoFocus
                    emphasis: KanteButton.Emphasis.Quiet
                    text: qsTr("Keys")
                    onClicked: win.shell.openKeys(win.page)
                }
                KanteButton {
                    objectName: "dockButton"
                    focusPolicy: Qt.NoFocus
                    emphasis: KanteButton.Emphasis.Quiet
                    icon.name: "view-restore"
                    text: qsTr("Back to the main window")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Put this page back into the main window; closing this window does the same")
                    onClicked: win.dockRequested()
                }
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.margins: Kirigami.Units.largeSpacing

            Item {
                id: area
                anchors.fill: parent
            }

            MessageToasts {
                id: toasts
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                z: 10
                core: win.shell.backend
                // Messages show in the window the operator works in.
                listening: win.active
            }
            // In a Loader, so it goes before the window (release()).
            Loader {
                id: removalsLoader
                sourceComponent: Removals {
                    core: win.shell.backend
                    act: win.shell.act
                    show: win.shell.show
                    column: toasts.column
                    undoKeyFree: !win.shell.keyTaken("Ctrl+Z")
                }
            }
        }

        // The foot: the page's panic and "Panic all", where the page bar
        // has them in the main window.
        Rectangle {
            Layout.fillWidth: true
            implicitHeight: footRow.implicitHeight + Kirigami.Units.smallSpacing * 2
            color: KanteStyle.sunkenColor
            Rectangle {
                width: parent.width
                height: 1
                color: KanteStyle.ruleColor
            }
            RowLayout {
                id: footRow
                anchors.fill: parent
                anchors.leftMargin: Kirigami.Units.largeSpacing
                anchors.rightMargin: Kirigami.Units.largeSpacing
                Item {
                    Layout.fillWidth: true
                }
                PanicButtons {
                    shell: win.shell
                    page: win.page
                }
            }
        }
    }
}
