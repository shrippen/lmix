import QtQuick
import Kante

/**
 * A text field that keeps to the length lmix-core takes (`limit`) and
 * cuts on whole characters: maximumLength alone may cut between the two
 * UTF-16 units of an emoji, whose first half would come out as "�";
 * Qt cuts the inserted part, so the half may sit anywhere in the text.
 * The field counts UTF-16 units, lmix-core counts characters: the field
 * is at most stricter than the core, never more lenient.
 * The halves go by remove(), which keeps the undo history: Ctrl+Z takes
 * back a cut paste (setting `text` would clear the history).
 */
KanteTextField {
    id: field

    /** The longest text to type; 0: Qt's own limit. */
    property int limit: 0

    Binding on maximumLength {
        when: field.limit > 0
        value: field.limit
    }
    QtObject {
        id: own
        // remove() changes the text again; the outer call does all halves.
        property bool cutting: false
    }

    onTextChanged: {
        if (own.cutting) {
            return
        }
        const lone = Texts.halves(text)
        if (lone.length === 0) {
            return
        }

        // The cursor stays where it was, less the halves dropped before it.
        const cursor = cursorPosition - lone.filter(i => i < cursorPosition).length
        own.cutting = true
        for (let k = lone.length - 1; k >= 0; k--) {
            remove(lone[k], lone[k] + 1)
        }
        own.cutting = false
        cursorPosition = cursor
    }
}
