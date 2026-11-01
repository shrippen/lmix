import QtQuick
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The window's toasts, at the bottom right, the newest at the bottom
 * (docs/DESIGN-DIALOGE.md, rule 2): every refusal and error of lmix-core
 * or the Console as a toast that stays until it is closed. The same text
 * twice shows once; at most `limit`, the oldest goes first. Pages put
 * their own toasts (Undo after a removal) into `column` too.
 *
 * The toasts are in the window's content, under its menus and lists and
 * in its Tab order. While a modal dialog dims the window they move into a
 * popup of their own above it (not modal, no focus): a refusal of an edit
 * in the dialog, or the Undo of a removal made there, shows over the
 * dimmed ground and takes the click (docs/DESIGN-DIALOGE.md rule 1: Undo
 * acts at once).
 */
Item {
    id: toasts

    required property var core
    property int limit: 4
    /** Posts lmix-core's refusals and the Console's errors: with pages in
     *  windows of their own, only the window the operator works in does
     *  (AppShell.operatorWindow), so a message shows once. */
    property bool listening: true
    /** Where toasts stack; a page's own toast sets this as its parent. */
    readonly property alias column: column

    implicitWidth: column.implicitWidth
    implicitHeight: column.implicitHeight

    /** Show `text` as an error toast (no-op when empty or already shown). */
    function post(text) {
        if (text.length === 0) {
            return
        }
        for (let i = 0; i < errors.count; i++) {
            if (errors.get(i).text === text) {
                return
            }
        }
        while (errors.count >= limit) {
            errors.remove(0)
        }
        errors.append({ text: text })
    }

    ListModel {
        id: errors
    }
    // A message from before (the Console's own, before the window was there).
    Component.onCompleted: if (listening) post(Texts.message(core.message))

    Connections {
        target: toasts.core
        enabled: toasts.listening
        // One answer per action, also for the same refusal twice: closed,
        // it comes back when the action is refused again.
        function onAnswered(action, refusal) {
            toasts.post(Texts.message(refusal))
        }
        // Errors of the Console itself (a live picture that fails).
        function onMessageChanged() {
            toasts.post(Texts.message(toasts.core.message))
        }
    }

    /** Modal dialogs dim the window now: each has its dimmed ground in the
     *  overlay, the only items there besides the popups' own. */
    readonly property int dimmed: {
        const overlay = QQC2.Overlay.overlay
        let n = 0
        for (const item of overlay ? overlay.children : []) {
            if (item.visible && !String(item).startsWith("QQuickPopupItem")) {
                n++
            }
        }
        return n
    }
    /** The toasts are raised above the dialogs. */
    readonly property bool raised: dimmed > 0
    // Opened again for each dialog, after it has opened: of popups at the
    // same height the one opened last is on top, so a dialog's own lists
    // and menus opened later still cover the toasts.
    onDimmedChanged: Qt.callLater(() => {
        layer.close()
        if (toasts.raised) {
            layer.open()
        }
    })

    // Only as large as the toasts: it takes no clicks beside them and never
    // the keyboard focus.
    QQC2.Popup {
        id: layer
        objectName: "toastLayer"
        parent: toasts
        width: column.implicitWidth
        height: column.implicitHeight
        x: toasts.width - width
        y: toasts.height - height
        modal: false
        dim: false
        focus: false
        closePolicy: QQC2.Popup.NoAutoClose
        padding: 0
        background: null
        enter: null
        exit: null
        contentItem: Item {
            id: holder
        }
    }

    Column {
        id: column
        // In the content, or above the dialogs (see above).
        parent: toasts.raised ? holder : toasts
        x: parent ? parent.width - width : 0
        y: parent ? parent.height - height : 0
        spacing: Kirigami.Units.smallSpacing

        Repeater {
            model: errors
            KanteToast {
                id: toast
                required property int index
                required text
                width: Kirigami.Units.gridUnit * 20
                kind: KanteToast.Kind.Danger
                life: 0
                actionText: qsTr("Close")
                onClosed: errors.remove(index)
                Component.onCompleted: show()
                Accessible.role: Accessible.AlertMessage
                Accessible.name: toast.text
            }
        }
    }
}
