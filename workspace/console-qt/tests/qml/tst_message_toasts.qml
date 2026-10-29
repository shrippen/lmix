import QtQuick
import QtTest
import "../../qml"

/**
 * Refusals and errors as toasts (MessageToasts, docs/DESIGN-DIALOGE.md
 * rule 2): one per text, at most `limit`, each until it is closed.
 */
TestCase {
    id: test
    name: "MessageToasts"
    when: windowShown
    width: 800
    height: 600

    QtObject {
        id: fakeCore
        property string message: ""
        signal answered(string action, string refusal)
    }

    Component {
        id: toastsComponent
        MessageToasts {
            anchors.fill: parent
            core: fakeCore
        }
    }

    function refusal(text) {
        return JSON.stringify({ message: text })
    }

    function shownToasts(toasts) {
        var shown = []
        for (var i = 0; i < toasts.column.children.length; i++) {
            var child = toasts.column.children[i]
            if (child.shown) {
                shown.push(child)
            }
        }
        return shown
    }

    function test_a_refusal_stays_until_closed() {
        var toasts = createTemporaryObject(toastsComponent, test)
        fakeCore.answered("{}", refusal("Show is locked"))
        fakeCore.answered("{}", "")
        tryCompare(shownToasts(toasts), "length", 1)
        var toast = shownToasts(toasts)[0]
        compare(toast.text, "Show is locked")
        compare(toast.life, 0)
        toast.actionTriggered()
        toast.hide()
        tryCompare(shownToasts(toasts), "length", 0)
        // Refused again after closing: it comes back.
        fakeCore.answered("{}", refusal("Show is locked"))
        tryCompare(shownToasts(toasts), "length", 1)
    }

    function test_the_same_text_shows_once() {
        var toasts = createTemporaryObject(toastsComponent, test)
        fakeCore.answered("{}", refusal("Stinger running"))
        fakeCore.message = refusal("Stinger running")
        fakeCore.answered("{}", refusal("Stinger running"))
        tryCompare(shownToasts(toasts), "length", 1)
        fakeCore.message = ""
    }

    function test_the_oldest_goes_first() {
        var toasts = createTemporaryObject(toastsComponent, test)
        for (var i = 1; i <= toasts.limit + 2; i++) {
            fakeCore.answered("{}", refusal("Refusal " + i))
        }
        tryCompare(shownToasts(toasts), "length", toasts.limit)
        compare(shownToasts(toasts)[0].text, "Refusal 3")
    }
}
