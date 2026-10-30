import QtQuick
import QtTest
import "../../qml"

/**
 * Factory settings only once the word is typed (FactoryResetDialog,
 * docs/DESIGN-DIALOGE.md F12).
 */
TestCase {
    id: test
    name: "FactoryResetDialog"
    when: windowShown
    width: 800
    height: 600

    property var sent: []

    Component {
        id: dialogComponent
        FactoryResetDialog {
            act: action => test.sent.push(action)
        }
    }

    function init() {
        sent = []
    }

    function test_enter_does_nothing_until_the_word_is_typed() {
        var dialog = createTemporaryObject(dialogComponent, test)
        dialog.open()
        tryCompare(dialog, "opened", true)
        compare(dialog.acceptEnabled, false)
        keyClick(Qt.Key_Return)
        compare(dialog.visible, true)
        compare(sent.length, 0)
        for (const c of dialog.word.toUpperCase()) {
            keyClick(c)
        }
        compare(dialog.acceptEnabled, true)
        keyClick(Qt.Key_Return)
        tryCompare(dialog, "visible", false)
        compare(sent, [{ action: "reset_settings" }])
    }

    function test_it_opens_empty_again() {
        var dialog = createTemporaryObject(dialogComponent, test)
        dialog.open()
        tryCompare(dialog, "opened", true)
        for (const c of dialog.word) {
            keyClick(c)
        }
        dialog.reject()
        tryCompare(dialog, "visible", false)
        dialog.open()
        tryCompare(dialog, "opened", true)
        compare(dialog.acceptEnabled, false)
    }
}
