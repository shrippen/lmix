import QtQuick
import QtTest
import "../../qml"

/**
 * The Moods page (MOOD.4, 7, 8, 10): a group's head sets its base, a
 * variant tile the variant; a collection shows its moods and one-shots,
 * "Compose" puts them in or out (`set_moods`); both side bars fold; the
 * fade's progress runs only while a change fades.
 */
TestCase {
    id: test
    name: "MoodsPage"
    when: windowShown
    // The progress runs only while shown.
    visible: true
    width: 1400
    height: 800

    readonly property var moods: ({
        layers: [{ id: "l-light", name: "Light" }, { id: "l-music", name: "Music" }],
        groups: [
            {
                id: "g-tavern", name: "Tavern", color: "orange",
                base: {
                    "l-light": { kind: "actions", actions: [{ action: "http_request", target: "Lights", path: "/warm" }] },
                    "l-music": { kind: "sound", sound: { file: "/sounds/lute.wav" } }
                },
                variants: [
                    { id: "v-night", name: "Night", layers: { "l-light": { kind: "off" } } },
                    { id: "v-fight", name: "Fight", layers: { "l-music": { kind: "sound", sound: { file: "/sounds/drums.wav" } } } }
                ]
            },
            { id: "g-forest", name: "Forest", base: {}, variants: [] }
        ],
        collections: [{ id: "c-act1", name: "Act 1", moods: ["v-night"], pads: ["p-door"] }],
        change_ms: 3000,
        inside_ms: 1000
    })
    readonly property var padList: ({
        id: "list-pads", number: 2, name: "Pads", pads: true,
        cues: [{ id: "p-door", name: "Door", kind: "sound" }, { id: "p-thunder", name: "Thunder", kind: "sound" }]
    })

    QtObject {
        id: fakeShell
        property var show: ({ moods: test.moods, mood: {}, cue_lists: [test.padList], running_cues: [] })
        property var acted: []
        function act(action) { acted.push(action) }
        function keyTaken(sequence) { return false }
        function askRemoveWith(request) {}
    }

    Component {
        id: pageComponent
        MoodsPage {
            shell: fakeShell
            width: test.width
            height: test.height
        }
    }

    function init() {
        fakeShell.acted = []
        fakeShell.show = { moods: test.moods, mood: {}, cue_lists: [test.padList], running_cues: [] }
    }

    function page() {
        const p = createTemporaryObject(pageComponent, test)
        p.collectionId = ""
        p.collectionsOpen = true
        p.inspectorOpen = true
        return p
    }

    function test_a_head_sets_the_base_and_a_tile_the_variant() {
        const p = page()
        findChild(p, "base:Tavern").clicked()
        findChild(p, "variant:Tavern/Night").clicked()
        compare(fakeShell.acted, [
            { action: "set_mood", mood: "g-tavern" },
            { action: "set_mood", mood: "v-night" }
        ])
    }

    function test_the_mood_now_shows_with_its_layers_and_their_origin() {
        const p = page()
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: {
            mood: "v-night", previous: "g-tavern",
            layers: [{ layer: "l-music", origin: { kind: "mood", mood: "v-night" } }]
        } })
        compare(findChild(p, "nowName").text, "Tavern / Night")
        compare(p.subtitle, "Tavern / Night")
        const music = findChild(p, "layer:Music")
        compare(music.source, "Base", "the variant takes music from the base")
        compare(findChild(p, "layer:Light").source, "", "off: nothing runs there")
        const back = findChild(p, "back")
        verify(back.enabled)
        back.clicked()
        compare(fakeShell.acted, [{ action: "set_mood", mood: "g-tavern" }])
    }

    function test_a_cue_on_a_layer_says_which() {
        const p = page()
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: {
            mood: "g-tavern",
            layers: [{ layer: "l-music", origin: { kind: "cue", list: "list-pads", cue: "p-thunder" } }]
        } })
        const music = findChild(p, "layer:Music")
        compare(music.source, "Cue")
        compare(p.cueLabel(music.origin.origin), "Thunder")
    }

    function test_the_panic_mood_says_so() {
        const p = page()
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: {
            previous: "g-tavern",
            layers: [{ layer: "l-light", origin: { kind: "panic" } }]
        } })
        compare(findChild(p, "layer:Light").source, "Panic")
        // The panic mood is what holds now: not "no mood" under a panic head.
        compare(findChild(p, "nowName").text, "Panic mood")
        compare(p.subtitle, "Panic mood")
    }

    // The mood set was removed since: lmix-core may still name it (or no
    // mood any more) while its layers run; no base and no panic to name.
    function test_a_removed_mood_names_no_base() {
        const p = page()
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: {
            mood: "g-gone",
            layers: [{ layer: "l-music", origin: { kind: "mood", mood: "g-gone" } }]
        } })
        const music = findChild(p, "layer:Music")
        compare(music.source, "Mood (gone)")
        verify(!findChild(p, "layerText:Music").visible, "no origin text")
        compare(findChild(p, "nowName").text, "Mood (gone)")
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: {
            layers: [{ layer: "l-music", origin: { kind: "mood", mood: "g-gone" } }]
        } })
        compare(music.source, "Mood (gone)")
        compare(findChild(p, "layerText:Music").text, "")
        compare(findChild(p, "nowName").text, "No mood set")
    }

    function test_counts_say_one_and_many() {
        const p = page()
        compare(p.moodCount(1), "1 mood")
        compare(p.moodCount(4), "4 moods")
        compare(p.shotCount(1), "1 one-shot")
        compare(p.shotCount(0), "0 one-shots")
        // In the language's numbers.
        compare(p.secondsLeft(6000, Qt.locale("de_DE")), "6,0 s left")
        compare(p.secondsLeft(6000, Qt.locale("en_GB")), "6.0 s left")
    }

    // The name beside the fade keeps its width: the fade has its room
    // whether it shows or not.
    function test_the_head_does_not_jump_while_a_change_fades() {
        const p = page()
        waitForRendering(p)
        const name = findChild(p, "nowName")
        const fade = findChild(p, "fadeColumn")
        const nameWidth = name.width
        const fadeWidth = fade.width
        compare(fade.opacity, 0)
        verify(findChild(p, "fade").Accessible.ignored, "not read out while hidden")
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: {
            mood: "v-fight", fading: { since_ms: Date.now(), duration_ms: 2000 }
        } })
        verify(p.fading)
        waitForRendering(p)
        compare(fade.opacity, 1)
        compare(fade.width, fadeWidth)
        compare(name.width, nameWidth)
    }

    // Composing, a member is marked with a check (not by a shade only); a
    // head that cannot be clicked is no button to look at.
    function test_members_and_heads_show_what_they_are() {
        const p = page()
        p.collectionId = "c-act1"
        const tavern = findChild(p, "base:Tavern")
        verify(!tavern.enabled)
        compare(tavern.emphasis, 4 /* KanteButton.Emphasis.Quiet */)
        findChild(p, "compose").clicked()
        const night = findChild(p, "variant:Tavern/Night")
        const fight = findChild(p, "variant:Tavern/Fight")
        compare(night.icon.name, "checkmark")
        compare(night.emphasis, 3 /* KanteButton.Emphasis.Data */)
        compare(fight.icon.name, "")
        compare(fight.emphasis, 0 /* KanteButton.Emphasis.Normal */)
        verify(tavern.enabled)
        compare(tavern.emphasis, 0)
        compare(findChild(p, "pad:Door").icon.name, "checkmark")
        compare(findChild(p, "pad:Thunder").icon.name, "")
        findChild(p, "compose").clicked()
        compare(night.icon.name, "", "done: no marks")
    }

    function test_a_variant_tells_its_whole_name() {
        const p = page()
        verify(findChild(p, "variant:Tavern/Night").tip.startsWith("Night\n"))
    }

    function test_a_collection_shows_its_moods_and_one_shots() {
        const p = page()
        compare(p.shownGroups.length, 2, "all")
        compare(p.shownPads.length, 2)
        p.collectionId = "c-act1"
        compare(p.shownGroups.map(g => g.name), ["Tavern"], "a group with a mood in it")
        verify(!findChild(p, "base:Tavern").enabled, "its base is not in the collection")
        verify(findChild(p, "variant:Tavern/Night") !== null)
        compare(findChild(p, "variant:Tavern/Fight"), null)
        compare(p.shownPads.map(x => x.cue.name), ["Door"])
        findChild(p, "pad:Door").clicked()
        compare(fakeShell.acted, [{ action: "cue_toggle", list: 2, cue: "p-door" }])
    }

    function test_compose_puts_moods_and_one_shots_in_or_out() {
        const p = page()
        p.collectionId = "c-act1"
        findChild(p, "compose").clicked()
        verify(p.composing)
        compare(p.shownGroups.length, 2, "composing shows the whole pool")
        findChild(p, "base:Forest").clicked()
        findChild(p, "variant:Tavern/Night").clicked()
        findChild(p, "pad:Thunder").clicked()
        compare(fakeShell.acted.length, 3, "no mood is set while composing")
        const first = fakeShell.acted[0]
        compare(first.action, "set_moods")
        compare(first.moods.collections[0].moods, ["v-night", "g-forest"])
        compare(fakeShell.acted[1].moods.collections[0].moods, [], "out again")
        compare(fakeShell.acted[2].moods.collections[0].pads, ["p-door", "p-thunder"])
        compare(test.moods.collections[0].moods, ["v-night"], "the state stays as it is")
        p.collectionId = ""
        verify(!p.composing, "another collection ends composing")
    }

    function test_a_list_switched_off_keeps_its_one_shots_in_the_collection() {
        // "Door" stays in the collection while its list has no pads: not
        // counted, not shown, kept when composing; back once on again.
        const off = Object.assign({}, test.padList, { pads: false })
        const other = { id: "list-more", number: 3, name: "More", pads: true,
            cues: [{ id: "p-bell", name: "Bell", kind: "sound" }] }
        fakeShell.show = Object.assign({}, fakeShell.show, { cue_lists: [off, other] })
        const p = page()
        compare(findChild(p, "collection:Act 1").subtitle, "1 mood · 0 one-shots")
        p.collectionId = "c-act1"
        compare(p.shownPads.length, 0)
        findChild(p, "compose").clicked()
        findChild(p, "pad:Bell").clicked()
        compare(fakeShell.acted[0].moods.collections[0].pads, ["p-door", "p-bell"])
        fakeShell.show = Object.assign({}, fakeShell.show, { cue_lists: [test.padList, other] })
        compare(findChild(p, "collection:Act 1").subtitle, "1 mood · 1 one-shot")
    }

    function test_a_full_collection_says_that_hidden_one_shots_count() {
        compare(Texts.message(JSON.stringify({ error: { error: "collection_full", collection: "Act 1", max: 256 } })),
            "Collection Act 1 holds at most 256 entries, one-shots of lists without pads included")
    }

    function test_the_side_bars_fold() {
        const p = page()
        const collections = findChild(p, "collections")
        const inspector = findChild(p, "inspector")
        const wide = collections.width
        findChild(p, "foldCollections").clicked()
        findChild(p, "foldInspector").clicked()
        verify(!p.collectionsOpen && !p.inspectorOpen)
        tryVerify(() => collections.width < wide / 2)
        compare(findChild(p, "layer:Light"), null, "folded: no layer rows")
        findChild(p, "foldCollections").clicked()
        tryCompare(collections, "width", wide)
    }

    function test_the_progress_runs_only_while_a_change_fades() {
        const p = page()
        verify(!p.fading)
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: {
            mood: "v-fight", fading: { since_ms: Date.now(), duration_ms: 400 }
        } })
        verify(p.fading, "fades: " + p.fadeProgress)
        verify(findChild(p, "fade").visible, "the bar shows")
        tryVerify(() => p.fadeProgress > 0.2)
        tryVerify(() => !p.fading, 2000, "done after its time")
    }

    function test_keys_set_a_group_and_go_back() {
        const p = page()
        // After a click on a pad as well: the keys are the window's, and
        // the pad has no focus for space to press it again.
        waitForRendering(p)
        mouseClick(findChild(p, "pad:Door"))
        compare(fakeShell.acted.length, 1, "the pad ran")
        verify(!findChild(p, "pad:Door").activeFocus)
        fakeShell.acted = []
        keyClick(Qt.Key_2)
        compare(fakeShell.acted, [{ action: "set_mood", mood: "g-forest" }])
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: { mood: "g-forest", previous: "v-night" } })
        keyClick(Qt.Key_Backspace)
        compare(fakeShell.acted[1], { action: "set_mood", mood: "v-night" })
    }

    function test_keys_do_what_a_click_could() {
        const p = page()
        p.collectionId = "c-act1"
        keyClick(Qt.Key_1)
        compare(fakeShell.acted, [], "the base of Tavern is not in Act 1")
        findChild(p, "compose").clicked()
        fakeShell.show = Object.assign({}, fakeShell.show, { mood: { mood: "g-forest", previous: "v-night" } })
        keyClick(Qt.Key_Backspace)
        keyClick(Qt.Key_2)
        compare(fakeShell.acted, [], "no mood is set while composing")
        findChild(p, "compose").clicked()
        keyClick(Qt.Key_Backspace)
        compare(fakeShell.acted, [{ action: "set_mood", mood: "v-night" }], "done: the keys work again")
    }

    function test_a_collection_of_another_show_shows_all() {
        const p = page()
        p.collectionId = "c-gone"
        compare(p.shownGroups.length, 2)
        compare(p.shownCollectionId, "", "All is marked")
    }

    function test_without_moods_it_says_how_they_come() {
        fakeShell.show = { moods: {}, mood: {}, cue_lists: [], running_cues: [] }
        const p = page()
        compare(p.shownGroups.length, 0)
        verify(!findChild(p, "groups").visible)
        verify(!findChild(p, "oneShots").visible)
    }

    function test_edit_opens_the_moods_dialog() {
        const p = page()
        const dialog = findChild(p, "moodsDialog")
        verify(!dialog.opened)
        findChild(p, "editMoods").clicked()
        tryVerify(() => dialog.opened)
        compare(dialog.moods, test.moods)
        dialog.close()
    }
}
