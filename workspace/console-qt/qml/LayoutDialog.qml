import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * A layout: background input and up to four boxes, each an input at a
 * preset place, optionally framed. New layouts get a name.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    /** Layout being edited; `null` adds one. Set before `open()`. */
    property var layout: null

    /** Inputs with a picture. */
    readonly property var pictures: inputs.filter(i => i.source.kind !== "audio_device")
    readonly property int boxCount: 4
    readonly property int frameWidth: 4

    /** Places a box can take, as fractions of the picture. */
    readonly property var places: [
        { label: qsTr("Top right quarter"), rect: { x: 0.5, y: 0, w: 0.5, h: 0.5 } },
        { label: qsTr("Top left quarter"), rect: { x: 0, y: 0, w: 0.5, h: 0.5 } },
        { label: qsTr("Bottom right quarter"), rect: { x: 0.5, y: 0.5, w: 0.5, h: 0.5 } },
        { label: qsTr("Bottom left quarter"), rect: { x: 0, y: 0.5, w: 0.5, h: 0.5 } },
        { label: qsTr("Left half"), rect: { x: 0, y: 0.25, w: 0.5, h: 0.5 } },
        { label: qsTr("Right half"), rect: { x: 0.5, y: 0.25, w: 0.5, h: 0.5 } },
        { label: qsTr("Small, top right"), rect: { x: 0.7, y: 0.05, w: 0.25, h: 0.25 } },
        { label: qsTr("Small, bottom right"), rect: { x: 0.7, y: 0.7, w: 0.25, h: 0.25 } },
        { label: qsTr("Small, bottom left"), rect: { x: 0.05, y: 0.7, w: 0.25, h: 0.25 } }
    ]

    function placeOf(rect) {
        var near = (a, b) => Math.abs(a - b) < 0.01
        var index = places.findIndex(p => near(p.rect.x, rect.x) && near(p.rect.y, rect.y) && near(p.rect.w, rect.w) && near(p.rect.h, rect.h))
        return index >= 0 ? index : 0
    }

    function pictureIndex(id) {
        return pictures.findIndex(i => i.id === id)
    }

    title: layout ? qsTr("Layout %1 %2").arg(layout.number).arg(layout.name) : qsTr("New layout")
    modal: true
    width: Kirigami.Units.gridUnit * 34
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: {
        nameField.text = ""
        // Combo index 0 is "none": inputs start at 1.
        background.currentIndex = layout && layout.background ? pictureIndex(layout.background) + 1 : 0
        for (var i = 0; i < boxCount; i++) {
            var row = boxRows.itemAt(i)
            var box = layout && layout.boxes[i] ? layout.boxes[i] : null
            row.inputIndex = box ? pictureIndex(box.input) + 1 : 0
            row.placeIndex = box ? placeOf(box.rect) : i
            row.framed = box ? box.border_px > 0 : false
        }
    }

    onAccepted: {
        var boxes = []
        for (var i = 0; i < boxCount; i++) {
            var row = boxRows.itemAt(i)
            if (row.inputIndex <= 0)
                continue
            boxes.push({
                rect: places[row.placeIndex].rect,
                input: pictures[row.inputIndex - 1].id,
                border_px: row.framed ? frameWidth : 0
            })
        }
        var bg = background.currentIndex > 0 ? pictures[background.currentIndex - 1].id : null
        if (layout)
            act({ action: "set_layout", layout: layout.number, background: bg, boxes: boxes })
        else
            act({ action: "add_layout", name: nameField.text.trim() || null, background: bg, boxes: boxes })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel {
            visible: dialog.layout === null
            text: qsTr("Name")
        }
        KanteTextField {
            id: nameField
            visible: dialog.layout === null
            Layout.fillWidth: true
            placeholderText: qsTr("optional")
        }

        KanteSectionLabel { text: qsTr("Background") }
        QQC2.ComboBox {
            id: background
            Layout.fillWidth: true
            model: [qsTr("Black")].concat(dialog.pictures.map(i => i.number + " " + i.name))
            KanteFieldSkin { control: background }
        }

        KanteSectionLabel { text: qsTr("Boxes (bottom to top)") }
        Repeater {
            id: boxRows
            model: dialog.boxCount

            RowLayout {
                id: boxRow
                required property int index
                property alias inputIndex: inputBox.currentIndex
                property alias placeIndex: placeBox.currentIndex
                property alias framed: frame.checked
                Layout.fillWidth: true

                QQC2.ComboBox {
                    id: inputBox
                    Layout.fillWidth: true
                    model: [qsTr("Empty")].concat(dialog.pictures.map(i => i.number + " " + i.name))
                    Accessible.name: qsTr("Input of box %1").arg(boxRow.index + 1)
                    KanteFieldSkin { control: inputBox }
                }
                QQC2.ComboBox {
                    id: placeBox
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 11
                    enabled: inputBox.currentIndex > 0
                    model: dialog.places.map(p => p.label)
                    Accessible.name: qsTr("Place of box %1").arg(boxRow.index + 1)
                    KanteFieldSkin { control: placeBox }
                }
                QQC2.CheckBox {
                    id: frame
                    text: qsTr("Frame")
                    enabled: inputBox.currentIndex > 0
                    KanteCheckSkin { control: frame }
                }
            }
        }
    }
}
