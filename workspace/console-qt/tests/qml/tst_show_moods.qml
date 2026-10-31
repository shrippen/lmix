import QtQuick
import QtTest
import "../../qml"

/**
 * Moods in the Show page (MOOD.6): the side bar's Moods tab sets a mood
 * with a click and, in edit mode, adds a cue that sets it after the chosen
 * cue; a mood cue reads as its mood, and the inspector chooses the mood
 * and its fade.
 */
TestCase {
    id: test
    name: "ShowMoods"
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

    function show() {
        return {
            cue_lists: [{
                id: "list-1", number: 1, name: "Act", standby: 1, active: true,
                cues: [
                    cue("1", "note"),
                    cue("2", "mood", { mood: { mood: "v-night" } }),
                    cue("3", "mood", { mood: { mood: "gone", fade_ms: 2000 } })
                ]
            }],
            moods: {
                layers: [{ id: "l-light", name: "Light" }],
                groups: [
                    { id: "g-tavern", name: "Tavern", color: "orange", base: {},
                      variants: [{ id: "v-night", name: "Night", layers: {} }] },
                    { id: "g-forest", name: "Forest", base: {}, variants: [] }
                ],
                collections: []
            },
            mood: { mood: "v-night" },
            running_cues: [],
            ran_cues: [],
            cue_warnings: [],
            zones: [],
            stages: [],
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

    function makeView() {
        const view = createTemporaryObject(viewComponent, test.Window.contentItem, { show: show() })
        waitForRendering(view)
        return view
    }

    function moodsTab(view) {
        const side = findChild(view, "moods")
        for (let item = side; item; item = item.parent) {
            if (item.editLists !== undefined) {
                item.tab = 2
                break
            }
        }
        waitForRendering(view)
        return side
    }

    function test_moods_come_in_order_with_their_names() {
        const view = makeView()
        compare(view.moods.map(m => m.name), ["Tavern", "Tavern / Night", "Forest"])
        compare(view.cueName(view.list.cues[1]), "Tavern / Night")
        compare(view.cueName(view.list.cues[2]), "Mood (gone)")
        compare(view.kindLabel(view.list.cues[1]), "Set mood")
    }

    function test_a_click_sets_a_mood_and_plus_adds_a_cue() {
        const view = makeView()
        const list = moodsTab(view)
        const forest = findChild(list, "mood:Forest")
        verify(forest !== null)
        verify(findChild(list, "mood:Tavern / Night").selected, "the mood set now")
        forest.clicked()
        compare(sent, [{ action: "set_mood", mood: "g-forest" }])

        view.selectedId = "id-1"
        const add = findChild(forest, "addMood")
        verify(add.visible, "edit mode")
        add.clicked()
        compare(sent[1].action, "add_cue")
        compare(sent[1].after, "id-1")
        compare(sent[1].cue.kind, "mood")
        compare(sent[1].cue.mood, { mood: "g-forest" })

        view.editing = false
        verify(!add.visible, "show mode: only setting")
    }

    function test_the_inspector_chooses_the_mood_and_keeps_a_gone_one_marked() {
        const view = makeView()
        view.selectedId = "id-3"
        waitForRendering(view)
        const box = findChild(view, "moodBox")
        verify(box.visible)
        verify(box.missing, "a mood that is gone stays named")
        compare(box.currentIndex, 0)
        box.activated(3)
        compare(sent, [{ action: "update_cue", list: 1, cue: "id-3", changes: { mood: { mood: "g-forest" } } }])

        view.selectedId = "id-2"
        waitForRendering(view)
        verify(!box.missing)
        compare(box.currentIndex, 1)
    }
}
