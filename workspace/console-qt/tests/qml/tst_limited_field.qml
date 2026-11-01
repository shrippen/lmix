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
