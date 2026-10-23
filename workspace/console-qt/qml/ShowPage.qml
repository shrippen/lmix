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
    busyColor: KanteStyle.neutralTextColor
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
    }

    ShowView {
        id: showView
        anchors.fill: parent
        act: root.shell.act
        show: root.shell.show
        advanced: viewSettings.showAdvanced
        onAdvancedChanged: viewSettings.showAdvanced = advanced
        darkRoom: viewSettings.darkRoom
        onDarkRoomChanged: viewSettings.darkRoom = darkRoom
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
