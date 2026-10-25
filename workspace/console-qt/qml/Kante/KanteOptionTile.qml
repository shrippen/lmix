import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import "."

/**
 * Option tile (Kante 1.43, from lmix): one of several kinds to choose from, as a tile with an
 * icon, a name and a hint, such as the source kinds in "Add input" (Camera, Video, Stream, …).
 * Use it where the choice needs to be seen at once; a combo box hides it.
 *   icon.name / icon.source  the icon (drawn in the text colour, focus colour when chosen)
 *   text      the name
 *   hint      one short line under it ("Capture, webcam")
 *   checked   chosen; tiles of one group are `autoExclusive` (default true)
 * Chosen: the selection tint and a 2 px frame in the focus colour; hover: tint step 1;
 * keyboard focus: a 1 px ring inside, also on the chosen tile (further in). Screen readers hear a radio button with name and hint.
 * Kante and Kante Light cut the top right corner; System: square, platform colours.
 */
QQC2.AbstractButton {
    id: tile

    property string hint: ""

    checkable: true
    autoExclusive: true
    hoverEnabled: true
    focusPolicy: Qt.StrongFocus
    padding: KanteStyle.unit(10)

    implicitWidth: Math.max(KanteStyle.unit(116), implicitContentWidth + leftPadding + rightPadding)
    implicitHeight: Math.max(KanteStyle.unit(84), implicitContentHeight + topPadding + bottomPadding)

    Accessible.role: Accessible.RadioButton
    Accessible.name: text
    Accessible.description: hint

    background: KanteCard {
        color: tile.checked ? KanteStyle.selectionColor
             : tile.hovered ? Qt.tint(KanteStyle.cardColor, KanteStyle.tint1Color)
             : KanteStyle.cardColor
        borderColor: tile.checked ? "transparent" : KanteStyle.frameColor
        chamfer: KanteStyle.chamferSmall

        Rectangle {
            visible: tile.checked
            anchors.fill: parent
            color: "transparent"
            border.color: KanteStyle.focusColor
            border.width: 2
        }
        // Keyboard focus, chosen or not: a 1 px ring inside (on a chosen tile further in,
        // apart from its 2 px frame, so the two read as a double ring).
        Rectangle {
            visible: tile.visualFocus
            anchors.fill: parent
            anchors.margins: tile.checked ? 4 : 2
            color: "transparent"
            border.color: KanteStyle.focusColor
            border.width: 1
        }
    }

    contentItem: ColumnLayout {
        spacing: KanteStyle.unit(4)

        Kirigami.Icon {
            Layout.preferredWidth: Kirigami.Units.iconSizes.smallMedium
            Layout.preferredHeight: Kirigami.Units.iconSizes.smallMedium
            source: tile.icon.name !== "" ? tile.icon.name : tile.icon.source
            color: tile.checked ? KanteStyle.focusColor : KanteStyle.textColor
            isMask: true
            visible: tile.icon.name !== "" || tile.icon.source.toString() !== ""
        }
        Item {
            Layout.fillHeight: true
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: tile.text
            font.weight: Font.DemiBold
            color: KanteStyle.strongTextColor
            elide: Text.ElideRight
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: tile.hint.length > 0
            text: tile.hint
            font: KanteStyle.smallFont
            color: KanteStyle.mutedTextColor
            elide: Text.ElideRight
        }
    }
}
