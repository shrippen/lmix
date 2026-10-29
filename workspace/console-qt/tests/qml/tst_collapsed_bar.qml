import QtQuick
import QtTest
import "../../qml"

/**
 * A collapsed input (CollapsedBar, docs/DESIGN-DIALOGE.md M04): a click acts
 * as on its tile, a double click only expands. A click that goes on air
 * waits for the double click, so expanding never cuts the input to program.
 */
TestCase {
    id: test
    name: "CollapsedBar"
    when: windowShown
    width: 800
    height: 200

    property var sent: []
    property var expanded: []

    QtObject {
        id: fakeCore
        property bool connected: true
    }

    Component {
        id: barComponent
        CollapsedBar {
            width: 600
            core: fakeCore
            show: ({ program: "", preview: "" })
            act: action => test.sent.push(action)
            inputs: [
                { id: "cam", number: 1, name: "Cam", click: "preview_input" },
                { id: "clip", number: 2, name: "Clip", click: "cut_direct" }
            ]
            collapsed: ["cam", "clip"]
            onExpandRequested: id => test.expanded.push(id)
        }
    }

    function init() {
        sent = []
        expanded = []
    }

    function buttonNamed(item, name) {
        for (let i = 0; i < item.children.length; i++) {
            const child = item.children[i]
            if (child.Accessible.name === name && child.clicked !== undefined) {
                return child
            }
            const found = buttonNamed(child, name)
            if (found) {
                return found
            }
        }
        return null
    }

    // In the window: the TestCase itself is not shown, clicks would miss.
    function makeBar() {
        const bar = createTemporaryObject(barComponent, test.Window.contentItem)
        waitForRendering(bar)
        return bar
    }

    function test_a_preview_click_acts_at_once() {
        const bar = makeBar()
        mouseClick(buttonNamed(bar, "Input 1 Cam, collapsed"))
        compare(sent, [{ action: "click_input", input: "cam" }])
    }

    function test_a_double_click_only_expands() {
        const bar = makeBar()
        mouseDoubleClickSequence(buttonNamed(bar, "Input 2 Clip, collapsed"))
        compare(expanded, ["clip"])
        wait(Qt.styleHints.mouseDoubleClickInterval + 100)
        compare(sent.length, 0)
    }

    function test_a_cut_click_comes_after_the_wait() {
        const bar = makeBar()
        mouseClick(buttonNamed(bar, "Input 2 Clip, collapsed"))
        compare(sent.length, 0)
        tryCompare(test, "sent", [{ action: "click_input", input: "clip" }], Qt.styleHints.mouseDoubleClickInterval + 1000)
    }
}
