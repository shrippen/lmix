import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import Kante

/**
 * The tile size as three small pictures (docs/DESIGN-DIALOGE.md P01 A):
 * each shows how many tiles fit a row at that size, the letter under it.
 * `currentIndex` 0–2 (S, M, L); `activated(index)` on a choice.
 */
RowLayout {
    id: picker

    property int currentIndex: 1
    signal activated(int index)
    /** Width of each picture. */
    readonly property real across: Kirigami.Units.gridUnit * 3

    spacing: Kirigami.Units.smallSpacing

    Repeater {
        // Tiles per row and rows in the picture, per size.
        model: [
            { columns: 4, rows: 2, letter: qsTr("S", "small tiles"), name: qsTr("Small tiles") },
            { columns: 3, rows: 2, letter: qsTr("M", "medium tiles"), name: qsTr("Medium tiles") },
            { columns: 2, rows: 2, letter: qsTr("L", "large tiles"), name: qsTr("Large tiles") }
        ]
        delegate: KanteOptionTile {
            id: tile
            required property var modelData
            required property int index
            Layout.fillWidth: true
            implicitWidth: Kirigami.Units.gridUnit * 4
            implicitHeight: implicitContentHeight + topPadding + bottomPadding
            padding: Kirigami.Units.smallSpacing
            text: modelData.letter
            Accessible.name: modelData.name
            // The setting decides, not the click: the binding stays.
            checkable: false
            checked: picker.currentIndex === index
            onClicked: picker.activated(index)

            contentItem: ColumnLayout {
                spacing: Kirigami.Units.smallSpacing
                // As high as the largest picture: the tiles stay one height.
                Item {
                    Layout.alignment: Qt.AlignHCenter
                    implicitWidth: picker.across
                    implicitHeight: (picker.across / 2) * 9 / 16 * 2 + 2
                    Grid {
                        id: picture
                        anchors.centerIn: parent
                        columns: tile.modelData.columns
                        spacing: 2
                        Repeater {
                            model: tile.modelData.columns * tile.modelData.rows
                            Rectangle {
                                width: (picker.across - picture.spacing * (picture.columns - 1)) / picture.columns
                                height: width * 9 / 16
                                color: tile.checked ? KanteStyle.focusColor : KanteStyle.mutedTextColor
                                opacity: tile.checked ? 1 : 0.6
                            }
                        }
                    }
                }
                QQC2.Label {
                    Layout.alignment: Qt.AlignHCenter
                    text: tile.text
                    font: KanteStyle.smallFont
                    color: tile.checked ? KanteStyle.focusColor : KanteStyle.mutedTextColor
                }
            }
        }
    }
}
