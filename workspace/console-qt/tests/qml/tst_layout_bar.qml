import QtQuick
import QtTest
import "../../qml"

/**
 * A layout's right-click menu swaps two of its boxes (LAY.7): one entry
 * per pair, each box named by its input, sending `swap_boxes`.
 */
TestCase {
    id: test
    name: "LayoutBar"
    when: windowShown
    width: 800
    height: 200

    property var sent: []

    QtObject {
        id: fakeCore
        property bool connected: true
    }

    readonly property var fakeShow: ({
        program: "l1",
        preview: null,
        inputs: [
            { id: "a", name: "Wide" },
            { id: "b", name: "Host" },
            { id: "c", name: "Guest" }
        ],
        layouts: [
            { id: "l1", number: 1, name: "Interview", background: "a",
              boxes: [{ input: "b" }, { input: "c" }] },
            { id: "l2", number: 2, name: "Single", background: "a",
              boxes: [{ input: "b" }] }
        ],
        layout_templates: []
    })

    Component {
        id: barComponent
        LayoutBar {
            core: fakeCore
            show: test.fakeShow
            act: action => test.sent.push(action)
        }
    }

    function init() {
        sent = []
    }

    /** The "Swap boxes" submenu of layout `number`. */
    function swapMenuOf(bar, number) {
        function walk(item) {
            if (item.modelData !== undefined && item.modelData.number === number) {
                return item
            }
            for (var i = 0; i < item.children.length; i++) {
                var found = walk(item.children[i])
                if (found) {
                    return found
                }
            }
            return null
        }
        const entry = walk(bar)
        verify(entry !== null, "no button for layout " + number)
        for (var i = 0; i < entry.data.length; i++) {
            const menu = entry.data[i]
            if (menu.menuAt === undefined) {
                continue
            }
            for (var j = 0; j < menu.count; j++) {
                const sub = menu.menuAt(j)
                if (sub && sub.title === "Swap boxes") {
                    return sub
                }
            }
        }
        return null
    }

    function test_pairs_name_their_inputs() {
        const bar = createTemporaryObject(barComponent, test)
        const choices = bar.swapChoices(fakeShow.layouts[0])
        compare(choices.length, 1)
        compare(choices[0].label, "Box 1 (Host) and box 2 (Guest)")
        compare(bar.swapChoices(fakeShow.layouts[1]).length, 0)
        const four = { boxes: [{ input: "a" }, { input: "b" }, { input: "c" }, { input: "x" }] }
        const pairs = bar.swapChoices(four)
        compare(pairs.length, 6)
        compare(pairs[5].label, "Box 3 (Guest) and box 4 (?)")
    }

    function test_a_hidden_box_says_so() {
        const bar = createTemporaryObject(barComponent, test)
        const layout = { boxes: [{ input: "b" }, { input: "c", hidden: true }] }
        compare(bar.swapChoices(layout)[0].label, "Box 1 (Host) and box 2 (Guest, hidden)")
    }

    function test_a_pick_swaps_also_on_program() {
        const bar = createTemporaryObject(barComponent, test)
        waitForRendering(bar)
        const menu = swapMenuOf(bar, 1)
        verify(menu !== null, "no Swap boxes menu")
        verify(menu.enabled, "the layout on program can swap")
        menu.picked([1, 2])
        compare(sent, [{ action: "swap_boxes", layout: 1, a: 1, b: 2 }])
        verify(!swapMenuOf(bar, 2).enabled, "one box has nothing to swap")
    }
}
