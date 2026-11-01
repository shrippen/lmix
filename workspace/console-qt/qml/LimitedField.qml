import QtQuick
import Kante

/**
 * A text field that keeps to the length lmix-core takes (`limit`) and
 * cuts on whole characters: maximumLength alone may cut between the two
 * UTF-16 units of an emoji, whose first half would come out as "�";
 * Qt cuts the inserted part, so the half may sit anywhere in the text.
 * The field counts UTF-16 units, lmix-core counts characters: the field
 * is at most stricter than the core, never more lenient.
 * Cutting sets `text`, which ends a binding on it; a field bound to a
 * value restores the binding itself when editing ends, as before.
 */
KanteTextField {
    id: field

    /** The longest text to type; 0: Qt's own limit. */
    property int limit: 0

    Binding on maximumLength {
        when: field.limit > 0
        value: field.limit
    }
    onTextChanged: {
        const whole = Texts.whole(text)
        if (whole !== text) {
            // The cursor stays where it was, less the halves dropped before it.
            const cursor = Texts.whole(text.slice(0, cursorPosition)).length
            text = whole
            cursorPosition = cursor
        }
    }
}
