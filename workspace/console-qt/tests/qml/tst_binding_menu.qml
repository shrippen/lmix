import QtQuick
import QtTest
import "../../qml"

/**
 * Binding of a title field (BindingMenu, docs/DESIGN-DIALOGE.md M11 A):
 * a section per source, the columns named once with the current value,
 * the bound one ticked; a new row changes the examples, not the items.
 */
TestCase {
    id: test
    name: "BindingMenu"
    when: windowShown
    width: 600
    height: 600

    property var picked: []

    readonly property var team: ({ id: "team", name: "team.csv", columns: ["Name", "Role"], current: ["Jonas Brandt", "Camera"] })
    readonly property var weather: ({ id: "sea", name: "seewetter", columns: ["Title"], current: [] })

    Component {
        id: menuComponent
        BindingMenu {
            sources: [test.team, test.weather]
            bound: ({ source: "team", column: "Role" })
            onPicked: binding => test.picked.push(binding)
        }
    }

    function init() {
        picked = []
    }

    /** The menu's rows, without the skin. */
    function entries(menu) {
        const out = []
        for (let i = 0; i < menu.count; i++) {
            if (menu.itemAt(i).text !== undefined) {
                out.push(menu.itemAt(i))
            }
        }
        return out
    }
    function rows(menu) {
        const out = []
        for (const item of entries(menu)) {
            out.push(item.valueText !== undefined
                ? item.text + (item.valueText ? "=" + item.valueText : "") + (item.checked ? " ✓" : "")
                : "§" + item.text)
        }
        return out
    }

    function test_a_section_per_source_with_examples() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        compare(rows(menu), ["Not bound", "§team.csv", "Name=Jonas Brandt", "Role=Camera ✓", "§seewetter", "Title"])
        menu.bound = null
        compare(rows(menu), ["Not bound ✓", "§team.csv", "Name=Jonas Brandt", "Role=Camera", "§seewetter", "Title"])
    }

    function test_a_pick_reports_the_binding() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        entries(menu)[2].triggered()
        entries(menu)[0].triggered()
        compare(picked, [{ source: "team", column: "Name" }, null])
    }

    // Rows move on (auto advance): the examples follow, the items stay.
    function test_a_new_row_keeps_the_items() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        const name = entries(menu)[2]
        menu.sources = [Object.assign({}, team, { current: ["Mia Roth", "Sound"] }), weather]
        verify(entries(menu)[2] === name, "the same item")
        compare(name.valueText, "Mia Roth")
        menu.sources = [Object.assign({}, team, { columns: ["Name", "Role", "Photo"] }), weather]
        compare(rows(menu).length, 7)
    }
}
