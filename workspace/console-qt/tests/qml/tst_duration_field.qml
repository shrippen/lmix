import QtQuick
import QtTest
import "../../qml"

/**
 * A time field (DurationField, PLAN-SEITEN "Stand"): an unreadable time is
 * not dropped. After Enter it stays in the field, marked, and the field
 * keeps the keys; leaving the field keeps it too. Nothing goes to the
 * cue until it reads. Another cue shown (`key`) shows that cue's time.
 */
TestCase {
    id: test
    name: "DurationField"
    when: windowShown
    width: 400
    height: 200

    property var edits: []
    property int accepts: 0

    Item {
        id: elsewhere
        focus: false
        activeFocusOnTab: true
    }

    Component {
        id: fieldComponent
        DurationField {
            width: 200
            ms: 2000
            key: "cue-a"
            onEdited: value => test.edits.push(value)
            // As in the inspector: a taken time gives the keys back.
            onAccepted: {
                test.accepts++
                elsewhere.forceActiveFocus()
            }
        }
    }

    function init() {
        edits = []
        accepts = 0
    }

    function type(text) {
        for (const c of text) {
            keyClick(c)
        }
    }

    function typed(text) {
        const field = createTemporaryObject(fieldComponent, test)
        field.forceActiveFocus()
        field.selectAll()
        type(text)
        return field
    }

    function test_enter_keeps_an_unreadable_time() {
        const field = typed("2x")
        keyClick(Qt.Key_Return)
        compare(field.text, "2x")
        verify(field.invalid, "the field is not marked")
        verify(field.activeFocus, "the field gave the keys away")
        compare(edits.length, 0)
        compare(accepts, 0)
    }

    function test_leaving_keeps_an_unreadable_time() {
        const field = typed("1:7a")
        elsewhere.forceActiveFocus()
        compare(field.text, "1:7a")
        verify(field.invalid, "the field is not marked")
        compare(edits.length, 0)
        // A new state of the same cue does not overwrite it.
        field.ms = 3000
        compare(field.text, "1:7a")
    }

    function test_a_readable_time_is_taken_after_a_wrong_one() {
        const field = typed("x")
        keyClick(Qt.Key_Return)
        field.selectAll()
        type("2.5")
        keyClick(Qt.Key_Return)
        compare(edits, [2500])
        compare(accepts, 1)
        verify(!field.invalid)
    }

    function test_another_cue_shows_its_time() {
        const field = typed("2x")
        elsewhere.forceActiveFocus()
        field.ms = 5000
        field.key = "cue-b"
        compare(field.text, "0:05.0")
        verify(!field.invalid)
    }
}
