import QtQuick
import QtTest
import QtQuick.Controls as QQC2
import Kante
import org.kde.kirigami as Kirigami
import "../../qml"

/**
 * Editing the moods (MOOD.12, MOOD.9): every edit sends the whole moods
 * (`set_moods`) with only that change; a variant's layer can take the
 * base's again; a new sound or picture waits for its file, keeping what
 * is chosen meanwhile; a refused name does not stay; quick edits build
 * on each other until the state has their answers (only its own count);
 * a drag moves only from its grip and only when let go; a removal asks
 * (Removals) and leaves nothing behind; the panic mood holds actions only.
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
    /** Like the bridge not taking the action (`act` false). */
    property bool unsent: false
    /** Like lmix-core: what it takes becomes the show's. */
    function act(action) {
        acted.push(action)
        if (unsent) {
            return false
        }
        if (action.action === "set_moods" && !refuse && !slow) {
            show = Object.assign({}, show, { moods: action.moods })
        }
        return true
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
        unsent = false
        asked = null
        show = { moods: JSON.parse(JSON.stringify(test.moods)), stages: [{ name: "Back" }], inputs: [], outputs: [] }
    }

    QtObject {
        id: fakeCore
        signal answered(string action, string refusal)
        function choices(what) { return "[]" }
        /** Like lmix-core's types reading and writing an action: keys in a
            fixed order, empty fields and the default `repeat: "once"` left
            out; "" for no action. */
        function canonicalAction(json) {
            const write = v => {
                if (Array.isArray(v)) {
                    return v.map(write)
                }
                if (v !== null && typeof v === "object") {
                    const out = {}
                    Object.keys(v).sort().forEach(k => {
                        const empty = v[k] === null || v[k] === undefined || (Array.isArray(v[k]) && v[k].length === 0)
                            || (typeof v[k] === "object" && v[k] !== null && !Array.isArray(v[k]) && Object.keys(v[k]).length === 0)
                        if (!empty && !(k === "repeat" && v[k] === "once")) {
                            out[k] = write(v[k])
                        }
                    })
                    return out
                }
                return v
            }
            const action = JSON.parse(json)
            return typeof action.action === "string" ? JSON.stringify(write(action)) : ""
        }
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

    /** Like the Console answering `action`: written back by lmix-core's
        types (fakeCore.canonicalAction). */
    function answer(action, refusal) {
        fakeCore.answered(fakeCore.canonicalAction(JSON.stringify(action)), refusal)
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
        answer(acted[0], "")
        answer(acted[1], "")
        show = Object.assign({}, show, { moods: acted[1].moods })
        compare(d.draft, null)
        compare(d.groups.length, 4)
    }

    function test_an_edit_after_a_removal_builds_on_it() {
        slow = true
        const d = createTemporaryObject(answeredComponent, test)
        d.open()
        d.chosenId = "g-tavern"
        findChild(d.contentItem, "removePlace").clicked()
        accept()
        compare(d.groups.map(g => g.name), ["Forest"], "shows the removal at once")
        findChild(d.contentItem, "addGroup").clicked()
        compare(acted[1].moods.groups.map(g => g.name), ["Forest", "Group"])
        d.close()
        compare(d.draft, null, "closing lets go of edits never answered")
    }

    function test_a_refused_edit_drops_the_draft() {
        slow = true
        const d = createTemporaryObject(answeredComponent, test)
        d.open()
        findChild(d.contentItem, "addGroup").clicked()
        verify(d.draft !== null)
        answer(acted[0], "too many groups")
        compare(d.draft, null)
        compare(d.groups.length, 2)
    }

    function test_only_its_own_answers_end_the_draft() {
        slow = true
        const d = createTemporaryObject(answeredComponent, test)
        d.open()
        findChild(d.contentItem, "addGroup").clicked()
        compare(d.inFlight, 1)
        // Another part of the Console set the moods (a restore, the moods
        // page): its answer and its state are not this edit's.
        const other = JSON.parse(JSON.stringify(test.moods))
        other.groups[1].name = "Woods"
        answer({ action: "set_moods", moods: other }, "")
        answer({ action: "set_moods", moods: other }, "refused elsewhere")
        answer({ action: "cut" }, "")
        compare(d.inFlight, 1)
        show = Object.assign({}, show, { moods: other })
        compare(d.groups.map(g => g.name), ["Tavern", "Forest", "Group"], "keeps the draft")
        // Its own answer, then the state with it.
        answer(acted[0], "")
        compare(d.inFlight, 0)
        show = Object.assign({}, show, { moods: acted[0].moods })
        compare(d.draft, null)
    }

    function test_an_answer_written_otherwise_still_ends_the_draft() {
        slow = true
        const d = createTemporaryObject(answeredComponent, test)
        d.open()
        d.chosenId = "g-tavern"
        const loop = findChild(d.contentItem, "loop:Music")
        verify(loop.checked)
        loop.toggle()
        loop.toggled()
        compare(acted[0].moods.groups[0].base["l-music"].sound.repeat, "once")
        // lmix-core leaves the default repeat out of its answer.
        verify(fakeCore.canonicalAction(JSON.stringify(acted[0])).indexOf("once") < 0)
        compare(d.inFlight, 1)
        answer(acted[0], "")
        compare(d.inFlight, 0)
        show = Object.assign({}, show, { moods: acted[0].moods })
        compare(d.draft, null)
        // Changes from elsewhere (an Undo) show again and stay.
        const other = JSON.parse(JSON.stringify(acted[0].moods))
        other.groups[1].name = "Woods"
        show = Object.assign({}, show, { moods: other })
        compare(d.groups[1].name, "Woods")
    }

    function test_an_edit_not_sent_is_not_waited_for() {
        slow = true
        unsent = true
        const d = createTemporaryObject(answeredComponent, test)
        d.open()
        findChild(d.contentItem, "addGroup").clicked()
        compare(acted.length, 1)
        compare(d.inFlight, 0)
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
        drag(findChild(d.contentItem, "placeGrip:Forest"), findChild(d.contentItem, "placeGrip:Tavern"))
        compare(show.moods.groups.map(g => g.name), ["Forest", "Tavern"])
        compare(d.dragList, "")
        // A variant stays in its group: dropped on another group, nothing.
        acted = []
        drag(findChild(d.contentItem, "placeGrip:Tavern/Night"), findChild(d.contentItem, "placeGrip:Forest"))
        compare(acted.length, 0)
    }

    function test_dragging_a_place_off_its_grip_moves_nothing() {
        const d = dialog()
        waitForRendering(d.contentItem)
        // The rest of the row is the list's, to scroll (on touch, too).
        drag(findChild(d.contentItem, "place:Forest"), findChild(d.contentItem, "place:Tavern"))
        compare(acted.length, 0)
        compare(d.dragList, "")
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

    Component {
        id: otherWindow
        Window { visible: true; width: 100; height: 100 }
    }

    function test_a_drag_taken_away_drops_nothing() {
        const d = dialog()
        d.tab = 1
        waitForRendering(d.contentItem)
        const grip = findChild(d.contentItem, "layerGrip:1")
        const from = grip.mapToItem(null, grip.width / 2, grip.height / 2)
        const to = findChild(d.contentItem, "layerGrip:0").mapToItem(null, grip.width / 2, grip.height / 2)
        const root = test.Window.contentItem
        mousePress(root, from.x, from.y)
        for (let step = 1; step <= 10; step++) {
            mouseMove(root, from.x, from.y + (to.y - from.y) * step / 10, -1, Qt.LeftButton)
        }
        verify(grip.active)
        compare(d.dragList, "layers")
        // Taken away before the pointer is let go: another window comes up.
        const other = createTemporaryObject(otherWindow, test)
        other.requestActivate()
        tryVerify(() => !grip.active)
        mouseRelease(root, to.x, to.y)
        compare(acted.length, 0)
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

    function test_a_layer_picture_gets_its_opacity_and_layer() {
        const picture = { file: "/pictures/sea.mp4", stage: "Back", repeat: "forever" }
        const moods = JSON.parse(JSON.stringify(test.moods))
        moods.groups[1].base = { "l-light": { kind: "picture", picture: picture } }
        show = Object.assign({}, show, { moods: moods })
        const d = dialog()
        d.chosenId = "g-forest"
        findChild(d.contentItem, "media:Light").clicked()
        const media = findChild(d, "mediaDialog")
        tryVerify(() => media.opened)
        compare(media.title, "Layer Light")
        verify(!findChild(media.contentItem, "mediaLevel").visible, "no level for a picture")
        const opacity = findChild(media.contentItem, "mediaOpacity")
        const layer = findChild(media.contentItem, "mediaPictureLayer")
        verify(opacity.visible)
        verify(layer.visible)
        compare(opacity.value, 100, "unset: fully opaque")
        compare(layer.value, 1, "unset: layer 1")
        opacity.value = 40
        opacity.valueModified()
        compare(sent().groups[1].base["l-light"].picture, Object.assign({ opacity: 0.4 }, picture))
        // The next builds on the state the first brought back.
        acted = []
        compare(opacity.value, 40)
        layer.value = 3
        layer.valueModified()
        compare(sent().groups[1].base["l-light"].picture, Object.assign({ opacity: 0.4, layer: 3 }, picture))
        // The rest of the moods stays as it was.
        compare(sent().groups[0], test.moods.groups[0])
        media.close()
    }

    /** The moods with `n` layers. */
    function manyLayers(n) {
        const moods = JSON.parse(JSON.stringify(test.moods))
        for (let i = moods.layers.length; i < n; i++) {
            moods.layers.push({ id: "l-" + i, name: "Layer " + i })
        }
        return moods
    }
    /** `item` lies inside the dialog's content, not below its foot. */
    function inside(d, item) {
        const at = item.mapToItem(d.contentItem, 0, 0)
        return item.visible && at.y >= 0 && at.y + item.height <= d.contentItem.height + 1
    }

    // Many layers scroll; "+ Layer" and the change times stay in reach.
    function test_many_layers_scroll_and_add_stays_in_reach() {
        show = Object.assign({}, show, { moods: manyLayers(16) })
        const d = dialog()
        d.tab = 1
        waitForRendering(d.contentItem)
        const view = findChild(d.contentItem, "layersView")
        verify(view.contentHeight > view.height, "the rows scroll: " + view.contentHeight + " in " + view.height)
        const add = findChild(d.contentItem, "addLayer")
        verify(inside(d, add), "+ Layer in the dialog")
        mouseClick(add)
        compare(sent().layers.length, 17)
        // The panic mood scrolls the same way.
        d.tab = 3
        waitForRendering(d.contentItem)
        const panic = findChild(d.contentItem, "panicView")
        verify(panic.contentHeight > panic.height)
        verify(inside(d, panic))
    }

    // In a small window the dialog stays inside it; the tabs scroll.
    function test_the_dialog_keeps_to_its_size_step() {
        const d = dialog()
        compare(d.title, "Edit moods")
        compare(d.size, KanteDialog.Size.Large)
        verify(d.width <= test.Window.width * 0.9 + 1)
        verify(d.height <= test.Window.height * 0.9 + 1)
        d.chosenId = "g-tavern"
        findChild(d.contentItem, "media:Music").clicked()
        const media = findChild(d, "mediaDialog")
        tryVerify(() => media.opened)
        compare(media.size, KanteDialog.Size.Medium)
        media.close()
    }

    // The list keeps its width with a choice and without: the buttons do
    // not jump or wrap differently.
    function test_the_list_keeps_its_width() {
        const d = dialog()
        waitForRendering(d.contentItem)
        const column = findChild(d.contentItem, "placesColumn")
        const width = column.width
        d.chosenId = "g-tavern"
        waitForRendering(d.contentItem)
        compare(column.width, width)
        // Remove at the end of its row, inside the column.
        const remove = findChild(d.contentItem, "removePlace")
        const at = remove.mapToItem(column, 0, 0)
        verify(at.x + remove.width <= column.width + 1)
        compare(remove.parent, findChild(d.contentItem, "placeUp").parent, "beside up and down")
    }

    // The file of a sound has a line of its own, wide enough to read.
    function test_a_file_is_wide_enough_to_read() {
        const d = dialog()
        d.chosenId = "g-tavern"
        waitForRendering(d.contentItem)
        const file = findChild(d.contentItem, "file:Music")
        verify(file.field.width > Kirigami.Units.gridUnit * 14, "the path field is " + file.field.width + " px")
        compare(findChild(d.contentItem, "media:Music").text, "Level…")
    }

    function test_a_picture_has_opacity_not_level() {
        const moods = JSON.parse(JSON.stringify(test.moods))
        moods.groups[1].base = { "l-light": { kind: "picture", picture: { file: "/pictures/sea.mp4", stage: "Back" } } }
        show = Object.assign({}, show, { moods: moods })
        const d = dialog()
        d.chosenId = "g-forest"
        compare(findChild(d.contentItem, "media:Light").text, "Opacity…")
    }

    // Escape in a name goes back to the name kept, sends nothing and
    // leaves the dialog open.
    function test_escape_in_a_name_cancels() {
        const d = dialog()
        tryVerify(() => d.opened)
        d.chosenId = "g-tavern"
        const field = findChild(d.contentItem, "moodName")
        field.forceActiveFocus()
        field.selectAll()
        keyClick(Qt.Key_X)
        compare(field.text, "x")
        keyClick(Qt.Key_Escape)
        compare(field.text, "Tavern")
        verify(d.opened, "the first Escape is the field's")
        // Leaving the field now sends nothing either.
        field.editingFinished()
        compare(acted.length, 0)
        d.tab = 1
        waitForRendering(d.contentItem)
        const layer = findChild(d.contentItem, "layerName:0")
        layer.forceActiveFocus()
        layer.selectAll()
        keyClick(Qt.Key_Y)
        keyClick(Qt.Key_Escape)
        compare(layer.text, "Light")
        compare(acted.length, 0)
        // Once more, with nothing to take back: Escape closes the dialog.
        keyClick(Qt.Key_Escape)
        tryVerify(() => !d.visible)
        compare(acted.length, 0)
    }

    // The layers' own fade has a head over its column.
    function test_the_fade_column_has_a_head() {
        const d = dialog()
        d.tab = 1
        waitForRendering(d.contentItem)
        const head = findChild(d.contentItem, "fadeHead")
        verify(head.visible)
        compare(head.text, "Own fade")
        const row = findChild(d.contentItem, "layerRow:0")
        verify(head.x > 0)
        compare(head.x, row.fadeX)
    }

    Component {
        id: smallWindow
        Window {
            visible: true
            width: 1000
            height: 420
            property alias dialog: smallDialog
            MoodsDialog {
                id: smallDialog
                parent: QQC2.Overlay.overlay
                anchors.centerIn: parent
                act: test.act
                askRemove: test.askRemove
                show: test.show
                core: null
            }
        }
    }

    // In a low window the dialog keeps to 90 % of it; the layers scroll and
    // "+ Layer" stays in reach.
    function test_a_low_window_keeps_the_dialog_inside() {
        show = Object.assign({}, show, { moods: manyLayers(14) })
        const win = createTemporaryObject(smallWindow, test)
        const d = win.dialog
        d.tab = 1
        d.open()
        tryVerify(() => d.opened)
        waitForRendering(d.contentItem)
        verify(d.height <= win.height * 0.9 + 1, "dialog " + d.height + " in " + win.height)
        verify(inside(d, findChild(d.contentItem, "addLayer")), "+ Layer in reach")
        const view = findChild(d.contentItem, "layersView")
        verify(view.contentHeight > view.height)
        d.close()
    }
}
