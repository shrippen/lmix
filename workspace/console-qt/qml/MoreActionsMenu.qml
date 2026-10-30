import QtQuick
import QtQuick.Controls as QQC2
import Kante

/**
 * "⋯ More input actions" (docs/DESIGN-DIALOGE.md M03 A): the actions
 * without room in the row, each a KanteMenuItem with its icon (rule 5),
 * then "In the row ▸", which ones the row shows.
 *   actions   the row's buttons by priority: `key`, `text`, `wanted`,
 *             `visible`, `enabled`, `clicked()`; optional `menuName` (the
 *             name in the menu, "Add input…" for "+ Input"), `iconName`
 *             and `barName` (only those can leave the row)
 *   menuOnly  keys of the actions kept in the menu
 * `menuOnlyChosen(key, on)` when "In the row" is ticked or unticked.
 */
QQC2.Menu {
    id: more

    property var actions: []
    property var menuOnly: []
    signal menuOnlyChosen(string key, bool on)

    KantePopupSkin { popup: more }
    // Also the row of the sub menu.
    delegate: KanteMenuItem { iconRoom: true }

    QQC2.MenuSeparator {}
    QQC2.Menu {
        id: barMenu
        objectName: "inTheRow"
        title: qsTr("In the row")
        KantePopupSkin { popup: barMenu }
    }

    // Made here, not by a Repeater (see ChoiceMenu).
    Component.onCompleted: {
        for (let i = 0; i < actions.length; i++) {
            more.insertItem(i, overflowItem.createObject(null, { button: actions[i] }))
            if (actions[i].barName !== undefined) {
                barMenu.addItem(barItem.createObject(null, { button: actions[i] }))
            }
        }
    }

    Component {
        id: overflowItem
        KanteMenuItem {
            property var button
            text: button.menuName !== undefined ? button.menuName : button.text
            icon.name: button.iconName !== undefined ? button.iconName : ""
            iconRoom: true
            visible: button.wanted && !button.visible
            height: visible ? implicitHeight : 0
            enabled: button.enabled
            onTriggered: button.clicked()
        }
    }
    // Checked: in the row while it fits; unchecked: always in the menu.
    Component {
        id: barItem
        KanteMenuItem {
            property var button
            text: button.barName
            checkable: true
            checked: more.menuOnly.indexOf(button.key) < 0
            onToggled: {
                more.menuOnlyChosen(button.key, !checked)
                checked = Qt.binding(() => more.menuOnly.indexOf(button.key) < 0)
            }
        }
    }
}
