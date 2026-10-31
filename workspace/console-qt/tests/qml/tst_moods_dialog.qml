import QtQuick
import QtTest
import "../../qml"

/**
 * Editing the moods (MOOD.12, MOOD.9): every edit sends the whole moods
 * (`set_moods`) with only that change; a variant's layer can take the
 * base's again; a new sound or picture waits for its file, keeping what
 * is chosen meanwhile; a refused name does not stay; quick edits build
 * on each other until the state has them; a removal asks (Removals) and
 * leaves nothing behind; the panic mood holds actions only.
 */
TestCase {
    id: test
    name: "MoodsDialog"
    when: windowShown
    width: 1400
    height: 900

    readonly property var moods: ({
        layers: [{ id: "l-light", name: "Light" }, { id: "l-music", name: "Music", fade_ms: 5000 }],
        groups: [
            {
                id: "g-tavern", name: "Tavern", color: "orange",
                base: {
                    "l-light": { kind: "actions", actions: [{ action: "http_request", target: "Lights", path: "/warm" }] },
                    "l-music": { kind: "sound", sound: { file: "/sounds/lute.wav", repeat: "forever" } }
                },
                variants: [{ id: "v-night", name: "Night", layers: { "l-light": { kind: "off" } } }]
            },
            { id: "g-forest", name: "Forest", base: {}, variants: [] }
        ],
        collections: [{ id: "c-act1", name: "Act 1", moods: ["v-night", "g-forest"], pads: [] }],
        panic: { "l-light": { kind: "actions", actions: [{ action: "http_request", target: "Lights", path: "/house" }] } },
        change_ms: 3000,
        inside_ms: 1000
    })

    property var show: null
    property var acted: []
    /** The removal asked (Removals.askWith), or null. */
    property var asked: null
    function askRemove(request) { asked = request }
    /** Accepts the removal asked. */
    function accept() { act(asked.makeAction()) }
    /** Like lmix-core answering later: the state does not change yet. */
    property bool slow: false
    /** Like lmix-core refusing every edit. */
    property bool refuse: false
    /** Like lmix-core: what it takes becomes the show's. */
    function act(action) {
        acted.push(action)
        if (action.action === "set_moods" && !refuse && !slow) {
            show = Object.assign({}, show, { moods: action.moods })
        }
    }

    Component {
        id: dialogComponent
        MoodsDialog {
            act: test.act
            askRemove: test.askRemove
            show: test.show
            core: null
        }
    }

    function init() {
        acted = []
        refuse = false
        slow = false
        asked = null
        show = { moods: JSON.parse(JSON.stringify(test.moods)), stages: [{ name: "Back" }], inputs: [], outputs: [] }
    }

    QtObject {
        id: fakeCore
        signal answered(string action, string refusal)
        function choices(what) { return "[]" }
    }
    Component {
        id: answeredComponent
        MoodsDialog {
            act: test.act
            askRemove: test.askRemove
            show: test.show
            core: fakeCore
        }
    }

    function dialog() {
        const d = createTemporaryObject(dialogComponent, test)
        d.open()
        return d
    }

    function sent() {
        compare(acted.length, 1, "one set_moods")
        compare(acted[0].action, "set_moods")
        return acted[0].moods
    }

    function test_a_rename_changes_only_the_name() {
        const d = dialog()
        d.chosenId = "g-tavern"
        const field = findChild(d.contentItem, "moodName")
        field.text = "Inn"
        field.editingFinished()
        const want = JSON.parse(JSON.stringify(test.moods))
        want.groups[0].name = "Inn"
        compare(sent(), want)
    }

    function test_a_variant_takes_the_base_again() {
        const d = dialog()
        d.chosenId = "v-night"
        const kind = findChild(d.contentItem, "kind:Light")
        compare(kind.currentText, "Off")
        kind.activated(0) // From the base
        compare(sent().groups[0].variants[0].layers, {})
        compare(kind.currentText, "From the base")
    }

    function test_a_new_sound_waits_for_its_file() {
        const d = dialog()
        d.chosenId = "g-forest"
        const kind = findChild(d.contentItem, "kind:Music")
        kind.activated(kind.model.indexOf("Sound"))
        compare(acted.length, 0, "nothing goes out without a file")
        compare(kind.currentText, "Sound")
        findChild(d.contentItem, "file:Music").edited("/sounds/birds.flac")
        compare(sent().groups[1].base, { "l-music": { kind: "sound", sound: { file: "/sounds/birds.flac", repeat: "forever" } } })
        compare(d.pending, {})
    }

    function test_a_picture_needs_a_stage() {
        show = Object.assign({}, show, { stages: [] })
        const d = dialog()
        d.chosenId = "g-forest"
        verify(findChild(d.contentItem, "kind:Light").model.indexOf("Picture") < 0)
    }

    function test_a_removed_group_leaves_the_collections() {
        const d = dialog()
        d.chosenId = "g-tavern"
        findChild(d.contentItem, "removePlace").clicked()
        compare(acted.length, 0, "it asks first")
        // As lmix-core names the removal, for the toast's Undo.
        compare(asked.kind, "mood")
        compare(asked.name, "Tavern")
        compare(asked.question, "Remove group \u201cTavern\u201d?")
        accept()
        const moods = sent()
        compare(moods.groups.map(g => g.id), ["g-forest"])
        compare(moods.collections[0].moods, ["g-forest"])
        compare(d.chosen, null)
    }

    function test_a_new_group_and_variant_get_free_names() {
        const d = dialog()
        findChild(d.contentItem, "addGroup").clicked()
        findChild(d.contentItem, "addGroup").clicked()
        compare(show.moods.groups.map(g => g.name), ["Tavern", "Forest", "Group", "Group 2"])
        findChild(d.contentItem, "addVariant").clicked()
        const group = show.moods.groups[3]
        compare(group.variants.map(v => v.name), ["Variant"])
        compare(d.chosenId, group.variants[0].id)
        verify(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(group.id))
    }

    function test_a_removed_layer_leaves_the_moods_and_the_panic() {
        const d = dialog()
        d.tab = 1
        findChild(d.contentItem, "removeLayer:0").clicked()
        compare(asked.name, "Light")
        accept()
        const moods = sent()
        compare(moods.layers.map(l => l.id), ["l-music"])
        compare(Object.keys(moods.groups[0].base), ["l-music"])
        compare(moods.groups[0].variants[0].layers, {})
        compare(moods.panic, {})
    }

    function test_the_panic_mood_holds_actions_or_nothing() {
        const d = dialog()
        d.tab = 3
        const music = findChild(d.contentItem, "panic:Music")
        compare(music.model, ["Off", "Actions"])
        const light = findChild(d.contentItem, "panic:Light")
        compare(light.currentIndex, 1)
        light.activated(0)
        compare(sent().panic, {})
        compare(light.currentIndex, 0)
    }

    function test_collections_are_added_and_renamed() {
        const d = dialog()
        d.tab = 2
        findChild(d.contentItem, "addCollection").clicked()
        compare(show.moods.collections.map(c => c.name), ["Act 1", "Collection"])
        const field = findChild(d.contentItem, "collectionName:1")
        field.text = "Act 2"
        field.editingFinished()
        compare(show.moods.collections[1].name, "Act 2")
        compare(show.moods.collections[0], test.moods.collections[0])
    }

    function test_a_picture_keeps_its_stage_until_the_file_comes() {
        show = Object.assign({}, show, { stages: [{ name: "Back" }, { name: "Front" }] })
        const d = dialog()
        d.chosenId = "g-forest"
        const kind = findChild(d.contentItem, "kind:Light")
        kind.activated(kind.model.indexOf("Picture"))
        const stage = findChild(d.contentItem, "stage:Light")
        stage.activated(1)
        compare(acted.length, 0)
        compare(stage.currentText, "Front")
        findChild(d.contentItem, "file:Light").edited("/pictures/sea.mp4")
        compare(sent().groups[1].base["l-light"].picture, { file: "/pictures/sea.mp4", stage: "Front", repeat: "forever" })
    }

    function test_a_refused_name_does_not_stay() {
        const d = dialog()
        d.chosenId = "g-tavern"
        refuse = true
        const field = findChild(d.contentItem, "moodName")
        field.text = "Forest"
        field.editingFinished()
        compare(acted.length, 1)
        compare(field.text, "Tavern")
        refuse = false
        field.text = "Inn"
        field.editingFinished()
        compare(field.text, "Inn")
    }

    function test_new_names_are_free_without_case() {
        show.moods.groups[1].name = "group"
        show = Object.assign({}, show)
        const d = dialog()
        findChild(d.contentItem, "addGroup").clicked()
        compare(show.moods.groups[2].name, "Group 2")
    }

    function test_a_variant_and_a_collection_name_their_removal() {
        const d = dialog()
        d.chosenId = "v-night"
        findChild(d.contentItem, "removePlace").clicked()
        compare(asked.name, "Tavern / Night")
        d.tab = 2
        findChild(d.contentItem, "removeCollection:0").clicked()
        compare(asked.name, "Act 1")
        compare(acted.length, 0)
    }

    // The removal is built when it is accepted, from the moods then.
    function test_a_removal_keeps_what_changed_while_it_asked() {
        const d = dialog()
        d.tab = 2
        findChild(d.contentItem, "removeCollection:0").clicked()
        const moods = JSON.parse(JSON.stringify(show.moods))
        moods.groups[1].name = "Woods"
        show = Object.assign({}, show, { moods: moods })
        accept()
        const out = sent()
        compare(out.collections, [])
        compare(out.groups[1].name, "Woods")
    }

    function test_quick_edits_build_on_each_other() {
        slow = true
        const d = createTemporaryObject(answeredComponent, test)
        d.open()
        findChild(d.contentItem, "addGroup").clicked()
        findChild(d.contentItem, "addGroup").clicked()
        compare(acted.length, 2)
        compare(acted[1].moods.groups.map(g => g.name), ["Tavern", "Forest", "Group", "Group 2"])
        compare(d.groups.length, 4, "shows what was sent")
        // A state before all answers keeps the draft.
        show = Object.assign({}, show, { moods: acted[0].moods })
        compare(d.groups.length, 4)
        fakeCore.answered(JSON.stringify({ action: "set_moods" }), "")
        fakeCore.answered(JSON.stringify({ action: "set_moods" }), "")
        show = Object.assign({}, show, { moods: acted[1].moods })
        compare(d.draft, null)
        compare(d.groups.length, 4)
    }

    function test_a_refused_edit_drops_the_draft() {
        slow = true
        const d = createTemporaryObject(answeredComponent, test)
        d.open()
        findChild(d.contentItem, "addGroup").clicked()
        verify(d.draft !== null)
        fakeCore.answered(JSON.stringify({ action: "set_moods" }), "too many groups")
        compare(d.draft, null)
        compare(d.groups.length, 2)
    }

    /** A real drag of `item` from its middle onto the middle of `onto`. */
    function drag(item, onto) {
        const from = item.mapToItem(null, item.width / 2, item.height / 2)
        const to = onto.mapToItem(null, onto.width / 2, onto.height / 2)
        const root = test.Window.contentItem
        mousePress(root, from.x, from.y)
        for (let step = 1; step <= 10; step++) {
            mouseMove(root, from.x + (to.x - from.x) * step / 10, from.y + (to.y - from.y) * step / 10, -1, Qt.LeftButton)
        }
        mouseRelease(root, to.x, to.y)
    }

    function test_places_are_dragged_into_order() {
        const d = dialog()
        waitForRendering(d.contentItem)
        drag(findChild(d.contentItem, "place:Forest"), findChild(d.contentItem, "place:Tavern"))
        compare(show.moods.groups.map(g => g.name), ["Forest", "Tavern"])
        // A variant stays in its group: dropped on another group, nothing.
        acted = []
        drag(findChild(d.contentItem, "place:Tavern/Night"), findChild(d.contentItem, "place:Forest"))
        compare(acted.length, 0)
    }

    function test_layers_are_dragged_by_their_grip() {
        const d = dialog()
        d.tab = 1
        waitForRendering(d.contentItem)
        // Straight up: the grip moves rows, not sideways.
        drag(findChild(d.contentItem, "layerGrip:1"), findChild(d.contentItem, "layerGrip:0"))
        compare(show.moods.layers.map(l => l.name), ["Music", "Light"])
        compare(d.dragList, "")
    }

    function test_a_layer_sound_gets_its_level_and_crosspoints() {
        show = Object.assign({}, show, { zones: [{ name: "Hall", channels: 2 }] })
        const d = dialog()
        d.chosenId = "g-tavern"
        findChild(d.contentItem, "media:Music").clicked()
        const media = findChild(d, "mediaDialog")
        tryVerify(() => media.opened)
        compare(media.title, "Layer Music")
        const level = findChild(media.contentItem, "mediaLevel")
        level.value = -60
        level.valueModified()
        compare(sent().groups[0].base["l-music"].sound, { file: "/sounds/lute.wav", repeat: "forever", gain_db: -6 })
        // The rest of the moods stays as it was.
        compare(sent().groups[0].base["l-light"], test.moods.groups[0].base["l-light"])
        media.close()
    }
}
