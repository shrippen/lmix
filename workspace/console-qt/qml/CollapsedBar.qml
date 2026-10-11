import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Collapsed inputs (UI.3): a button each instead of a tile, red on
 * program and green in preview like a tile. Click: into preview; double
 * click: cut; right click: expand or cut.
 */
Flow {
    id: bar

    required property var core
    required property var show
    required property var act
    /** lmix `Input`s that are collapsed, in input order. */
    required property var inputs
    signal expandRequested(string id)

    spacing: Kirigami.Units.smallSpacing
    visible: inputs.length > 0

    KanteSectionLabel {
        height: Kirigami.Units.gridUnit * 2
        verticalAlignment: Text.AlignVCenter
        text: qsTr("Collapsed")
    }

    Repeater {
        model: bar.inputs

        Item {
            id: entry
            required property var modelData
            readonly property bool onProgram: bar.show !== null && bar.show.program === modelData.id
            readonly property bool onPreview: bar.show !== null && bar.show.preview === modelData.id

            implicitWidth: button.implicitWidth
            implicitHeight: button.implicitHeight
            width: implicitWidth
            height: implicitHeight

            KanteButton {
                id: button
                anchors.fill: parent
                text: entry.modelData.number + " " + entry.modelData.name
                emphasis: entry.onProgram ? KanteButton.Emphasis.Destructive
                    : entry.onPreview ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
                enabled: bar.core.connected
                Accessible.name: qsTr("Input %1 %2, collapsed").arg(entry.modelData.number).arg(entry.modelData.name)
                onClicked: bar.act({ action: "preview_input", input: entry.modelData.id })
                onDoubleClicked: bar.act({ action: "cut_direct", input: entry.modelData.id })
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Click: into preview. Double click: cut. Right click: expand.")
            }

            TapHandler {
                acceptedButtons: Qt.RightButton
                enabled: bar.core.connected
                onTapped: menu.popup()
            }

            QQC2.Menu {
                id: menu
                KantePopupSkin { popup: menu }
                QQC2.MenuItem {
                    text: qsTr("Expand")
                    onTriggered: bar.expandRequested(entry.modelData.id)
                }
                QQC2.MenuItem {
                    text: qsTr("Cut to program")
                    enabled: !entry.onProgram
                    onTriggered: bar.act({ action: "cut_direct", input: entry.modelData.id })
                }
            }
        }
    }
}
