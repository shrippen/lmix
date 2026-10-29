import QtQuick
import QtTest
import "../../qml"
import "data/state.js" as Data

/**
 * A state update that changes one value must not rebuild the parts that
 * show the rest: every state of a fader drag rebuilt all input buttons
 * and froze the window (29.10.2026). Counts, not times, so it runs in CI.
 * The fader sends its moves at most every 33 ms, like the T-bar.
 */
TestCase {
    id: test
    name: "StateUpdates"
    when: windowShown
    width: 1200
    height: 900

    /** A state with input 1 at another volume: what a fader move sends back. */
    function withVolume(volume) {
        var s = JSON.parse(JSON.stringify(Data.state))
        s.inputs[0].audio.volume = volume
        return s
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

    /** How many items under `root` are not among `before`. */
    function madeSince(before, root) {
        var made = 0
        var after = itemsOf(root)
        for (var i = 0; i < after.length; i++) {
            if (before.indexOf(after[i]) < 0) {
                made++
            }
        }
        return made
    }

    QtObject {
        id: fakeCore
        function choices(kind) {
            return JSON.stringify({ buses: 8, name_max: 32 })
        }
        function shortName(name, max) {
            return name.slice(0, max)
        }
    }

    Component {
        id: busButtons
        BusButtons {
            width: 1000
            act: function (action) {}
        }
    }

    Component {
        id: narrowMixer
        NarrowMixer {
            width: 400
            height: 800
            core: fakeCore
            act: function (action) {}
        }
    }

    Component {
        id: choiceMenu
        ChoiceMenu {}
    }

    function test_input_buttons_survive_a_volume_change() {
        var s = Data.state
        var rows = createTemporaryObject(busButtons, test, { show: s, inputs: s.inputs })
        verify(rows)
        waitForRendering(rows)
        var before = itemsOf(rows)
        verify(before.length > 8, "the buttons are there")
        for (var v = 99; v > 89; v--) {
            var next = withVolume(v)
            rows.show = next
            rows.inputs = next.inputs
        }
        compare(madeSince(before, rows), 0, "items made by 10 volume changes")
    }

    function test_mixer_strips_survive_a_volume_change() {
        var mixer = createTemporaryObject(narrowMixer, test, { show: Data.state })
        verify(mixer)
        waitForRendering(mixer)
        var before = itemsOf(mixer)
        for (var v = 99; v > 89; v--) {
            mixer.show = withVolume(v)
        }
        compare(madeSince(before, mixer), 0, "items made by 10 volume changes")
    }

    /** The menu's choice items (its skin sits among them). */
    function choiceItems(menu) {
        var out = []
        for (var i = 0; i < menu.count; i++) {
            var item = menu.itemAt(i)
            if (item && item.choice !== undefined) {
                out.push(item)
            }
        }
        return out
    }

    function test_menu_items_survive_equal_choices() {
        var list = [{ value: "a", label: "A" }, { value: "b", label: "B" }]
        var menu = createTemporaryObject(choiceMenu, test, { choices: list })
        verify(menu)
        var before = choiceItems(menu)
        compare(before.length, 2)
        menu.choices = JSON.parse(JSON.stringify(list))
        var after = choiceItems(menu)
        verify(after[0] === before[0] && after[1] === before[1], "the same items after equal choices")
        menu.choices = [{ value: "c", label: "C" }]
        after = choiceItems(menu)
        compare(after.length, 1)
        compare(after[0].text, "C")
    }

    /** The vertical volume slider of the first input strip. */
    function firstFader(root) {
        var all = itemsOf(root)
        for (var i = 0; i < all.length; i++) {
            var item = all[i]
            if (item.orientation === Qt.Vertical && item.to === 100 && item.moved !== undefined) {
                return item
            }
        }
        return null
    }

    function test_a_fader_drag_sends_few_moves() {
        var sent = []
        var mixer = createTemporaryObject(narrowMixer, test, {
            show: Data.state,
            act: function (action) { sent.push(action) }
        })
        verify(mixer)
        waitForRendering(mixer)
        var fader = firstFader(mixer)
        verify(fader, "the fader of input 1")
        // 50 moves at once, as a quick drag delivers them.
        for (var v = 99; v >= 50; v--) {
            fader.value = v
            fader.moved()
        }
        verify(sent.length <= 2, "sent at once: " + sent.length)
        tryVerify(function () { return sent.length > 0 && sent[sent.length - 1].volume === 50 }, 1000,
            "the last position arrives")
        wait(100)
        verify(sent.length <= 3, "sent for 50 moves: " + sent.length)
        compare(sent[sent.length - 1].action, "set_volume")
        compare(sent[sent.length - 1].volume, 50)
    }
}
