import QtQuick
import QtTest
import "../../qml"

/**
 * LimitedField keeps to its `limit` on whole characters: a cut between
 * the two UTF-16 units of an emoji drops the half left over, a field
 * bound to a value is bound again when its owner restores the binding.
 */
TestCase {
    id: test
    name: "LimitedField"
    when: windowShown

    property string kept: "Stage"

    Component {
        id: fieldComponent
        LimitedField {
            limit: 64
        }
    }
    Component {
        id: boundComponent
        LimitedField {
            id: bound
            limit: 64
            text: test.kept
            onEditingFinished: text = Qt.binding(() => test.kept)
        }
    }

    function test_a_cut_keeps_whole_characters() {
        const field = createTemporaryObject(fieldComponent, test)
        compare(field.maximumLength, 64)
        field.text = "M".repeat(70)
        compare(field.text.length, 64)
        field.text = "M".repeat(63) + "\u{1F600}"
        compare(field.text, "M".repeat(63), "the half emoji goes")
        field.text = "M".repeat(62) + "\u{1F600}"
        compare(field.text, "M".repeat(62) + "\u{1F600}", "a whole one fits")
    }

    // Qt cuts the inserted part: an emoji typed or pasted in the middle
    // with one unit left leaves its first half there, not at the end.
    function test_an_insert_in_the_middle_keeps_whole_characters() {
        const field = createTemporaryObject(fieldComponent, test, { limit: 10 })
        field.text = "abcdefghi"
        field.cursorPosition = 2
        field.insert(2, "\u{1F600}")
        compare(field.text, "abcdefghi", "the half emoji goes")
        compare(field.cursorPosition, 2, "the cursor stays")
        // Pasted: a whole one, then one cut in half (four units left).
        field.text = "abcdef"
        field.cursorPosition = 3
        field.insert(3, "X\u{1F600}\u{1F601}")
        compare(field.text, "abcX\u{1F600}def")
        compare(field.cursorPosition, 6, "after what was kept")
    }

    // Dropping the half keeps the undo history: Ctrl+Z takes back the paste.
    function test_a_cut_paste_can_be_undone() {
        const field = createTemporaryObject(fieldComponent, test, { limit: 10 })
        field.text = "abcdef"
        field.cursorPosition = 3
        field.insert(3, "X\u{1F600}\u{1F601}")
        compare(field.text, "abcX\u{1F600}def")
        verify(field.canUndo, "the paste is in the history")
        field.undo()
        compare(field.text, "abcdef", "back to before the paste")
    }

    function test_whole_drops_lone_halves_anywhere() {
        compare(Texts.whole("ab"), "ab")
        compare(Texts.whole("a\u{1F600}b"), "a\u{1F600}b")
        compare(Texts.whole("a\uD83Db"), "ab", "a first half alone")
        compare(Texts.whole("a\uDE00b"), "ab", "a second half alone")
        compare(Texts.whole("\uDE00\uD83D\uDE00\uD83D"), "\u{1F600}")
    }

    function test_no_limit_takes_qts_own() {
        const field = createTemporaryObject(fieldComponent, test, { limit: 0 })
        compare(field.maximumLength, 32767)
        field.text = "M".repeat(100) + "\u{1F600}"
        compare(field.text, "M".repeat(100) + "\u{1F600}")
    }

    function test_the_binding_comes_back_after_a_cut() {
        kept = "Stage"
        const field = createTemporaryObject(boundComponent, test)
        compare(field.text, "Stage")
        field.text = "M".repeat(63) + "\u{1F600}"
        compare(field.text, "M".repeat(63))
        field.editingFinished()
        compare(field.text, "Stage")
        kept = "Foyer"
        compare(field.text, "Foyer", "bound again")
    }
}
