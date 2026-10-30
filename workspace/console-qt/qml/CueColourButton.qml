import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import Kante

/**
 * A cue's colour (docs/DESIGN-DIALOGE.md M12 A): a button with the
 * swatch and the name opens a swatch picker, the chosen colour with its
 * ring, "None" first, the name under it.
 *   tags     the colour names the core knows (`choices("colors")`)
 *   current  the cue's colour, null for none
 * `picked(value)` with a name or null; the popover closes.
 */
RowLayout {
    id: root

    property var tags: []
    property var current: null
    signal picked(var value)

    readonly property string currentName: current ? Texts.label("colors", current) : qsTr("None")
    /** The picker, for tests. */
    readonly property alias swatches: swatches
    readonly property alias popover: popover
    readonly property alias button: button

    spacing: KanteStyle.unit(6)

    KanteSwatch {
        visible: root.current !== null
        swatchColor: KanteStyle.labelColor(root.current)
        size: KanteSwatch.Size.Small
    }
    KanteButton {
        id: button
        focusPolicy: Qt.NoFocus
        text: root.currentName + " ▾"
        Accessible.name: qsTr("Colour: %1").arg(root.currentName)
        onClicked: popover.openAt(button)
    }

    KantePopover {
        id: popover
        KanteSwatchPicker {
            id: swatches
            focus: true
            colors: root.tags.map(c => KanteStyle.labelColor(c))
            names: root.tags.map(c => Texts.label("colors", c))
            noneText: qsTr("None")
            currentIndex: root.tags.indexOf(root.current)
            Accessible.name: qsTr("Cue colour")
            onPicked: index => {
                root.picked(index < 0 ? null : root.tags[index])
                popover.close()
            }
        }
        NameRow {}
    }

    // The chosen colour's name, at the right under the swatches.
    component NameRow: Item {
        width: swatches.width
        implicitHeight: name.implicitHeight
        QQC2.Label {
            id: name
            anchors.right: parent.right
            text: root.currentName
            font: KanteStyle.active ? KanteStyle.monoFont(KanteStyle.smallFont.pointSize) : KanteStyle.smallFont
            color: KanteStyle.mutedTextColor
        }
    }
}
