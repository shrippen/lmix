import QtQuick
import QtCore
import Kante
import org.lmix.console

/**
 * lmix Show: runs prepared cues with GO (LASTENHEFT-SHOW.md). The window
 * around it is AppShell, shared with lmix Mixer (DECISIONS 2026-10-18).
 * Global shortcuts stay with the mixer, so a press runs once; the show
 * keeps its window keys (space GO, Escape panic).
 */
AppShell {
    id: root

    appName: qsTr("lmix Show")

    Settings {
        id: viewSettings
        category: "view"
        /** Waits and follows in the show view (SUI.13). */
        property bool showAdvanced: false
        /** The show view dimmed for a dark room (SUI.6). */
        property bool darkRoom: false
    }

    // Space and Escape reach the view from the start.
    Component.onCompleted: showView.forceActiveFocus()

    ShowView {
        id: showView
        anchors.fill: parent
        act: root.act
        show: root.show
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
        when: viewSettings.darkRoom
        restoreMode: Binding.RestoreBindingOrValue
    }
    Rectangle {
        parent: root.contentItem
        anchors.fill: parent
        z: 1000
        color: KanteStyle.scrimColor
        opacity: 0.6
        visible: viewSettings.darkRoom
    }
}
