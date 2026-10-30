import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Removing, for every page (docs/DESIGN-DIALOGE.md rule 1): a question
 * first, red, the button naming verb and object, Cancel on Enter; then a
 * toast whose Undo (or Ctrl+Z while it shows) brings it back (`restore`,
 * AUT.10). lmix-core brings back the newest removal, so one toast: a new
 * removal takes it over.
 *
 * The toast waits for the core's answer and for the state that names the
 * removal as the one to bring back: a refused removal shows nothing, and
 * one from elsewhere does not pass for this one.
 *
 * `ask(kind, item)` takes an lmix `Input`, `Layout`, `Bus` or `Output` of
 * the state, by its kind ("input", "layout", "bus", "output"), or a layout
 * template ("template", asked without a way back). The removal names it
 * by id: its number may move while the question is open.
 *
 * `change(output, actions)` sends an output's new settings (Settings…,
 * `update_output` first) without a question; its toast brings back the
 * settings before, which lmix-core keeps on the same stack.
 */
Item {
    id: removals

    required property var core
    required property var act
    /** ShowState of the window, or null. */
    required property var show
    /** Where the toast stacks (MessageToasts.column). */
    required property Item column
    /** Ctrl+Z is not a show's own key. */
    property bool undoKeyFree: true

    /** What removing `item` of `kind` asks and says. */
    function request(kind, item) {
        const named = "\u201c" + item.name + "\u201d"
        return Object.assign({ kind: kind, name: item.name }, texts(kind, item, named))
    }
    function texts(kind, item, named) {
        switch (kind) {
        case "input": return {
            action: { action: "remove_input", input: item.id },
            question: qsTr("Remove input %1 %2?").arg(item.number).arg(named),
            consequence: qsTr("Its picture leaves program, preview, the layouts and the overlays."),
            acceptText: qsTr("Remove input"),
            removedText: qsTr("Input %1 %2 removed").arg(item.number).arg(named)
        }
        case "layout": return {
            action: { action: "remove_layout", layout: item.id },
            question: qsTr("Remove layout %1?").arg(named),
            consequence: qsTr("Its boxes and their places go with it."),
            acceptText: qsTr("Remove layout"),
            removedText: qsTr("Layout %1 removed").arg(named)
        }
        case "bus": return {
            action: { action: "remove_bus", bus: item.id },
            question: qsTr("Remove bus %1?").arg(named),
            consequence: qsTr("Every input's send to it goes with it."),
            acceptText: qsTr("Remove bus"),
            removedText: qsTr("Bus %1 removed").arg(named)
        }
        // Not kept for Undo (rule 1 names templates too; they follow): the
        // question says so.
        case "template": return {
            action: { action: "remove_layout_template", template: item.name },
            question: qsTr("Remove template %1?").arg(named),
            consequence: qsTr("Layouts made from it stay. The template cannot be brought back."),
            acceptText: qsTr("Remove template"),
            undo: false
        }
        case "output": return {
            action: { action: "remove_output", output: item.id },
            question: qsTr("Remove output %1?").arg(named),
            consequence: qsTr("Its address and settings go with it."),
            acceptText: qsTr("Remove output"),
            removedText: qsTr("Output %1 removed").arg(named)
        }
        }
        return null
    }

    /** Ask, then remove (see above). */
    function ask(kind, item) {
        dialog.request = request(kind, item)
        dialog.open()
    }
    /** Remove now, with the toast to bring it back. */
    function remove(asked) {
        pending = asked.undo === false ? null : { request: asked, answered: false }
        removals.act(asked.action)
    }
    /** New settings for `output` (the state's, before): the toast brings
     *  them back. The core names the settings by the name they had. */
    function change(output, actions) {
        const named = "\u201c" + output.name + "\u201d"
        pending = { request: {
            kind: "output_settings",
            name: output.name,
            action: actions[0],
            title: qsTr("Changed"),
            removedText: qsTr("Settings of output %1 changed").arg(named)
        }, answered: false }
        actions.forEach(a => removals.act(a))
    }

    /** What made `settle` look again. */
    enum Came {
        Answer,
        State
    }

    property var pending: null
    readonly property var restorable: show ? show.restorable : null
    onRestorableChanged: settle(Removals.Came.State)

    /** The removal went through and is the one to bring back: the toast.
     *  The answer may come before or after the state that names it; a
     *  state that names something else after the answer (a removal from
     *  elsewhere in between) ends the wait. */
    function settle(came) {
        if (!pending || !pending.answered) {
            return
        }
        const request = pending.request
        if (restorable && restorable.kind === request.kind && restorable.name === request.name) {
            toast.text = request.removedText
            toast.title = request.title || qsTr("Removed")
            toast.removedKind = request.kind
            toast.restores = request.name
            toast.show()
            pending = null
        } else if (came === Removals.Came.State) {
            pending = null
        }
    }
    function sameAction(answered, sent) {
        return answered.action === sent.action
            && Object.keys(sent).every(key => String(answered[key]) === String(sent[key]))
    }

    Connections {
        target: removals.core
        function onAnswered(action, refusal) {
            if (!removals.pending || !removals.sameAction(JSON.parse(action), removals.pending.request.action)) {
                return
            }
            if (refusal.length > 0) {
                removals.pending = null
                return
            }
            removals.pending.answered = true
            // The state may have come first.
            removals.settle(Removals.Came.Answer)
        }
    }

    KanteDialog {
        id: dialog
        property var request: null
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: request ? request.question : ""
        destructive: true
        size: KanteDialog.Size.Small
        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
        acceptText: request ? request.acceptText : ""
        // KanteDialog has a child of its own, so it does not size to this.
        contentHeight: consequence.implicitHeight
        onOpened: consequence.forceActiveFocus()
        onAccepted: removals.remove(request)

        QQC2.Label {
            id: consequence
            width: parent.width
            text: dialog.request ? dialog.request.consequence : ""
            wrapMode: Text.WordWrap
            visible: text.length > 0
            // Enter keeps it: removing takes the button.
            focus: true
            Keys.onReturnPressed: dialog.reject()
            Keys.onEnterPressed: dialog.reject()
        }
    }

    KanteToast {
        id: toast
        parent: removals.column
        width: Kirigami.Units.gridUnit * 20
        kind: KanteToast.Kind.Info
        life: 8000
        /** What it brings back: only that one, not a removal since. */
        property string removedKind
        property string restores
        readonly property bool restorable: removals.restorable !== null
            && removals.restorable.kind === removedKind && removals.restorable.name === restores
        actionText: restorable ? qsTr("Undo") : ""
        actionShortcut: removals.undoKeyFree && restorable ? qsTr("Ctrl+Z") : ""
        onActionTriggered: removals.act({ action: "restore" })
    }
    // A show's own shortcut on Ctrl+Z wins, as for the File keys: two
    // shortcuts on one key would both stay silent.
    Shortcut {
        sequence: "Ctrl+Z"
        enabled: toast.shown && toast.restorable && removals.undoKeyFree
        onActivated: {
            toast.hide()
            removals.act({ action: "restore" })
        }
    }
}
