import QtQuick
import "."

/**
 * Kante look for a sheet or drawer (a QQC2.Drawer or Popup that comes from an edge):
 * a cut-corner card with the yellow bar and the Kante colours for its content.
 * `drawer: true` for one over a dialog's content: raised, cut on the side facing it.
 * Give it `popup: <id>`; a QtObject, so it is no content of the popup.
 */
QtObject {
    id: skin

    required property var popup
    /** Bar colour; negative for a destructive question. */
    property color barColor: KanteStyle.accentColor
    /** A drawer over a dialog's content: one surface step raised, cut top left (`.sheet.is-drawer`). */
    property bool drawer: false

    readonly property KanteScope scope: KanteScope { target: skin.popup ? skin.popup.contentItem : null }
    readonly property Item kanteBackground: KanteCard {
        color: skin.drawer ? Qt.tint(KanteStyle.dialogColor, KanteStyle.cardColor) : KanteStyle.dialogColor
        barColor: skin.barColor
        mirrored: skin.drawer
    }
    readonly property Binding backgroundBinding: Binding {
        target: skin.popup
        property: "background"
        value: skin.kanteBackground
        when: KanteStyle.themed && skin.popup !== null
        restoreMode: Binding.RestoreBindingOrValue
    }
}
