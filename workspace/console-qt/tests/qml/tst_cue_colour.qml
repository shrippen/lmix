import QtQuick
import QtTest
import Kante
import "../../qml"

/**
 * A cue's colour as a swatch picker (CueColourButton,
 * docs/DESIGN-DIALOGE.md M12 A): the chosen colour with its ring, "None"
 * first, a pick reports the name and closes.
 */
TestCase {
    id: test
    name: "CueColour"
    when: windowShown
    width: 600
    height: 400

    property var picked: []

    Component {
        id: buttonComponent
        CueColourButton {
            tags: ["red", "blue"]
            current: "blue"
            onPicked: value => test.picked.push(value)
        }
    }

    function init() {
        picked = []
    }

    function test_the_chosen_colour_has_its_ring() {
        const button = createTemporaryObject(buttonComponent, test.Window.contentItem)
        compare(button.button.text, "Blue ▾")
        mouseClick(button.button)
        tryCompare(button.popover, "opened", true)
        compare(button.swatches.colors.length, 2)
        compare(button.swatches.noneText, "None")
        compare(button.swatches.currentIndex, 1)
        button.current = null
        compare(button.swatches.currentIndex, -1)
        compare(button.button.text, "None ▾")
    }

    function test_a_pick_reports_and_closes() {
        const button = createTemporaryObject(buttonComponent, test.Window.contentItem)
        mouseClick(button.button)
        tryCompare(button.popover, "opened", true)
        button.swatches.picked(0)
        compare(picked, ["red"])
        tryCompare(button.popover, "opened", false)
        // A click right after closing counts as the closing press (KantePopover).
        wait(Qt.styleHints.mouseDoubleClickInterval + 50)
        mouseClick(button.button)
        tryCompare(button.popover, "opened", true)
        button.swatches.picked(-1)
        compare(picked, ["red", null])
    }
}
