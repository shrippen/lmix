import QtQuick
import QtQuick.Controls as QQC2
import QtTest
import "../../qml"

/**
 * The keys overview (KeysDialog, docs/DESIGN-DIALOGE.md S02 A): keycaps in
 * groups, the own shortcuts as a third group, a search over key and
 * description, and "Shortcuts…" in the footer.
 */
TestCase {
    id: test
    name: "KeysDialog"
    when: windowShown
    width: 1000
    height: 700

    Component {
        id: dialogComponent
        KeysDialog {
            always: [
                { what: "Pages", key: "Ctrl+1…5" },
                { what: "Save show", key: "Ctrl+S" },
                { what: "Panic all", gesture: "button in the page bar" }
            ]
            pageTitle: "Mix"
            pageKeys: [
                { what: "Input to preview", key: "1…9" },
                { what: "Quick play of preview", gesture: "double click preview" }
            ]
            own: [
                { what: "Sea weather on", key: "F2" },
                { what: "Credits", key: "Ctrl+B" }
            ]
        }
    }

    function openDialog() {
        var dialog = createTemporaryObject(dialogComponent, test)
        dialog.open()
        tryCompare(dialog, "opened", true)
        return dialog
    }

    function titles(dialog) {
        return dialog.groups.map(g => g.title)
    }

    function find(item, name, out) {
        out = out || []
        if (item.objectName === name && item.visible) {
            out.push(item)
        }
        for (var i = 0; i < item.children.length; ++i) {
            find(item.children[i], name, out)
        }
        return out
    }

    function test_three_groups_the_own_shortcuts_last() {
        var dialog = openDialog()
        compare(titles(dialog), ["Always", "Only on Mix", "Own shortcuts"])
        compare(dialog.groups[2].keys.length, 2)
        tryCompare(find(dialog.contentItem, "keysGroup"), "length", 3)
        compare(find(dialog.contentItem, "keysEntry").length, 7)
    }

    function test_a_group_without_keys_is_left_out() {
        var dialog = openDialog()
        dialog.own = []
        dialog.pageKeys = []
        compare(titles(dialog), ["Always"])
    }

    function test_keys_split_into_caps() {
        var dialog = openDialog()
        compare(dialog.caps("Ctrl+Shift+S"), ["Ctrl", "Shift", "S"])
        compare(dialog.caps("Ctrl++"), ["Ctrl", "+"])
        compare(dialog.caps("1…9"), ["1…9"])
    }

    function test_search_takes_focus_and_filters_by_description() {
        var dialog = openDialog()
        keyClick(Qt.Key_C)
        keyClick(Qt.Key_R)
        keyClick(Qt.Key_E)
        compare(dialog.query, "cre")
        compare(titles(dialog), ["Own shortcuts"])
        compare(dialog.groups[0].keys[0].what, "Credits")
        tryCompare(find(dialog.contentItem, "keysEntry"), "length", 1)
    }

    function test_search_filters_by_key_in_any_order() {
        var dialog = openDialog()
        dialog.query = "b ctrl"
        compare(titles(dialog), ["Own shortcuts"])
        compare(dialog.groups[0].keys.map(k => k.what), ["Credits"])
        dialog.query = "Ctrl+S"
        compare(dialog.groups[0].keys.map(k => k.what), ["Save show"])
        dialog.query = "F2"
        compare(dialog.groups.map(g => g.keys.length), [1])
        compare(dialog.groups[0].keys[0].what, "Sea weather on")
    }

    function test_a_trailing_plus_is_ignored() {
        var dialog = openDialog()
        dialog.query = "ctrl+"
        compare(dialog.groups.map(g => g.keys.length), [2, 1])
        compare(titles(dialog), ["Always", "Own shortcuts"])
    }

    function test_the_last_part_of_a_key_is_a_prefix() {
        var dialog = openDialog()
        dialog.own = [{ what: "Save all", key: "Ctrl+Shift+S" }]
        dialog.query = "ctrl+shi"
        compare(titles(dialog), ["Own shortcuts"])
        dialog.query = "shift+ctrl+"
        compare(titles(dialog), ["Own shortcuts"])
        dialog.own = []
        dialog.query = "ctrl+sa"
        compare(dialog.groups.length, 0)
        dialog.query = "ctrl+s"
        compare(dialog.groups[0].keys.map(k => k.what), ["Save show"])
        dialog.query = "ctr+s"
        compare(dialog.groups.length, 0)
    }

    function test_a_range_finds_each_of_its_keys() {
        var dialog = openDialog()
        dialog.query = "ctrl+3"
        compare(titles(dialog), ["Always"])
        compare(dialog.groups[0].keys.map(k => k.what), ["Pages"])
        dialog.query = "ctrl+6"
        compare(dialog.groups.length, 0)
        dialog.query = "+7"
        compare(dialog.groups.length, 0)
        dialog.query = "7"
        compare(titles(dialog), ["Only on Mix"])
    }

    function whats(dialog) {
        return [].concat(...dialog.groups.map(g => g.keys.map(k => k.what)))
    }

    function test_each_part_takes_a_cap_of_its_own() {
        var dialog = openDialog()
        dialog.always = []
        dialog.pageKeys = []
        dialog.own = [
            { what: "Save", key: "Strg+S" },
            { what: "New all", key: "Strg+Umschalt+N" },
            { what: "Play", key: "Strg+Leertaste" },
            { what: "Save en", key: "Ctrl+S" },
            { what: "New en", key: "Ctrl+Shift+N" },
            { what: "Play en", key: "Ctrl+Space" }
        ]
        dialog.query = "strg+s"
        compare(whats(dialog), ["Save"])
        dialog.query = "ctrl+s"
        compare(whats(dialog), ["Save en"])
        dialog.query = "strg+um"
        compare(whats(dialog), ["New all"])
        dialog.query = "ctrl+shift+n"
        compare(whats(dialog), ["New en"])
        dialog.query = "ctrl+sp"
        compare(whats(dialog), ["Play en"])
        // A cap counts once: Strg does not stand for two parts.
        dialog.query = "strg+strg"
        compare(whats(dialog), [])
    }

    function test_a_letter_typed_last_may_start_a_function_key() {
        var dialog = openDialog()
        dialog.always = []
        dialog.pageKeys = []
        dialog.own = [
            { what: "Quit", key: "Alt+F4" },
            { what: "Lights", key: "Alt+F12" },
            { what: "Find", key: "Alt+F" },
            { what: "Sea weather on", key: "F2" }
        ]
        // Still typing "alt+f4": the list does not empty on the way.
        dialog.query = "alt+f"
        compare(whats(dialog), ["Quit", "Lights", "Find"])
        dialog.query = "alt+f1"
        compare(whats(dialog), ["Lights"])
        dialog.query = "alt+f4"
        compare(whats(dialog), ["Quit"])
        // A part typed before the last one is done: "f" is the key F only.
        dialog.query = "f+alt"
        compare(whats(dialog), ["Find"])
    }

    function test_key_names_read_as_in_the_rest_of_the_dialog() {
        // One helper for the built-in keys and the own bindings (Texts.keyLabel):
        // the names a binding is stored with become the names the dialog shows.
        compare(Texts.keyLabel("Ctrl+Space"), Texts.keyLabel("Ctrl") + "+" + Texts.keyLabel("Space"))
        compare(Texts.keyLabel("ctrl+shift+s"), "Ctrl+Shift+S")
        compare(Texts.keyLabel("alt+return"), "Alt+Return")
        compare(Texts.keyLabel("Esc"), Texts.keyLabel("Escape"))
        compare(Texts.keyLabel("Delete"), Texts.keyLabel("Del"))
        compare(Texts.keyLabel("pgup"), "PgUp")
        compare(Texts.keyLabel("left"), "Left")
        compare(Texts.keyLabel("f4"), "F4")
        compare(Texts.keyLabel("Ctrl++"), "Ctrl++")
        compare(Texts.keyLabel("Ctrl+1…5"), "Ctrl+1…5")
    }

    function test_a_plus_in_a_name_finds_it_too() {
        var dialog = openDialog()
        dialog.own = [{ what: "Ton+Licht", key: "F5" }]
        dialog.query = "ton+licht"
        compare(whats(dialog), ["Ton+Licht"])
    }

    function test_search_finds_gestures() {
        var dialog = openDialog()
        dialog.query = "double"
        compare(titles(dialog), ["Only on Mix"])
    }

    function test_nothing_found_says_so_and_closing_clears_the_search() {
        var dialog = openDialog()
        dialog.query = "zzz"
        compare(dialog.groups.length, 0)
        dialog.close()
        tryCompare(dialog, "visible", false)
        compare(dialog.query, "")
    }

    function test_the_footer_opens_the_shortcuts() {
        var dialog = openDialog()
        var asked = 0
        dialog.shortcutsRequested.connect(() => asked++)
        // An own button at the start of the footer, no standard one.
        verify(!dialog.standardButton(QQC2.Dialog.Help))
        compare(dialog.footerStart.length, 1)
        var button = dialog.footerStart[0]
        compare(button.text, "Shortcuts…")
        compare(button.parent.parent, dialog.footer)
        button.clicked()
        compare(asked, 1)
        tryCompare(dialog, "visible", false)
    }
}
