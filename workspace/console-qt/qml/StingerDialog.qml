import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Stinger of a transition button: a clip (file, URL or playlist, ideally
 * with alpha) played over program, its length, and the cut point at which
 * program cuts to preview underneath.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    /** The transition button being set; set before `open()`. */
    property var button: null

    readonly property var clips: inputs.filter(i => ["file", "url", "playlist"].indexOf(i.source.kind) >= 0)

    title: button ? qsTr("Stinger on button %1").arg(button.slot) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 26
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: {
        var current = button && button.stinger ? button.stinger.input : null
        var index = clips.findIndex(i => i.id === current)
        clipBox.currentIndex = index >= 0 ? index : 0
        length.value = button && button.effect === "stinger" ? button.duration_ms : 2000
        cut.value = button && button.stinger ? button.stinger.cut_ms : 1000
    }

    onAccepted: act({
        action: "set_transition",
        slot: button.slot,
        effect: "stinger",
        duration_ms: length.value,
        stinger: { input: clips[clipBox.currentIndex].id, cut_ms: Math.min(cut.value, length.value) }
    })

    Component.onCompleted: standardButton(QQC2.Dialog.Ok).enabled = Qt.binding(() => dialog.clips.length > 0)

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel { text: qsTr("Clip") }
        QQC2.ComboBox {
            id: clipBox
            Layout.fillWidth: true
            visible: dialog.clips.length > 0
            model: dialog.clips.map(i => i.number + " " + i.name)
            KanteFieldSkin { control: clipBox }
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.clips.length === 0
            wrapMode: Text.WordWrap
            text: qsTr("Add the stinger clip as an input first (file, URL or playlist; WebM with alpha shows program around it).")
        }

        KanteSettingRow {
            Layout.fillWidth: true
            title: qsTr("Length")
            hint: qsTr("How long the clip plays")
            RowLayout {
                QQC2.SpinBox {
                    id: length
                    from: 100
                    to: 10000
                    stepSize: 100
                    editable: true
                    KanteFieldSkin { control: length }
                }
                QQC2.Label { text: "ms" }
            }
        }
        KanteSettingRow {
            Layout.fillWidth: true
            title: qsTr("Cut point")
            hint: qsTr("When program cuts underneath, best where the clip covers the picture")
            RowLayout {
                QQC2.SpinBox {
                    id: cut
                    from: 0
                    to: length.value
                    stepSize: 100
                    editable: true
                    KanteFieldSkin { control: cut }
                }
                QQC2.Label { text: "ms" }
            }
        }
    }
}
