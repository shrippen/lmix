import QtQuick
import QtCore
import Kante
import org.lmix.console

/**
 * The Show page: runs prepared cues with GO (LASTENHEFT-SHOW.md). Keys:
 * space GO and Escape panic in the show view, the shortcuts in the window
 * and, by default, at the desktop (keys/global).
 */
AppPage {
    id: root

    /** The window (AppShell): show, actions, helpers. */
    required property var shell

    pageId: "show"
    title: qsTr("Show")
    subtitle: shell.show && shell.show.cue_lists ? qsTr("Cue lists: %1").arg(shell.show.cue_lists.length) : ""
    busy: shell.show !== null && (shell.show.running_cues || []).length > 0
    focusTarget: showView
    pageKeys: [
        { what: qsTr("GO"), key: qsTr("Space") },
        { what: qsTr("Panic, twice: stop at once"), key: qsTr("Escape") }
    ]
    // The show view has its own panic (SUI.3): stops only the cues.

    Settings {
        id: viewSettings
        category: "view"
        /** Waits and follows in the show view (SUI.13). */
        property bool showAdvanced: false
        /** The show view dimmed for a dark room (SUI.6). */
        property bool darkRoom: false
        /** Edit mode; off in show mode, where cues cannot be changed. */
        property bool editing: true
    }

    tools: [
        KanteSegmented {
            activeFocusOnTab: false
            model: [qsTr("Edit"), qsTr("Show")]
            currentIndex: viewSettings.editing ? 0 : 1
            tooltips: [qsTr("Cues, zones, targets and the script can be changed"), qsTr("Only running the show: no buttons that change it")]
            onActivated: index => {
                viewSettings.editing = index === 0
                root.takeFocus()
            }
        }
    ]

    ShowView {
        id: showView
        anchors.fill: parent
        act: root.shell.act
        show: root.shell.show
        core: root.shell.backend
        levels: root.shell.levels
        advanced: viewSettings.showAdvanced
        onAdvancedChanged: viewSettings.showAdvanced = advanced
        darkRoom: viewSettings.darkRoom
        onDarkRoomChanged: viewSettings.darkRoom = darkRoom
        editing: viewSettings.editing
    }

    // Dark room (SUI.6): Kante's dark scheme (no white), and the window
    // under its scrim; clicks go through.
    Binding {
        target: KanteStyle
        property: "preferDark"
        value: true
        when: viewSettings.darkRoom && root.visible
        restoreMode: Binding.RestoreBindingOrValue
    }
    Rectangle {
        parent: root.shell.contentItem
        anchors.fill: parent
        z: 1000
        color: KanteStyle.scrimColor
        opacity: 0.6
        // Only over the Show page: the other pages stay as they are.
        visible: viewSettings.darkRoom && root.visible
    }
}
