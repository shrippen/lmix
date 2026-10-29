import QtQuick
import QtTest
import "../../qml"

/**
 * Effect and length on one level (EffectMenu, docs/DESIGN-DIALOGE.md M08 A):
 * a direction pair is one row with ◀ ▶, the lengths are chips, quick play
 * offers no stinger.
 */
TestCase {
    id: test
    name: "EffectMenu"
    when: windowShown
    width: 800
    height: 700

    property var picked: []

    readonly property var effects: [
        { value: "cut", label: "Cut" }, { value: "fade", label: "Fade" },
        { value: "slide_left", label: "Slide ◀" }, { value: "slide_right", label: "Slide ▶" },
        { value: "stinger", label: "Stinger" }
    ]

    Component {
        id: menuComponent
        EffectMenu {
            heading: "Button 1 · Effect"
            effects: test.effects
            effect: "slide_right"
            duration: 500
            durations: [250, 500, 1000]
            onEffectPicked: value => test.picked.push(value)
            onLengthPicked: ms => test.picked.push(ms)
            onStingerRequested: test.picked.push("stinger…")
        }
    }

    function init() {
        picked = []
    }

    function texts(menu) {
        const out = []
        for (let i = 0; i < menu.count; i++) {
            const item = menu.itemAt(i)
            if (item.text !== undefined && item.triggered !== undefined) {
                out.push(item.text)
            }
        }
        return out
    }
    function itemNamed(menu, text) {
        for (let i = 0; i < menu.count; i++) {
            if (menu.itemAt(i).text === text && menu.itemAt(i).triggered !== undefined) {
                return menu.itemAt(i)
            }
        }
        return null
    }

    function lengths(menu) {
        for (let i = 0; i < menu.count; i++) {
            if (menu.itemAt(i).objectName === "lengths") {
                return menu.itemAt(i)
            }
        }
        return null
    }

    function test_one_row_per_effect_and_the_lengths_last() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        compare(texts(menu), ["Cut", "Fade", "Slide", "Stinger…", "Length"])
        const slide = itemNamed(menu, "Slide")
        compare(slide.checked, true)
        compare(slide.current, 1)
        menu.stinger = false
        compare(texts(menu), ["Cut", "Fade", "Slide", "Length"])
    }

    function test_choices_report() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        menu.open()
        tryCompare(menu, "opened", true)
        itemNamed(menu, "Fade").triggered()
        const slide = itemNamed(menu, "Slide")
        // The row keeps its direction; Left picks the other.
        slide.triggered()
        slide.pick(0)
        lengths(menu).step(1)
        compare(picked, ["fade", "slide_right", "slide_left", 1000])
    }

    function test_a_segment_picks_its_length() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        menu.open()
        tryCompare(menu, "opened", true)
        const length = lengths(menu)
        const segmented = length.contentItem.children.find(c => c.select !== undefined)
        compare(segmented.currentIndex, 1)
        segmented.select(0)
        compare(picked, [250])
        // Not taken yet: 500 stays chosen until the state says otherwise.
        compare(segmented.currentIndex, 1)
        menu.duration = 250
        compare(segmented.currentIndex, 0)
    }

    // The rows went in under the items declared so far: the heading stood
    // under the effects.
    function test_the_heading_comes_first() {
        const menu = createTemporaryObject(menuComponent, test.Window.contentItem)
        const shown = []
        for (let i = 0; i < menu.count; i++) {
            if (menu.itemAt(i).text !== undefined) {
                shown.push(menu.itemAt(i).text)
            }
        }
        compare(shown, ["Button 1 · Effect", "Cut", "Fade", "Slide", "Stinger…", "Length", "Length"])
    }
}
