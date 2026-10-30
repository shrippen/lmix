import QtQuick
import QtTest
import "../../qml"

/**
 * Input names are one per input, case aside (INP.2): the add dialog says
 * so at its name field before lmix-core would refuse it and the typed
 * source would be lost; the settings dialog shows the show's name again
 * after a rename lmix-core refused.
 */
TestCase {
    id: test
    name: "InputNames"
    when: windowShown
    width: 800
    height: 600

    property var sent: []
    property var added: []
    property var inputs: [
        { id: "cam", number: 1, name: "Camera", source: { kind: "pattern", pattern: "bars" }, has_audio: false }
    ]

    QtObject {
        id: fakeCore
        property string devices: ""
        function choices(kind) { return [] }
        function refreshLists() {}
        function addInput(kind, value, options, name) {
            test.added.push(name)
            return true
        }
        function replaceSource() { return true }
    }

    Component {
        id: settingsComponent
        InputSettingsDialog {
            act: action => test.sent.push(action)
            core: fakeCore
            inputs: test.inputs
        }
    }

    Component {
        id: addComponent
        AddInputDialog {
            core: fakeCore
            inputs: test.inputs
        }
    }

    function init() {
        sent = []
        added = []
        inputs = [
            { id: "cam", number: 1, name: "Camera", source: { kind: "pattern", pattern: "bars" }, has_audio: false }
        ]
    }

    /** The item named `name` below `item`, or null. */
    function named(item, name) {
        if (item.objectName === name)
            return item
        var kids = item.children || []
        if (item.contentItem && kids.indexOf(item.contentItem) < 0)
            kids = kids.concat([item.contentItem])
        for (var i = 0; i < kids.length; i++) {
            var found = named(kids[i], name)
            if (found)
                return found
        }
        return null
    }

    function test_a_refused_rename_shows_the_show_name_again() {
        var dialog = createTemporaryObject(settingsComponent, test)
        dialog.inputId = "cam"
        dialog.open()
        tryCompare(dialog, "opened", true)
        var field = named(dialog.contentItem, "inputName")
        verify(field)
        field.text = "Clip"
        field.editingFinished()
        compare(sent.length, 1)
        compare(sent[0].name, "Clip")
        // Refused: the show keeps "Camera", and so does the field.
        compare(field.text, "Camera")
        // Taken: the field follows the new name.
        test.inputs = [Object.assign({}, test.inputs[0], { name: "Clip" })]
        compare(field.text, "Clip")
    }

    function test_a_taken_name_holds_the_add_button() {
        var dialog = createTemporaryObject(addComponent, test)
        dialog.open()
        tryCompare(dialog, "opened", true)
        var field = named(dialog.contentItem, "inputName")
        var hint = named(dialog.contentItem, "nameTaken")
        verify(field)
        verify(hint)
        verify(dialog.acceptEnabled, "a pattern needs no more")

        field.text = " camera "
        verify(!dialog.acceptEnabled, "another input has the name")
        verify(hint.visible)

        field.text = "Stage"
        verify(dialog.acceptEnabled)
        verify(!hint.visible)
        dialog.accept()
        compare(added, ["Stage"])
    }
}
