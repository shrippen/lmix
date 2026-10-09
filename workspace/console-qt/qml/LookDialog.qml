import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Colour correction of an input (its look): brightness, contrast,
 * saturation, hue, gamma, temperature and tint. Changes go to lmix-core as
 * the whole look, at most every 100 ms.
 */
KanteDialog {
    id: dialog

    required property var act
    /** The input whose look is edited; set before `open()`. */
    property var input: null
    /** Working copy, sent as a whole. */
    property var look: null

    readonly property var neutral: ({ brightness: 0, contrast: 1, saturation: 1, hue: 0, gamma: 1, temperature: 0, tint: 0 })

    function edit(field, value) {
        var l = Object.assign({}, look)
        l[field] = value
        look = l
        send.restart()
    }

    title: input ? qsTr("Colour of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 30
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight
    onOpened: look = Object.assign({}, neutral, input.look || {})

    Timer {
        id: send
        interval: 100
        onTriggered: dialog.act({ action: "set_look", input: dialog.input.number, look: dialog.look })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: 0

        Repeater {
            model: [
                { field: "brightness", title: qsTr("Brightness"), from: -1, to: 1 },
                { field: "contrast", title: qsTr("Contrast"), from: 0, to: 2 },
                { field: "saturation", title: qsTr("Saturation"), from: 0, to: 2 },
                { field: "hue", title: qsTr("Hue"), from: -1, to: 1 },
                { field: "gamma", title: qsTr("Gamma"), from: 0.1, to: 3 },
                { field: "temperature", title: qsTr("Temperature"), hint: qsTr("Cooler to warmer"), from: -1, to: 1 },
                { field: "tint", title: qsTr("Tint"), hint: qsTr("Green to magenta"), from: -1, to: 1 }
            ]

            KanteSettingRow {
                id: row
                required property var modelData
                Layout.fillWidth: true
                title: modelData.title
                hint: modelData.hint || ""
                titleWidth: Kirigami.Units.gridUnit * 8

                RowLayout {
                    QQC2.Slider {
                        id: slider
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                        from: row.modelData.from
                        to: row.modelData.to
                        stepSize: 0.01
                        value: dialog.look ? dialog.look[row.modelData.field] : 0
                        Accessible.name: row.title
                        onMoved: dialog.edit(row.modelData.field, Math.round(value * 100) / 100)
                        KanteSliderSkin { control: slider }
                    }
                    QQC2.Label {
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 3
                        text: slider.value.toFixed(2)
                        font.family: KanteStyle.monoFamily
                    }
                }
            }
        }

        KanteButton {
            Layout.alignment: Qt.AlignRight
            Layout.topMargin: Kirigami.Units.smallSpacing
            text: qsTr("Reset")
            onClicked: {
                dialog.look = Object.assign({}, dialog.neutral)
                send.restart()
            }
        }
    }
}
