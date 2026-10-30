import QtQuick
import QtTest
import "../../qml"

/**
 * One menu for all tiles and collapsed inputs (InputMenu,
 * docs/DESIGN-DIALOGE.md M04): it acts on the input it was opened for, and
 * a collapsed one gets Expand instead of Collapse.
 */
TestCase {
    id: test
    name: "InputMenu"
    when: windowShown
    width: 800
    height: 600

    property var sent: []
    property var requests: []

    QtObject {
        id: fakeCore
        function choices(name) { return JSON.stringify(["red", "blue"]) }
    }

    Component {
        id: menuComponent
        InputMenu {
            core: fakeCore
            act: action => test.sent.push(action)
            inputs: [
                { id: "cam", number: 1, name: "Camera", source: { kind: "pattern" }, audio: { on: true, follow_video: false, volume: 100 }, has_audio: false, listen: "off", signal_loss: "hold", color: null, go: "quick_play" },
                { id: "clip", number: 2, name: "Clip", source: { kind: "file" }, audio: { on: true, follow_video: false, volume: 100 }, has_audio: true, listen: "off", signal_loss: "hold", color: null, go: "quick_play" }
            ]
            show: ({ program: "cam", preview: "", looks_available: false, dynamic_inputs: [] })
            onCollapseRequested: test.requests.push("collapse " + inputId)
            onExpandRequested: test.requests.push("expand " + inputId)
            onRenameRequested: test.requests.push("rename " + inputId)
        }
    }

    function init() {
        sent = []
        requests = []
    }

    function entry(menu, text) {
        for (var i = 0; i < menu.count; i++) {
            var item = menu.itemAt(i)
            if (item && item.text === text) {
                return item
            }
        }
        return null
    }

    function test_a_collapsed_input_gets_expand() {
        var menu = createTemporaryObject(menuComponent, test)
        menu.openFor("clip", true)
        tryCompare(menu, "opened", true)
        verify(entry(menu, "Collapse") === null)
        entry(menu, "Expand").triggered()
        compare(requests, ["expand clip"])

        menu.openFor("cam", false)
        tryCompare(menu, "opened", true)
        verify(entry(menu, "Expand") === null)
        entry(menu, "Collapse").triggered()
        compare(requests, ["expand clip", "collapse cam"])
    }

    function test_it_acts_on_the_input_it_was_opened_for() {
        var menu = createTemporaryObject(menuComponent, test)
        menu.openFor("cam", false)
        menu.close()
        menu.openFor("clip", false)
        tryCompare(menu, "opened", true)
        compare(menu.hasAudio, true)
        compare(menu.onProgram, false)
        entry(menu, "GO: " + Texts.label("presses", "quick_play")).triggered()
        compare(sent.length, 1)
        compare(sent[0].action, "go_input")
        compare(sent[0].input, 2)
    }
}
