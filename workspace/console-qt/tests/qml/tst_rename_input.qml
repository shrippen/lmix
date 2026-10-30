import QtQuick
import QtTest
import "../../qml"

/**
 * One rename dialog for all tiles (RenameInputDialog, docs/DESIGN-DIALOGE.md
 * F13): it renames the input it was opened for, the name selected, and
 * sends nothing for an empty or unchanged name.
 */
TestCase {
    id: test
    name: "RenameInputDialog"
    when: windowShown
    width: 800
    height: 600

    property var sent: []
    property var inputs: [
        { id: "cam", number: 1, name: "Camera" },
        { id: "clip", number: 2, name: "Clip" }
    ]

    Component {
        id: dialogComponent
        RenameInputDialog {
            act: action => test.sent.push(action)
            inputs: test.inputs
        }
    }

    function init() {
        sent = []
    }

    function openFor(inputId) {
        var dialog = createTemporaryObject(dialogComponent, test)
        dialog.inputId = inputId
        dialog.open()
        tryCompare(dialog, "opened", true)
        return dialog
    }

    function test_typing_replaces_the_selected_name() {
        var dialog = openFor("clip")
        compare(dialog.title, "Rename input 2")
        keyClick(Qt.Key_O)
        keyClick(Qt.Key_K)
        keyClick(Qt.Key_Return)
        tryCompare(dialog, "visible", false)
        compare(sent.length, 1)
        compare(sent[0].action, "rename_input")
        compare(sent[0].input, "clip")
        compare(sent[0].name, "ok")
    }

    function test_the_same_dialog_serves_another_input() {
        var dialog = openFor("cam")
        dialog.reject()
        tryCompare(dialog, "visible", false)
        dialog.inputId = "clip"
        dialog.open()
        tryCompare(dialog, "opened", true)
        compare(dialog.title, "Rename input 2")
        keyClick(Qt.Key_Return)
        tryCompare(dialog, "visible", false)
        // Unchanged: nothing to send.
        compare(sent.length, 0)
    }

    function test_an_empty_name_does_not_rename() {
        var dialog = openFor("cam")
        keyClick(Qt.Key_Delete)
        compare(dialog.acceptEnabled, false)
        keyClick(Qt.Key_Return)
        compare(dialog.visible, true)
        compare(sent.length, 0)
    }
}
