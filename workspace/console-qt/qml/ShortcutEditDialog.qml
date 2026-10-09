import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One shortcut: label, key (pressed, not typed), shown on the web page
 * "Keys", and up to eight actions run in order.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    required property var outputs
    /** Shortcut being edited; `null` adds one. Set before `open()`. */
    property var shortcut: null

    title: shortcut ? qsTr("Shortcut %1").arg(shortcut.label) : qsTr("New shortcut")
    modal: true
    width: Kirigami.Units.gridUnit * 38
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: {
        label.text = shortcut ? shortcut.label : ""
        keyButton.sequence = shortcut && shortcut.key ? shortcut.key : ""
        web.checked = shortcut ? shortcut.web : true
        actions.load(shortcut ? shortcut.actions : [])
    }

    Component.onCompleted: standardButton(QQC2.Dialog.Ok).enabled = Qt.binding(() => actions.error === "" && label.text.trim() !== "")

    onAccepted: {
        var spec = { label: label.text.trim(), key: keyButton.sequence || null, web: web.checked, actions: actions.collect() }
        if (shortcut)
            act({ action: "set_shortcut", target: shortcut.number, shortcut: spec })
        else
            act({ action: "add_shortcut", shortcut: spec })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            KanteTextField {
                id: label
                Layout.fillWidth: true
                placeholderText: qsTr("Label")
            }
            // Press a key to set it; Escape clears.
            KanteButton {
                id: keyButton
                property string sequence: ""
                property bool listening: false
                text: listening ? qsTr("Press a key…") : (sequence || qsTr("No key"))
                onClicked: {
                    listening = true
                    forceActiveFocus()
                }
                onActiveFocusChanged: if (!activeFocus) listening = false
                Keys.onPressed: event => {
                    if (!listening)
                        return
                    event.accepted = true
                    if (event.key === Qt.Key_Escape) {
                        sequence = ""
                        listening = false
                        return
                    }
                    var modifier = [Qt.Key_Control, Qt.Key_Shift, Qt.Key_Alt, Qt.Key_Meta].indexOf(event.key) >= 0
                    if (modifier)
                        return
                    var parts = []
                    if (event.modifiers & Qt.ControlModifier) parts.push("Ctrl")
                    if (event.modifiers & Qt.AltModifier) parts.push("Alt")
                    if (event.modifiers & Qt.ShiftModifier) parts.push("Shift")
                    var name = event.key >= Qt.Key_F1 && event.key <= Qt.Key_F35 ? "F" + (event.key - Qt.Key_F1 + 1)
                        : event.key === Qt.Key_Space ? "Space"
                        : event.key === Qt.Key_Return || event.key === Qt.Key_Enter ? "Return"
                        : event.text.toUpperCase()
                    if (!name)
                        return
                    parts.push(name)
                    sequence = parts.join("+")
                    listening = false
                }
            }
            QQC2.CheckBox {
                id: web
                text: qsTr("Web page")
                KanteCheckSkin { control: web }
            }
        }

        ActionRows {
            id: actions
            Layout.fillWidth: true
            inputs: dialog.inputs
            outputs: dialog.outputs
        }
    }
}
