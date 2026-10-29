import QtQuick
import QtQuick.Controls as QQC2
import Kante

/**
 * Menu of choices `[{value, label}]`, the current one checked. Items are
 * made here rather than by a Repeater: Breeze's MenuItem calls RegExp(),
 * which a Repeater's model data object shadows.
 */
QQC2.Menu {
    id: menu

    property var choices: []
    property var current: undefined
    signal picked(var value)

    KantePopupSkin { popup: menu }

    property var made: []
    /** The choices `made` shows: a state update hands over equal ones as a new list. */
    property string madeFrom: ""

    // Keys sorted and lists as lists: set at creation, the same choices
    // arrive as a Qt list with sorted keys.
    function canonical(value) {
        if (value !== null && typeof value === "object" && typeof value.length === "number") {
            return "[" + Array.from(value).map(canonical).join(",") + "]"
        }
        if (value !== null && typeof value === "object") {
            return "{" + Object.keys(value).sort().map(k => JSON.stringify(k) + ":" + canonical(value[k])).join(",") + "}"
        }
        return JSON.stringify(value)
    }

    function rebuild() {
        var from = canonical(choices)
        if (from === madeFrom) {
            return
        }
        madeFrom = from
        for (var i = 0; i < made.length; i++) {
            menu.removeItem(made[i])
            made[i].destroy()
        }
        var list = []
        for (var j = 0; j < choices.length; j++) {
            var item = itemComponent.createObject(null, { choice: choices[j] })
            menu.addItem(item)
            list.push(item)
        }
        made = list
    }

    onChoicesChanged: rebuild()
    Component.onCompleted: rebuild()

    Component {
        id: itemComponent
        QQC2.MenuItem {
            property var choice
            text: choice.label
            checkable: menu !== null && menu.current !== undefined
            checked: menu !== null && menu.current !== undefined && choice.value === menu.current
            onTriggered: menu.picked(choice.value)
        }
    }
}
