import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The show's shortcuts: key, label, how many actions; add, edit, remove,
 * try. They also appear on the web page "Keys" when marked so.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var shortcuts
    signal editRequested(var shortcut)

    title: qsTr("Shortcuts")
    modal: true
    width: Kirigami.Units.gridUnit * 32
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.shortcuts.length === 0
            wrapMode: Text.WordWrap
            text: qsTr("No shortcuts yet. A shortcut runs a list of actions from a key, the web page \"Keys\" or the API.")
        }

        Repeater {
            model: dialog.shortcuts

            RowLayout {
                id: entry
                required property var modelData
                Layout.fillWidth: true

                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    text: entry.modelData.key || "–"
                    font.family: KanteStyle.monoFamily
                }
                QQC2.Label {
                    Layout.fillWidth: true
                    text: entry.modelData.label
                    elide: Text.ElideRight
                }
                QQC2.Label {
                    text: qsTr("Actions: %1").arg(entry.modelData.actions.length)
                    color: KanteStyle.mutedTextColor
                }
                KanteButton {
                    text: qsTr("Run")
                    onClicked: dialog.act({ action: "run_shortcut", target: entry.modelData.number })
                }
                KanteButton {
                    text: qsTr("Edit…")
                    onClicked: dialog.editRequested(entry.modelData)
                }
                KanteButton {
                    text: qsTr("Remove")
                    onClicked: dialog.act({ action: "remove_shortcut", target: entry.modelData.number })
                }
            }
        }

        KanteButton {
            Layout.alignment: Qt.AlignRight
            text: qsTr("+ Shortcut")
            onClicked: dialog.editRequested(null)
        }
    }
}
