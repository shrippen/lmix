import QtQuick
import QtQuick.Controls as QQC2
import QtTest
import Kante
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
        property string message: ""
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
        removals.ask("layout", { id: "l2", number: 2, name: "Interview" })
        const dialog = dialogOf(removals)
        tryCompare(dialog, "opened", true)
        dialog.accept()
        // By id: the number may move while the question is open.
        compare(sent, [{ action: "remove_layout", layout: "l2" }])
        const toast = toastOf(removals)
        // Answered, but not yet in the state: no toast.
        fakeCore.answered(JSON.stringify({ action: "remove_layout", layout: "l2" }), "")
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

    // An output's new settings: no question, one action, the toast brings
    // back the settings before.
    function test_changed_settings_come_back_with_undo() {
        const removals = createTemporaryObject(removalsComponent, test)
        const update = { action: "update_output", output: "o1", spec: { kind: "ndi", name: "Desk" } }
        removals.change({ id: "o1", name: "CDN" }, update)
        compare(sent, [update])
        const toast = toastOf(removals)
        fakeCore.answered(JSON.stringify(update), "")
        fakeShow.restorable = { kind: "output_settings", name: "CDN" }
        tryCompare(toast, "shown", true)
        compare(toast.title, "Changed")
        compare(toast.text, "Settings of output “CDN” changed")
        compare(toast.actionText, "Undo")
        toast.actionTriggered()
        compare(sent[1], { action: "restore" })
    }

    function test_a_refused_change_shows_nothing() {
        const removals = createTemporaryObject(removalsComponent, test)
        const update = { action: "update_output", output: "o1", spec: { kind: "ndi", name: "Desk" } }
        removals.change({ id: "o1", name: "CDN" }, update)
        fakeCore.answered(JSON.stringify(update), "{\"error\":{\"error\":\"output_sending\"}}")
        fakeShow.restorable = { kind: "output_settings", name: "CDN" }
        wait(50)
        compare(toastOf(removals).shown, false)
    }

    function test_a_refused_removal_shows_nothing() {
        const removals = createTemporaryObject(removalsComponent, test)
        removals.remove(removals.request("output", { id: "o1", number: 1, name: "CDN" }))
        fakeCore.answered(JSON.stringify({ action: "remove_output", output: "o1" }), "Show is locked")
        // The same name removed from elsewhere later does not pass for it.
        fakeShow.restorable = { kind: "output", name: "CDN" }
        wait(50)
        compare(toastOf(removals).shown, false)
    }

    // The answer before the state, while the state still names an earlier
    // removal: the toast waited for null only and was lost.
    function test_the_answer_before_the_state_after_an_earlier_removal() {
        const removals = createTemporaryObject(removalsComponent, test)
        fakeShow.restorable = { kind: "bus", name: "Stream" }
        removals.remove(removals.request("input", { id: "i1", number: 1, name: "Cam 1" }))
        fakeCore.answered(JSON.stringify({ action: "remove_input", input: "i1" }), "")
        wait(50)
        const toast = toastOf(removals)
        compare(toast.shown, false)
        fakeShow.restorable = { kind: "input", name: "Cam 1" }
        tryCompare(toast, "shown", true)
        compare(toast.text, "Input 1 \u201cCam 1\u201d removed")
    }

    // A removal from elsewhere comes between the answer and this state:
    // no toast for something this one did not remove.
    function test_a_removal_from_elsewhere_ends_the_wait() {
        const removals = createTemporaryObject(removalsComponent, test)
        removals.remove(removals.request("layout", { id: "l1", number: 1, name: "Wide" }))
        fakeCore.answered(JSON.stringify({ action: "remove_layout", layout: "l1" }), "")
        fakeShow.restorable = { kind: "bus", name: "Stream" }
        fakeShow.restorable = { kind: "layout", name: "Wide" }
        wait(50)
        compare(toastOf(removals).shown, false)
    }

    // Asked for input 3, input 1 removed elsewhere meanwhile: by number the
    // removal would take the input that is 3 now.
    function test_it_removes_what_it_asked_for() {
        const removals = createTemporaryObject(removalsComponent, test)
        for (const kind of ["input", "layout", "output"]) {
            const asked = removals.request(kind, { id: "x-" + kind, number: 3, name: "Spare" })
            compare(asked.action[kind], "x-" + kind, kind)
        }
    }

    // Templates are not in the stack of removals: a question, then gone,
    // no toast waiting for a way back that never comes.
    function test_a_template_asks_and_goes_without_undo() {
        const removals = createTemporaryObject(removalsComponent, test)
        removals.ask("template", { name: "Interview" })
        const dialog = dialogOf(removals)
        tryCompare(dialog, "opened", true)
        compare(dialog.title, "Remove template \u201cInterview\u201d?")
        dialog.accept()
        compare(sent, [{ action: "remove_layout_template", template: "Interview" }])
        compare(removals.pending, null)
    }

    // A removal its caller words, built when it is accepted (the moods).
    function test_a_worded_removal_builds_its_action_when_accepted() {
        const removals = createTemporaryObject(removalsComponent, test)
        let built = 0
        removals.askWith({
            kind: "mood", name: "Tavern",
            question: "Remove group \u201cTavern\u201d?", consequence: "", acceptText: "Remove group",
            removedText: "Group \u201cTavern\u201d removed",
            makeAction: () => { built++; return { action: "set_moods", moods: { groups: [] } } }
        })
        const dialog = dialogOf(removals)
        tryCompare(dialog, "opened", true)
        compare(dialog.title, "Remove group \u201cTavern\u201d?")
        compare(built, 0, "not yet")
        dialog.accept()
        compare(sent, [{ action: "set_moods", moods: { groups: [] } }])
        fakeCore.answered(JSON.stringify(sent[0]), "")
        fakeShow.restorable = { kind: "mood", name: "Tavern" }
        const toast = toastOf(removals)
        tryCompare(toast, "shown", true)
        compare(toast.text, "Group \u201cTavern\u201d removed")
        compare(toast.actionText, "Undo")
    }

    // As in the window: the toasts of MessageToasts, and a modal dialog
    // (the moods editor) the removal was made in.
    Component {
        id: windowComponent
        Item {
            id: inWindow
            anchors.fill: parent
            property alias removals: windowRemovals
            property alias modal: modalDialog
            MessageToasts {
                id: windowToasts
                anchors.fill: parent
                core: fakeCore
            }
            Removals {
                id: windowRemovals
                core: fakeCore
                act: action => test.sent.push(action)
                show: fakeShow
                column: windowToasts.column
            }
            KanteDialog {
                id: modalDialog
                parent: QQC2.Overlay.overlay
                modal: true
                x: 20
                y: 20
                width: 300
                height: 200
                standardButtons: QQC2.Dialog.Close
            }
        }
    }

    /** Removes a bus through `removals` until its toast shows. */
    function removedBus(removals, name) {
        removals.remove(removals.request("bus", { id: "b-" + name, name: name }))
        fakeCore.answered(JSON.stringify({ action: "remove_bus", bus: "b-" + name }), "")
        fakeShow.restorable = { kind: "bus", name: name }
        const toast = toastIn(removals)
        tryCompare(toast, "shown", true)
        // In from the right: click once it is there, laid out in the window.
        tryCompare(toast.transform[0], "x", 0)
        tryVerify(() => toast.mapToItem(null, toast.width, 0).x <= test.Window.width)
        return toast
    }
    function toastIn(removals) {
        for (let i = 0; i < removals.column.children.length; i++) {
            if (removals.column.children[i].restores !== undefined) {
                return removals.column.children[i]
            }
        }
        return null
    }
    function undoButton(item) {
        for (let i = 0; i < item.children.length; i++) {
            const child = item.children[i]
            if (child.text === "Undo" && child.clicked !== undefined) {
                return child
            }
            const found = undoButton(child)
            if (found) {
                return found
            }
        }
        return null
    }

    // Undo acts at once (rule 1), also from a modal dialog: the toast is
    // over its dimmed ground and takes the click; Ctrl+Z works there too.
    function test_undo_works_over_a_modal_dialog() {
        const win = createTemporaryObject(windowComponent, test.Window.contentItem)
        win.modal.open()
        tryCompare(win.modal, "opened", true)
        let toast = removedBus(win.removals, "Stream")
        mouseClick(undoButton(toast))
        compare(sent[sent.length - 1], { action: "restore" })
        verify(win.modal.opened, "the click is the toast's, the dialog stays")
        sent = []
        fakeShow.restorable = null
        toast = removedBus(win.removals, "In-Ear")
        keyClick(Qt.Key_Z, Qt.ControlModifier)
        compare(sent[sent.length - 1], { action: "restore" })
        verify(win.modal.opened)
        win.modal.close()
    }
}
