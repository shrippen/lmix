import QtQuick
import QtTest
import "../../qml"
import "fixtures/state.js" as Data

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
        property string devices: "[]"
        function choices(kind) {
            switch (kind) {
            case "colors": return JSON.stringify(["red", "green", "blue"])
            case "outputs": return JSON.stringify(["srt", "record"])
            case "containers": return JSON.stringify(["mkv"])
            case "card_modes": return JSON.stringify(["1080p25"])
            case "card_channels": return JSON.stringify(["2"])
            case "card_limits": return JSON.stringify({ sound_delay_ms: 1000 })
            }
            return JSON.stringify({ buses: 8, name_max: 32 })
        }
        function refreshLists() {}
        function showLive(item, source) {}
        function releaseLive(item) {}
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

    /** A fader move patches one value: everything else keeps its object. */
    function test_a_patch_keeps_unchanged_parts() {
        var s = StatePatch.apply(null, [{ p: [], v: JSON.parse(JSON.stringify(Data.state)) }])
        var next = StatePatch.apply(s, [{ p: ["inputs", 0, "audio", "volume"], v: 42 }])
        compare(next.inputs[0].audio.volume, 42)
        compare(s.inputs[0].audio.volume, 100, "the old state stays as it was")
        verify(next !== s && next.inputs !== s.inputs && next.inputs[0] !== s.inputs[0], "the path is copied")
        verify(next.inputs[1] === s.inputs[1], "the other inputs are the same objects")
        verify(next.overlays === s.overlays && next.buses === s.buses && next.format === s.format,
            "the other parts are the same objects")
        compare(JSON.stringify(next.inputs[0].strip), JSON.stringify(s.inputs[0].strip))
    }

    Component {
        id: showStateComponent
        ShowState {
            property int programChanges: 0
            property int inputsChanges: 0
            onProgramChanged: programChanges++
            onInputsChanged: inputsChanges++
        }
    }

    /** Bindings on `show.program` stay quiet while a fader moves. */
    function test_show_state_sets_only_touched_keys() {
        var show = createTemporaryObject(showStateComponent, test)
        show.patch([{ p: [], v: JSON.parse(JSON.stringify(Data.state)) }])
        compare(show.program, Data.state.program)
        compare(show.inputs.length, Data.state.inputs.length)
        var program = show.programChanges, inputs = show.inputsChanges, overlays = show.overlays
        for (var v = 99; v > 89; v--) {
            show.patch([{ p: ["inputs", 0, "audio", "volume"], v: v }])
        }
        compare(show.inputs[0].audio.volume, 90)
        compare(show.inputsChanges - inputs, 10)
        compare(show.programChanges, program, "program untouched")
        verify(show.overlays === overlays, "overlays untouched")
        show.patch([{ p: ["program"], v: null }])
        compare(show.program, null)
    }

    /** One input's cell changes with that input alone; the ids stay
     *  while no input comes, goes or moves. */
    function test_show_state_cells_follow_one_input() {
        var show = createTemporaryObject(showStateComponent, test)
        show.patch([{ p: [], v: JSON.parse(JSON.stringify(Data.state)) }])
        var first = Data.state.inputs[0].id, second = Data.state.inputs[1].id
        var ids = show.inputIds, cells = show.cells
        var secondChanges = 0
        show.cells[second].valueChanged.connect(() => secondChanges++)
        for (var v = 99; v > 89; v--) {
            show.patch([{ p: ["inputs", 0, "audio", "volume"], v: v }])
        }
        compare(show.inputById(first).audio.volume, 90)
        compare(secondChanges, 0, "the second input's cell stays quiet")
        verify(show.inputIds === ids && show.cells === cells, "ids and cells kept")
        var added = { id: "new", number: 9, name: "New" }
        show.patch([{ p: ["inputs"], a: show.inputs.map((i, n) => ({ k: n })).concat([{ v: added }]) }])
        compare(show.inputIds[show.inputIds.length - 1], "new")
        compare(show.inputById("new").name, "New")
        verify(show.cells[second] === cells[second], "an old cell stays")
        compare(show.inputById("gone"), undefined)
    }

    function test_a_patch_rebuilds_arrays_and_removes_keys() {
        var s = { inputs: [{ id: "a", n: 1 }, { id: "b", n: 2 }], gone: 1 }
        var next = StatePatch.apply(s, [
            { p: ["inputs"], a: [{ k: 1 }, { v: { id: "c", n: 3 } }, { k: 0 }] },
            { p: ["inputs", 2, "n"], v: 5 },
            { p: ["gone"], d: true }
        ])
        compare(JSON.stringify(next), JSON.stringify({ inputs: [{ id: "b", n: 2 }, { id: "c", n: 3 }, { id: "a", n: 5 }] }))
        verify(next.inputs[0] === s.inputs[1], "a kept element is the same object")
        compare(s.inputs[0].n, 1, "the old state stays as it was")
    }

    function test_by_id_finds_like_find() {
        var list = [{ id: "a", n: 1 }, { id: "b", n: 2 }, { id: "a", n: 3 }]
        compare(StatePatch.byId(list, "b").n, 2)
        compare(StatePatch.byId(list, "a").n, 1, "the first of two")
        compare(StatePatch.byId(list, "c"), undefined)
        compare(StatePatch.byId(null, "a"), undefined)
        var other = [{ id: "b", n: 5 }]
        compare(StatePatch.byId(other, "b").n, 5, "each list its own index")
        compare(StatePatch.byId(list, "b").n, 2)
    }

    /** What the tiles of `inputTile` reported, from their making on. */
    property var reported: []

    Component {
        id: inputTile
        InputTile {
            width: 300
            height: 200
            core: fakeCore
            act: function (action) {}
            onChromeSettled: value => test.reported.push(value)
        }
    }

    /** A tile reports its chrome once it is made and stops when it goes;
     *  the smaller heights on the way made the bar's last row come and go. */
    function test_a_tile_reports_only_its_settled_chrome() {
        reported = []
        var tile = inputTile.createObject(test, { input: Data.state.inputs[0] })
        verify(tile.made)
        waitForRendering(tile)
        var chrome = tile.chrome
        verify(reported.length > 0, "reported once made")
        for (var i = 0; i < reported.length; i++) {
            compare(reported[i], chrome, "report " + i)
        }
        var before = reported.length
        tile.destroy()
        wait(50)
        compare(reported.length, before, "nothing reported while it goes")
    }

    Component {
        id: addOutputDialog
        AddOutputDialog {
            core: fakeCore
        }
    }

    /** What an output carries stays chosen while a fader moves. */
    function test_add_output_keeps_its_source_through_a_volume_change() {
        var dialog = createTemporaryObject(addOutputDialog, test, { inputs: Data.state.inputs })
        dialog.open()
        tryVerify(() => dialog.opened)
        var box = itemsOf(dialog.contentItem).find(i => i.model !== undefined && i.count === 3 + Data.state.inputs.length)
        verify(box, "the source box")
        box.currentIndex = 4
        for (var v = 99; v > 89; v--) {
            dialog.inputs = withVolume(v).inputs
        }
        compare(box.currentIndex, 4)
        var renamed = withVolume(90)
        renamed.inputs[0].name = "Renamed"
        dialog.inputs = renamed.inputs
        compare(box.model[3], "1 Renamed", "a new name shows")
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
