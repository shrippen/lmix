import QtQuick
import org.kde.kirigami as Kirigami
import Kante

/**
 * The window's toasts, at the bottom right, the newest at the bottom
 * (docs/DESIGN-DIALOGE.md, rule 2): every refusal and error of lmix-core
 * or the Console as a toast that stays until it is closed. The same text
 * twice shows once; at most `limit`, the oldest goes first. Pages put
 * their own toasts (Undo after a removal) into `column` too.
 */
Item {
    id: toasts

    required property var core
    property int limit: 4
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

    Connections {
        target: toasts.core
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

    Column {
        id: column
        anchors.right: parent.right
        anchors.bottom: parent.bottom
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
