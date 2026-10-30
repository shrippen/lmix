import QtQuick
import QtTest
import "../../qml"

/**
 * A collapsed input (CollapsedBar, docs/DESIGN-DIALOGE.md M04): a click acts
 * as on its tile, at once. A double click expands an input whose click only
 * previews; one whose click goes on air expands from its menu only.
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
                { id: "clip", number: 2, name: "Clip", click: "cut_direct" },
                { id: "jingle", number: 3, name: "Jingle", click: "quick_play" }
            ]
            collapsed: ["cam", "clip", "jingle"]
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

    // A cut waited for a possible double click, 400-500 ms late on air.
    function test_a_cut_click_acts_at_once() {
        const bar = makeBar()
        mouseClick(buttonNamed(bar, "Input 2 Clip, collapsed"))
        compare(sent, [{ action: "click_input", input: "clip" }])
    }

    // Its click goes on air: a double click is not to expand it (the menu
    // does), and cuts once, not twice.
    // Quick play goes on air too.
    function test_a_double_click_on_a_cut_input_cuts_once() {
        const bar = makeBar()
        mouseDoubleClickSequence(buttonNamed(bar, "Input 2 Clip, collapsed"))
        mouseDoubleClickSequence(buttonNamed(bar, "Input 3 Jingle, collapsed"))
        compare(expanded, [])
        wait(Qt.styleHints.mouseDoubleClickInterval + 100)
        compare(sent, [{ action: "click_input", input: "clip" }, { action: "click_input", input: "jingle" }])
    }

    function test_a_double_click_expands_a_preview_input() {
        const bar = makeBar()
        mouseDoubleClickSequence(buttonNamed(bar, "Input 1 Cam, collapsed"))
        compare(expanded, ["cam"])
    }
}
