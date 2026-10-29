import QtQuick
import QtTest
import "../../qml"

/**
 * The tile size as pictures (TileSizePicker, docs/DESIGN-DIALOGE.md P01 A):
 * a click reports the size, the setting decides which one shows as chosen.
 */
TestCase {
    id: test
    name: "TileSizePicker"
    when: windowShown
    width: 400
    height: 200

    property var chosen: []

    Component {
        id: pickerComponent
        TileSizePicker {
            onActivated: index => test.chosen.push(index)
        }
    }

    function init() {
        chosen = []
    }

    function tiles(picker) {
        const found = []
        for (let i = 0; i < picker.children.length; i++) {
            if (picker.children[i].checked !== undefined) {
                found.push(picker.children[i])
            }
        }
        return found
    }

    function test_a_click_reports_the_setting_decides() {
        const picker = createTemporaryObject(pickerComponent, test.Window.contentItem, { currentIndex: 1 })
        const all = tiles(picker)
        compare(all.length, 3)
        compare(all.map(t => t.checked), [false, true, false])
        compare(all[2].Accessible.name, "Large tiles")
        mouseClick(all[2])
        compare(chosen, [2])
        // Not taken yet: M stays chosen.
        compare(all.map(t => t.checked), [false, true, false])
        picker.currentIndex = 2
        compare(all.map(t => t.checked), [false, false, true])
    }

    // The pictures differ in height; the tiles do not.
    function test_the_tiles_are_one_height() {
        const picker = createTemporaryObject(pickerComponent, test.Window.contentItem)
        const all = tiles(picker)
        compare(all[0].height, all[2].height)
        compare(all[1].height, all[2].height)
    }
}
