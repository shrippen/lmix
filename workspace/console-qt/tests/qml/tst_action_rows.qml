import QtQuick
import QtTest
import "../../qml"

/**
 * Action rows (shortcuts, triggers, cues, moods): up to sixteen, as
 * lmix-core takes; eight show, then one more each time the last shown is
 * used, so a list set over the API comes back whole.
 */
TestCase {
    id: test
    name: "ActionRows"
    when: windowShown
    width: 800
    height: 900

    Component {
        id: rowsComponent
        ActionRows {
            inputs: []
            outputs: []
        }
    }

    function test_sixteen_actions_come_back_whole() {
        const rows = createTemporaryObject(rowsComponent, test)
        compare(rows.shown, 8)
        const actions = []
        for (let i = 0; i < 12; i++) {
            actions.push({ action: "cut" })
        }
        rows.load(actions)
        compare(rows.filled, 12)
        compare(rows.shown, 13, "the used ones and one more")
        compare(rows.collect().length, 12)
        for (let i = 12; i < 16; i++) {
            actions.push({ action: "cut" })
        }
        rows.load(actions)
        compare(rows.shown, 16)
        compare(rows.collect().length, 16)
    }
}
