import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Layouts: one button each (click: into preview; right click: edit or
 * remove), red on program and green in preview like a tile. "+ Layout"
 * adds one.
 */
RowLayout {
    id: bar

    required property var core
    required property var show
    required property var act
    signal editRequested(var layout)

    readonly property var layouts: show && show.layouts ? show.layouts : []

    spacing: Kirigami.Units.smallSpacing

    KanteSectionLabel { text: qsTr("Layouts") }

    Repeater {
        model: bar.layouts

        Item {
            id: entry
            required property var modelData
            readonly property bool onProgram: bar.show !== null && bar.show.program === modelData.id
            readonly property bool onPreview: bar.show !== null && bar.show.preview === modelData.id

            implicitWidth: button.implicitWidth
            implicitHeight: button.implicitHeight

            KanteButton {
                id: button
                anchors.fill: parent
                text: entry.modelData.number + " " + entry.modelData.name
                emphasis: entry.onProgram ? KanteButton.Emphasis.Destructive
                    : entry.onPreview ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                enabled: bar.core.connected && !entry.onProgram
                onClicked: bar.act({ action: "preview_layout", layout: entry.modelData.number })
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Click: into preview. Right click: edit or remove.")
            }

            // On the item: a disabled button (on program) passes it through.
            TapHandler {
                acceptedButtons: Qt.RightButton
                enabled: bar.core.connected
                onTapped: menu.popup()
            }

            QQC2.Menu {
                id: menu
                KantePopupSkin { popup: menu }
                QQC2.MenuItem {
                    text: qsTr("Edit…")
                    onTriggered: bar.editRequested(entry.modelData)
                }
                QQC2.MenuItem {
                    text: qsTr("Remove")
                    enabled: !entry.onProgram
                    onTriggered: bar.act({ action: "remove_layout", layout: entry.modelData.number })
                }
            }
        }
    }

    KanteButton {
        text: qsTr("+ Layout")
        enabled: bar.core.connected && bar.layouts.length < 4
        onClicked: bar.editRequested(null)
    }

    Item { Layout.fillWidth: true }
}
