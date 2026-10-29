import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Collapsed inputs (UI.3): a button each instead of a tile, red on
 * program and green in preview like a tile. Click and double click (GO)
 * as on the tile; right click: expand or cut.
 */
Flow {
    id: bar

    required property var core
    required property var show
    required property var act
    /** lmix `Input`s of the show. */
    required property var inputs
    /** Ids of the collapsed ones. */
    required property var collapsed
    signal expandRequested(string id)

    // Keyed: buttons (and an open menu) survive state updates.
    KeyedModel { id: keys }
    function sync() {
        keys.sync(inputs.filter(i => collapsed.indexOf(i.id) >= 0).map(i => i.id))
    }
    onInputsChanged: sync()
    onCollapsedChanged: sync()
    Component.onCompleted: sync()

    spacing: Kirigami.Units.smallSpacing
    visible: keys.count > 0

    KanteSectionLabel {
        height: Kirigami.Units.gridUnit * 2
        verticalAlignment: Text.AlignVCenter
        text: qsTr("Collapsed")
    }

    Repeater {
        model: keys

        Item {
            id: entry
            required property string key
            readonly property var modelData: StatePatch.byId(bar.inputs, key) || ({ id: key, number: 0, name: "" })
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
                // As on its tile (INP.8, INP.9).
                onClicked: bar.act({ action: "click_input", input: entry.modelData.id })
                onDoubleClicked: bar.act({ action: "go_input", input: entry.modelData.id })
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Click: %1. Double click (GO): %2. Right click: expand.")
                    .arg(Texts.label("presses", entry.modelData.click || "preview_input"))
                    .arg(Texts.label("presses", entry.modelData.go || "quick_play"))
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
