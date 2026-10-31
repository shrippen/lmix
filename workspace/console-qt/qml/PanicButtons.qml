import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * A page's own panic (Mix: FTB), then the one for everything ("Panic all",
 * AppShell.panicAll). In the page bar of the window (the current page's),
 * in its header while the bar is a band, and at the foot of a page's own
 * window (that page's).
 */
RowLayout {
    id: panics

    /** The window (AppShell): panicAll, backend. */
    required property var shell
    /** The page whose panic the first button is (AppPage), or null. */
    property var page: null

    spacing: Kirigami.Units.smallSpacing

    KanteButton {
        id: pagePanic
        readonly property bool active: panics.page !== null && panics.page.panicActive
        visible: panics.page !== null && panics.page.panicText.length > 0
        focusPolicy: Qt.NoFocus
        emphasis: KanteButton.Emphasis.Destructive
        size: KanteButton.Size.Small
        text: panics.page ? panics.page.panicText : ""
        QQC2.ToolTip.visible: hovered && panics.page !== null && panics.page.panicTip.length > 0
        QQC2.ToolTip.text: panics.page ? panics.page.panicTip : ""
        onClicked: panics.page.panicRequested()
        // In force (program black): the button pulses until it is ended.
        SequentialAnimation on opacity {
            running: pagePanic.active && panics.visible
            loops: Animation.Infinite
            NumberAnimation { to: 0.45; duration: 600 }
            NumberAnimation { to: 1; duration: 600 }
            onRunningChanged: if (!running) pagePanic.opacity = 1
        }
    }
    KanteButton {
        focusPolicy: Qt.NoFocus
        emphasis: KanteButton.Emphasis.Destructive
        size: KanteButton.Size.Small
        text: qsTr("Panic", "stops everything")
        Accessible.name: qsTr("Panic all")
        enabled: panics.shell.backend.connected
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.text: qsTr("Stop every cue over the panic time and fade program to black; recordings and streams go on")
        onClicked: panics.shell.panicAll()
    }
}
