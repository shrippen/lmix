import QtQuick
import QtTest
import "../../qml"

/**
 * Removing with a question and a way back (Removals, docs/DESIGN-DIALOGE.md
 * rule 1): Enter keeps it, the red button removes; the toast's Undo
 * appears only once the core took the removal and names it as the one to
 * bring back, and only for that one.
 */
TestCase {
    id: test
    name: "Removals"
    when: windowShown
    width: 800
    height: 600

    property var sent: []

    QtObject {
        id: fakeCore
        signal answered(string action, string refusal)
    }
    QtObject {
        id: fakeShow
        property var restorable: null
    }
    Item {
        id: toastColumn
        width: 300
        height: 200
    }

    Component {
        id: removalsComponent
        Removals {
            core: fakeCore
            act: action => test.sent.push(action)
            show: fakeShow
            column: toastColumn
        }
    }

    function init() {
        sent = []
        fakeShow.restorable = null
    }

    function toastOf(removals) {
        for (let i = 0; i < toastColumn.children.length; i++) {
            if (toastColumn.children[i].restores !== undefined) {
                return toastColumn.children[i]
            }
        }
        return null
    }

    function dialogOf(removals) {
        for (let i = 0; i < removals.data.length; i++) {
            if (removals.data[i].request !== undefined) {
                return removals.data[i]
            }
        }
        return null
    }

    function test_enter_keeps_it() {
        const removals = createTemporaryObject(removalsComponent, test)
        removals.ask("bus", { id: "b1", name: "In-Ear" })
        const dialog = dialogOf(removals)
        tryCompare(dialog, "opened", true)
        compare(dialog.title, "Remove bus “In-Ear”?")
        keyClick(Qt.Key_Return)
        tryCompare(dialog, "visible", false)
        compare(sent.length, 0)
    }

    function test_removed_then_undo_brings_it_back() {
        const removals = createTemporaryObject(removalsComponent, test)
        removals.ask("layout", { number: 2, name: "Interview" })
        const dialog = dialogOf(removals)
        tryCompare(dialog, "opened", true)
        dialog.accept()
        compare(sent, [{ action: "remove_layout", layout: 2 }])
        const toast = toastOf(removals)
        // Answered, but not yet in the state: no toast.
        fakeCore.answered(JSON.stringify({ action: "remove_layout", layout: 2 }), "")
        wait(50)
        compare(toast.shown, false)
        fakeShow.restorable = { kind: "layout", name: "Interview" }
        tryCompare(toast, "shown", true)
        compare(toast.text, "Layout “Interview” removed")
        compare(toast.actionText, "Undo")
        toast.actionTriggered()
        compare(sent[1], { action: "restore" })
        // Something removed since (from elsewhere): Undo would bring that
        // back, so the toast offers none.
        fakeShow.restorable = { kind: "bus", name: "Stream" }
        compare(toast.actionText, "")
    }

    function test_a_refused_removal_shows_nothing() {
        const removals = createTemporaryObject(removalsComponent, test)
        removals.remove(removals.request("output", { number: 1, name: "CDN" }))
        fakeCore.answered(JSON.stringify({ action: "remove_output", output: 1 }), "Show is locked")
        // The same name removed from elsewhere later does not pass for it.
        fakeShow.restorable = { kind: "output", name: "CDN" }
        wait(50)
        compare(toastOf(removals).shown, false)
    }
}
