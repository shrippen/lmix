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
        var button = dialog.standardButton(QQC2.Dialog.Help)
        verify(button)
        compare(button.text, "Shortcuts…")
        button.clicked()
        compare(asked, 1)
        tryCompare(dialog, "visible", false)
    }
}
