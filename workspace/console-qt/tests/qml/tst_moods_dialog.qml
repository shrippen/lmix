import QtQuick
import QtTest
import "../../qml"

/**
 * Editing the moods (MOOD.12, MOOD.9): every edit sends the whole moods
 * (`set_moods`) with only that change; a variant's layer can take the
 * base's again; a new sound waits for its file; a removed group or layer
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
    /** Like lmix-core: what it takes becomes the show's. */
    function act(action) {
        acted.push(action)
        if (action.action === "set_moods") {
            show = Object.assign({}, show, { moods: action.moods })
        }
    }

    Component {
        id: dialogComponent
        MoodsDialog {
            act: test.act
            show: test.show
            core: null
        }
    }

    function init() {
        acted = []
        show = { moods: JSON.parse(JSON.stringify(test.moods)), stages: [{ name: "Back" }], inputs: [], outputs: [] }
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
        const moods = sent()
        compare(moods.groups.map(g => g.id), ["g-forest"])
        compare(moods.collections[0].moods, ["g-forest"])
        compare(d.chosenId, "")
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
        const remove = findChild(d.contentItem, "removeLayer:0")
        remove.clicked()
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
}
