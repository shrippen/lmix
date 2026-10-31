import QtQuick
import QtTest
import "../../qml"

/**
 * The Show page's cue editing (PLAN-SEITEN "Stand"): the bar adds one cue
 * of a kind with `add_cue` after the chosen cue and chooses it; the time
 * fields of a running cue show its progress; the inspector edits the file,
 * the stage, what a fade fades and a sound's crosspoints.
 */
TestCase {
    id: test
    name: "ShowCues"
    when: windowShown
    width: 1400
    height: 900

    property var sent: []

    QtObject {
        id: fakeCore
        signal answered(string action, string refusal)
        function choices(kind) {
            return kind === "colors" ? JSON.stringify(["red", "blue"]) : JSON.stringify({ min_db: -60, max_db: 10 })
        }
    }

    function cue(number, kind, fields) {
        return Object.assign({ id: "id-" + number, number: number, name: "", kind: kind, pre_wait_ms: 0, post_wait_ms: 0, follow: "none" }, fields || {})
    }

    function show(standby, running) {
        return {
            cue_lists: [{
                id: "list-1", number: 1, name: "Act", standby: standby, active: true,
                cues: [
                    cue("1", "sound", { sound: { file: "/fx/rain.flac", gain_db: 0, fade_in_ms: 0, fade_out_ms: 0 } }),
                    cue("2", "wait", { pre_wait_ms: 1000, post_wait_ms: 500, duration_ms: 2000 }),
                    cue("3", "fade", { duration_ms: 3000, fade: { target: { cue: "1" }, gain_db: -20, then: "stop" } })
                ]
            }],
            running_cues: running || [],
            ran_cues: [],
            cue_warnings: [],
            zones: [{ name: "Hall", channels: 2, outputs: [{ device: "", channels: [1, 2] }], gain_db: 0, mute: false },
                    { name: "Foyer", channels: 1, outputs: [{ device: "", channels: [3] }], gain_db: 0, mute: false }],
            stages: [{ name: "Main", width: 1920, height: 1080 }],
            inputs: [],
            outputs: [],
            panic_ms: 1000,
            go_guard_ms: 300
        }
    }

    Component {
        id: viewComponent
        ShowView {
            width: 1400
            height: 900
            act: action => test.sent.push(action)
            core: fakeCore
        }
    }

    function init() {
        sent = []
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

    function find(root, test) {
        return itemsOf(root).find(test)
    }

    function makeView(standby, running) {
        const view = createTemporaryObject(viewComponent, test.Window.contentItem, { show: show(standby, running) })
        waitForRendering(view)
        return view
    }

    function bar(view) {
        return find(view, i => i.newWaitMs !== undefined)
    }

    function button(root, text) {
        return find(root, i => i.text === text && i.clicked !== undefined && i.visible)
    }

    function test_the_bar_adds_after_the_chosen_cue() {
        const view = makeView(2)
        mouseClick(button(bar(view), "Wait"))
        compare(sent.length, 1)
        const action = sent[0]
        compare(action.action, "add_cue")
        compare(action.list, 1)
        compare(action.after, "id-2")
        compare(action.cue.kind, "wait")
        compare(action.cue.duration_ms, 5000)
        verify(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(action.cue.id), action.cue.id)
        // The inspector shows the new cue once the state brings it.
        compare(view.selectedId, action.cue.id)
    }

    function test_the_bar_adds_at_the_end_past_the_last_cue() {
        const view = makeView(4)
        mouseClick(button(bar(view), "Note"))
        compare(sent[0].action, "add_cue")
        compare(sent[0].after, undefined)
        compare(sent[0].cue.kind, "note")
        // Nothing chosen: no start cue.
        verify(!button(bar(view), "Start").enabled)
    }

    function test_a_new_fade_fades_out_the_chosen_sound() {
        const view = makeView(1)
        mouseClick(button(bar(view), "Fade"))
        const made = sent[0].cue
        compare(made.kind, "fade")
        compare(made.duration_ms, 5000)
        compare(made.fade, { target: { cue: "1" }, then: "stop", gain_db: -60 })
        sent = []
        // A wait chosen: the first zone to silence; a start starts it.
        view.selectedId = "id-2"
        mouseClick(button(bar(view), "Fade"))
        compare(JSON.stringify(sent[0].cue.fade), JSON.stringify({ target: { zone: "Hall" }, gain_db: -60 }))
        compare(sent[0].after, "id-2")
        // The new fade is chosen now; until the state brings it, standby.
        view.selectedId = "id-2"
        mouseClick(button(bar(view), "Start"))
        compare(sent[1].cue.target, { cue: "2" })
    }

    function test_the_times_of_a_running_cue_fill() {
        // Cue 2: pre wait 1 s, 2 s long, post wait 0.5 s; 1.5 s in.
        const view = makeView(3, [{ list: "list-1", cue: "id-2", number: "2", name: "Wait", phase: "running", elapsed_ms: 1500, total_ms: 3000, paused: true }])
        view.advanced = true
        waitForRendering(view)
        const times = itemsOf(view).filter(i => i.clock !== undefined && i.done !== undefined && i.visible)
        // Pre, duration and post of three cues.
        compare(times.length, 9)
        const of = id => times.filter(t => t.parent.parent.cue.id === id)
        const [pre, duration, post] = of("id-2")
        compare(pre.done, 1000)
        verify(!pre.runs, "a pre wait that is over shows its time")
        compare(duration.done, 500)
        verify(duration.runs)
        compare(duration.Accessible.name, "0:00.5")
        compare(post.done, 500)
        verify(!post.runs, "the post wait is over")
        // A cue that does not run shows only its times.
        compare(of("id-3")[1].done, -1)
        compare(of("id-3")[1].Accessible.name, "0:03.0")
    }

    function inspector(view) {
        return find(view, i => i.fadePatch !== undefined)
    }

    function test_a_new_fade_target_takes_its_value() {
        const view = makeView(3)
        const ins = inspector(view)
        const fade = { target: { cue: "1" }, gain_db: -20, then: "stop" }
        compare(ins.fadePatch(fade, "zone", "Hall", ""),
                { target: { cue: null, zone: "Hall", stage: null }, gain_db: -20, opacity: null, brightness: null, then: "keep" })
        compare(ins.fadePatch(fade, "stage", "Main", ""),
                { target: { cue: null, zone: null, stage: "Main" }, gain_db: null, opacity: null, brightness: 1, then: "keep" })
        compare(ins.fadePatch(fade, "cue", "4", "picture"),
                { target: { cue: "4", zone: null, stage: null }, gain_db: null, opacity: 1, brightness: null, then: "stop" })
        // Only sound and picture cues, not the fade itself.
        compare(ins.fadeChoices.cue.map(c => c.ref), ["1"])
    }

    function test_the_fade_target_goes_to_the_cue() {
        const view = makeView(3)
        const ins = inspector(view)
        const kind = find(ins, i => i.tooltips !== undefined && i.model !== undefined && i.model.length === 3 && i.model[1] === "Zone")
        kind.activated(1)
        compare(sent.length, 1)
        compare(sent[0].action, "update_cue")
        compare(sent[0].cue, "id-3")
        compare(sent[0].changes.fade.target, { cue: null, zone: "Hall", stage: null })
    }

    Component {
        id: routesComponent
        SoundRoutes {
            zones: [{ name: "Hall", channels: 2 }, { name: "Foyer", channels: 1 }]
            key: "id-1"
            onEdited: routes => test.sent.push(routes)
        }
    }

    function test_two_quick_crosspoint_changes_both_stay() {
        const routes = createTemporaryObject(routesComponent, test, { routes: [{ channel: 1, zone: "Hall", zone_channel: 1, gain_db: 0 }] })
        const rowsBefore = itemsOf(routes).filter(i => i.route !== undefined)
        // Two changes before the state comes back.
        routes.change(0, { gain_db: -6 })
        routes.add()
        compare(sent.length, 2)
        compare(sent[1], [{ channel: 1, zone: "Hall", zone_channel: 1, gain_db: -6 }, { channel: 2, zone: "Hall", zone_channel: 2, gain_db: 0 }])
        verify(itemsOf(routes).filter(i => i.route !== undefined)[0] === rowsBefore[0], "the first row was rebuilt")
        // The state brings it: no draft left.
        routes.routes = sent[1]
        compare(routes.draft, null)
        // A zone with one channel takes channel 1 at most.
        routes.change(1, { zone: "Foyer", zone_channel: Math.min(2, routes.zoneChannels("Foyer")) })
        compare(sent[2][1].zone_channel, 1)
    }

    function test_a_refused_change_drops_the_draft() {
        const routes = createTemporaryObject(routesComponent, test, { routes: [], core: fakeCore })
        routes.add()
        verify(routes.draft !== null)
        fakeCore.answered(JSON.stringify({ action: "update_cue" }), "bad")
        compare(routes.draft, null)
        compare(routes.shown, [])
    }

    Component {
        id: pathComponent
        PathField {
            width: 400
            path: "/fx/rain.flac"
            key: "id-1"
            onEdited: path => test.sent.push(path)
        }
    }

    function test_a_relative_path_stays_marked() {
        const path = createTemporaryObject(pathComponent, test)
        path.field.forceActiveFocus()
        path.field.selectAll()
        for (const c of "fx/x.wav") {
            keyClick(c)
        }
        keyClick(Qt.Key_Return)
        compare(path.field.text, "fx/x.wav")
        verify(path.field.invalid)
        verify(path.field.activeFocus)
        compare(sent.length, 0)
        path.field.selectAll()
        for (const c of "/fx/x.wav") {
            keyClick(c)
        }
        keyClick(Qt.Key_Return)
        compare(sent, ["/fx/x.wav"])
    }
}
