import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Picture size and rate of program (OUT.1). Program stops for a moment
 * while the engine builds its mixer anew; running outputs restart.
 */
KanteDialog {
    id: dialog

    required property var act
    /** `format` of the show state. */
    property var format: ({ width: 1920, height: 1080, framerate: "30/1" })

    readonly property var sizes: [
        { width: 3840, height: 2160, label: qsTr("3840 × 2160 (4K UHD)") },
        { width: 2560, height: 1440, label: qsTr("2560 × 1440") },
        { width: 1920, height: 1080, label: qsTr("1920 × 1080 (Full HD)") },
        { width: 1280, height: 720, label: qsTr("1280 × 720 (HD)") },
        { width: 720, height: 576, label: qsTr("720 × 576 (PAL SD)") },
        { width: 1080, height: 1920, label: qsTr("1080 × 1920 (upright)") },
        { width: 720, height: 1280, label: qsTr("720 × 1280 (upright)") }
    ]
    readonly property var rates: [
        { value: "24000/1001", label: "23.976" },
        { value: "24/1", label: "24" },
        { value: "25/1", label: "25" },
        { value: "30000/1001", label: "29.97" },
        { value: "30/1", label: "30" },
        { value: "50/1", label: "50" },
        { value: "60000/1001", label: "59.94" },
        { value: "60/1", label: "60" }
    ]

    title: qsTr("Program format")
    modal: true
    width: Kirigami.Units.gridUnit * 24
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    contentHeight: column.implicitHeight

    onOpened: {
        var size = sizes.findIndex(s => s.width === format.width && s.height === format.height)
        sizeBox.currentIndex = size >= 0 ? size : 2
        var rate = rates.findIndex(r => r.value === format.framerate)
        rateBox.currentIndex = rate >= 0 ? rate : 4
    }

    onAccepted: {
        var size = sizes[sizeBox.currentIndex]
        act({
            action: "set_master_format",
            format: { width: size.width, height: size.height, framerate: rates[rateBox.currentIndex].value }
        })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel { text: qsTr("Size") }
        QQC2.ComboBox {
            id: sizeBox
            Layout.fillWidth: true
            model: dialog.sizes
            textRole: "label"
            KanteFieldSkin { control: sizeBox }
        }
        KanteSectionLabel { text: qsTr("Frames per second") }
        QQC2.ComboBox {
            id: rateBox
            Layout.fillWidth: true
            model: dialog.rates
            textRole: "label"
            KanteFieldSkin { control: rateBox }
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: qsTr("Program stops for a moment while the mixer is built anew; running outputs restart.")
            color: KanteStyle.mutedTextColor
            wrapMode: Text.WordWrap
        }
    }
}
