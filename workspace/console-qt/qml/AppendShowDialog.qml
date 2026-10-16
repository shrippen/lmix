import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Inputs from another show (SHW.3): its inputs, ticked to add them to
 * this show with their settings (triggers stay behind).
 */
KanteDialog {
    id: dialog

    required property var act
    required property string coreUrl
    /** Show file to take inputs from; set before `open()`. */
    property string path: ""
    /** [{ number, name, kind }] of that show. */
    property var inputs: []
    property var ticked: []
    property string error: ""

    title: qsTr("Inputs from %1").arg(path.split("/").pop())
    modal: true
    width: Kirigami.Units.gridUnit * 28
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    contentHeight: column.implicitHeight

    Component.onCompleted: standardButton(QQC2.Dialog.Ok).enabled = Qt.binding(() => dialog.ticked.length > 0)

    onOpened: {
        inputs = []
        ticked = []
        error = ""
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE)
                return
            try {
                const body = JSON.parse(request.responseText)
                if (request.status === 200) {
                    dialog.inputs = body
                    dialog.ticked = body.map(i => i.number)
                } else {
                    dialog.error = body.message || request.statusText
                }
            } catch (e) {
                dialog.error = request.statusText || String(e)
            }
        }
        request.open("GET", coreUrl + "/api/shows/inputs?path=" + encodeURIComponent(path))
        request.send()
    }

    onAccepted: act({ action: "append_show", path: path, inputs: ticked.slice().sort((a, b) => a - b) })

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteCallout {
            Layout.fillWidth: true
            visible: dialog.error !== ""
            kind: KanteCallout.Kind.Danger
            text: dialog.error
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.error === "" && dialog.inputs.length === 0
            text: qsTr("This show has no inputs.")
            color: KanteStyle.mutedTextColor
        }
        Repeater {
            model: dialog.inputs

            QQC2.CheckBox {
                id: box
                required property var modelData
                text: modelData.number + " " + modelData.name + "  (" + Texts.sourceBadge(modelData.kind) + ")"
                checked: dialog.ticked.indexOf(modelData.number) >= 0
                onToggled: dialog.ticked = checked ? dialog.ticked.concat([modelData.number])
                    : dialog.ticked.filter(n => n !== modelData.number)
                KanteCheckSkin { control: box }
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.inputs.length > 0
            wrapMode: Text.WordWrap
            color: KanteStyle.mutedTextColor
            text: qsTr("They are added at the end with their settings; their triggers stay behind (they name inputs of the other show).")
        }
    }
}
