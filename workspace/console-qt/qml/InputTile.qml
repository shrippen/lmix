import QtQuick
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import Kante

/**
 * One input of the input bar: its live picture, number and name. Program
 * and preview are a bar in the tally colour plus the word, never colour
 * alone. Click: preview; double click: straight to program.
 */
KanteCard {
    id: tile

    required property var core
    required property var input
    property bool onProgram: false
    property bool onPreview: false

    barColor: onProgram ? KanteStyle.negativeTextColor
        : (onPreview ? KanteStyle.positiveTextColor : "transparent")
    implicitWidth: Kirigami.Units.gridUnit * 12
    implicitHeight: column.implicitHeight + Kirigami.Units.smallSpacing * 2

    ColumnLayout {
        id: column
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        anchors.topMargin: tile.barHeight + Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        LiveView {
            Layout.fillWidth: true
            Layout.preferredHeight: width * 9 / 16
            core: tile.core
            source: tile.input.id
        }

        RowLayout {
            Layout.fillWidth: true
            KanteSectionLabel {
                text: tile.input.number
                font.family: KanteStyle.monoFamily
            }
            KanteSectionLabel {
                Layout.fillWidth: true
                text: tile.input.name
                elide: Text.ElideRight
            }
            KanteSectionLabel {
                visible: tile.onProgram || tile.onPreview
                text: tile.onProgram ? qsTr("PGM") : qsTr("PRV")
                color: tile.barColor
            }
            KanteSectionLabel {
                visible: tile.input.status !== "online"
                text: tile.input.status === "offline" ? qsTr("offline") : qsTr("starting")
                color: tile.input.status === "offline" ? KanteStyle.negativeTextColor : KanteStyle.mutedTextColor
            }
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: tile.core.preview(tile.input.number)
        onDoubleClicked: tile.core.cutDirect(tile.input.number)
    }
}
