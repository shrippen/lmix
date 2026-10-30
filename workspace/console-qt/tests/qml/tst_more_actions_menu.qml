import QtQuick
import QtTest
import "../../qml"

/**
 * "⋯ More input actions" (MoreActionsMenu, docs/DESIGN-DIALOGE.md M03 A):
 * KanteMenuItems with icons, "+ Input" as "Add input…", only what the row
 * has no room for, "In the row ▸" as a sub menu.
 */
TestCase {
    id: test
    name: "MoreActionsMenu"
    when: windowShown
    width: 800
    height: 600

    property var clicked: []
    property var chosen: []

    component Action: QtObject {
        property string key
        property string text
        property bool wanted: true
        property bool visible: false
        property bool enabled: true
        signal clicked()
        onClicked: test.clicked.push(key)
    }

    Action {
        id: add
        key: "add"
        text: "+ Input"
        readonly property string menuName: "Add input…"
        readonly property string iconName: "list-add"
        readonly property string barName: "+ Input"
    }
    Action {
        id: buses
        key: "buses"
        text: "Buses…"
        readonly property string iconName: "view-media-equalizer"
        readonly property string barName: "Buses…"
    }
    Action {
        id: solo
        key: "solo"
        text: "Clear solo"
        wanted: false
    }

    Component {
        id: menuComponent
        MoreActionsMenu {
            actions: [add, buses, solo]
            menuOnly: ["buses"]
            onMenuOnlyChosen: (key, on) => test.chosen.push(key + " " + on)
        }
    }

    function init() {
        clicked = []
        chosen = []
        add.visible = false
        buses.visible = false
    }

    function shown(menu) {
        const out = []
        for (let i = 0; i < menu.count; i++) {
            const item = menu.itemAt(i)
            if (item.text !== undefined && item.visible) {
                out.push(item.text)
            }
        }
        return out
    }
    function entry(menu, text) {
        for (let i = 0; i < menu.count; i++) {
            if (menu.itemAt(i).text === text) {
                return menu.itemAt(i)
            }
        }
        return null
    }

    function test_kante_items_with_icons() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        menu.open()
        tryCompare(menu, "opened", true)
        const item = entry(menu, "Add input…")
        verify(item, "+ Input is Add input… in the menu")
        verify(item.valueLabel !== undefined, "a KanteMenuItem")
        compare(item.icon.name, "list-add")
        compare(entry(menu, "Buses…").icon.name, "view-media-equalizer")
        item.triggered()
        compare(clicked, ["add"])
    }

    function test_only_what_the_row_has_no_room_for() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        menu.open()
        tryCompare(menu, "opened", true)
        // Clear solo is not wanted: not in the menu.
        compare(shown(menu), ["Add input…", "Buses…", "In the row"])
        add.visible = true
        compare(shown(menu), ["Buses…", "In the row"])
    }

    function test_in_the_row_is_a_sub_menu() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        const row = entry(menu, "In the row")
        verify(row && row.subMenu, "a sub menu")
        const sub = row.subMenu
        const plus = entry(sub, "+ Input")
        const bus = entry(sub, "Buses…")
        verify(entry(sub, "Clear solo") === null, "only actions with a bar name")
        compare(plus.checked, true)
        compare(bus.checked, false, "Buses… stays in the menu")
        bus.toggle()
        bus.toggled()
        compare(chosen, ["buses false"], "back into the row")
        // The setting decides, not the click.
        compare(bus.checked, false)
    }
}
