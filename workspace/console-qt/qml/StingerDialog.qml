import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Stinger of a transition button: a clip (file, URL or the first item of a
 * playlist, ideally with alpha) that the engine plays to its end over
 * program, cutting to preview underneath at the cut point. The length
 * holds switching back while the clip's own length is not known.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    /** The transition button being set; set before `open()`. */
    property var button: null

    readonly property var clips: inputs.filter(i => ["file", "url", "playlist"].indexOf(i.source.kind) >= 0)
    /** Length of the chosen clip as its player reports it (ms), or 0. */
    readonly property int clipLength: clips.length > 0 && clipBox.currentIndex >= 0 && clips[clipBox.currentIndex]
        ? Math.min(clips[clipBox.currentIndex].clip_ms || 0, length.to) : 0

    /** The clip's own length, to the next 100 ms. */
    function useClipLength() {
        if (clipLength > 0)
            length.value = Math.round(clipLength / 100) * 100
    }

    title: button ? qsTr("Stinger on button %1").arg(button.slot) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 26
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: qsTr("Apply")
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: {
        var current = button && button.stinger ? button.stinger.input : null
        var index = clips.findIndex(i => i.id === current)
        clipBox.currentIndex = index >= 0 ? index : 0
        length.value = button && button.effect === "stinger" ? button.duration_ms : 2000
        if (!(button && button.effect === "stinger"))
            useClipLength()
        cut.value = button && button.stinger ? button.stinger.cut_ms : 1000
    }

    onAccepted: act({
        action: "set_transition",
        slot: button.slot,
        effect: "stinger",
        duration_ms: length.value,
        stinger: { input: clips[clipBox.currentIndex].id, cut_ms: Math.min(cut.value, length.value) }
    })

    acceptEnabled: dialog.clips.length > 0

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
            onActivated: dialog.useClipLength()
            KanteFieldSkin { control: clipBox }
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.clips.length === 0
            wrapMode: Text.WordWrap
            text: qsTr("Add the stinger clip as an input first (file, URL or playlist, which plays its first item; WebM with alpha shows program around it).")
        }

        KanteSettingRow {
            Layout.fillWidth: true
            title: qsTr("Length")
            hint: qsTr("Until the player reports the clip's length; the clip always plays to its end")
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
                KanteButton {
                    visible: dialog.clipLength > 0
                    text: qsTr("Clip length (%1 s)").arg((dialog.clipLength / 1000).toFixed(1))
                    onClicked: dialog.useClipLength()
                }
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
        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: qsTr("Stingers run between inputs only, not with layouts.")
        }
    }
}
