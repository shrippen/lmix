import QtQuick
import QtTest
import "../../qml"

/**
 * The zone bar keeps its rows over a state update: the first move of a
 * volume slider sends `set_zone_level`, the core answers with a new state,
 * and a rebuilt row would end the drag under the finger (review of #171).
 */
TestCase {
    id: test
    name: "ZoneBar"
    when: windowShown
    width: 800
    height: 400

    function zones(hallDb) {
        return [
            { name: "Hall", gain_db: hallDb, mute: false, outputs: ["a"] },
            { name: "Foyer", gain_db: -6, mute: false, outputs: ["b"] }
        ]
    }

    function itemsOf(root) {
        var out = []
        function walk(item) {
            out.push(item)
            for (var i = 0; i < item.children.length; i++) {
                walk(item.children[i])
            }
        }
        walk(root)
        return out
    }

    function slidersOf(root) {
        return itemsOf(root).filter(i => i.stepSize !== undefined && i.Accessible.name.startsWith("Volume of"))
    }

    QtObject {
        id: fakeCore
        function choices(kind) {
            return JSON.stringify({ min_db: -60, max_db: 10 })
        }
    }

    Component {
        id: barComponent
        ZoneBar {
            width: 800
            core: fakeCore
            act: function (action) {}
        }
    }

    function test_a_new_state_keeps_the_rows() {
        const bar = createTemporaryObject(barComponent, test, { zones: zones(0) })
        waitForRendering(bar)
        const before = slidersOf(bar)
        compare(before.length, 2)
        bar.zones = zones(-0.5)
        const after = slidersOf(bar)
        compare(after.length, 2)
        verify(after[0] === before[0], "the Hall slider was rebuilt")
        verify(after[1] === before[1], "the Foyer slider was rebuilt")
        compare(after[0].value, -0.5)
    }

    function test_rows_follow_the_zones_by_name() {
        const bar = createTemporaryObject(barComponent, test, { zones: zones(0) })
        waitForRendering(bar)
        const foyer = slidersOf(bar)[1]
        bar.zones = [zones(0)[1]]
        const after = slidersOf(bar)
        compare(after.length, 1)
        verify(after[0] === foyer, "the Foyer slider was rebuilt")
        compare(after[0].Accessible.name, "Volume of Foyer")
    }
}
