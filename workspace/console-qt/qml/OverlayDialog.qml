import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Settings of one overlay channel (OVL.2): its effect (cut or fade; going
 * off runs it backwards) and length, an auto off after some seconds, and
 * full screen or picture in picture at a place (preset or in per cent of
 * the picture; the input keeps its aspect inside it).
 */
KanteDialog {
    id: dialog

    required property var act
    /** Channel 1–8 and its settings (`overlay_channels` of the state); set
     *  before `open()`. */
    property int channel: 1
    property var settings: null

    readonly property var effects: [
        { value: "fade", label: Texts.label("effects", "fade") },
        { value: "cut", label: Texts.label("effects", "cut") }
    ]
    /** Places a picture-in-picture overlay can take, as fractions of the picture. */
    readonly property var places: [
        { label: qsTr("Small, top right"), rect: { x: 0.7, y: 0.05, w: 0.25, h: 0.25 } },
        { label: qsTr("Small, top left"), rect: { x: 0.05, y: 0.05, w: 0.25, h: 0.25 } },
        { label: qsTr("Small, bottom right"), rect: { x: 0.7, y: 0.7, w: 0.25, h: 0.25 } },
        { label: qsTr("Small, bottom left"), rect: { x: 0.05, y: 0.7, w: 0.25, h: 0.25 } },
        { label: qsTr("Top right quarter"), rect: { x: 0.5, y: 0, w: 0.5, h: 0.5 } },
        { label: qsTr("Bottom right quarter"), rect: { x: 0.5, y: 0.5, w: 0.5, h: 0.5 } },
        { label: qsTr("Lower third"), rect: { x: 0, y: 0.67, w: 1, h: 0.33 } }
    ]

    /** The rectangle from the four fields, in fractions. */
    function rect() {
        return { x: xField.value / 100, y: yField.value / 100, w: wField.value / 100, h: hField.value / 100 }
    }
    function setRect(r) {
        xField.value = Math.round(r.x * 100)
        yField.value = Math.round(r.y * 100)
        wField.value = Math.round(r.w * 100)
        hField.value = Math.round(r.h * 100)
    }
    readonly property bool rectInside: xField.value + wField.value <= 100 && yField.value + hField.value <= 100

    title: qsTr("Overlay %1").arg(channel)
    modal: true
    width: Kirigami.Units.gridUnit * 28
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: qsTr("Apply")
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    onOpened: {
        var s = settings || { effect: "fade", duration_ms: 500 }
        effectBox.currentIndex = Math.max(0, effects.findIndex(e => e.value === s.effect))
        length.value = s.duration_ms
        autoOff.checked = !!s.auto_off_ms
        seconds.value = s.auto_off_ms ? Math.round(s.auto_off_ms / 1000) : 10
        pip.checked = !!s.rect
        setRect(s.rect || places[0].rect)
    }

    onAccepted: act({
        action: "set_overlay",
        channel: channel,
        effect: effects[effectBox.currentIndex].value,
        duration_ms: length.value,
        auto_off_ms: autoOff.checked ? seconds.value * 1000 : null,
        rect: pip.checked ? rect() : null
    })

    acceptEnabled: !pip.checked || dialog.rectInside

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSettingRow {
            Layout.fillWidth: true
            title: qsTr("Effect")
            hint: qsTr("Used when an action gives no length of its own; going off runs it backwards")
            RowLayout {
                QQC2.ComboBox {
                    id: effectBox
                    model: dialog.effects.map(e => e.label)
                    KanteFieldSkin { control: effectBox }
                }
                QQC2.SpinBox {
                    id: length
                    enabled: dialog.effects[effectBox.currentIndex].value !== "cut"
                    from: 0
                    to: 10000
                    stepSize: 100
                    editable: true
                    Accessible.name: qsTr("Length")
                    KanteFieldSkin { control: length }
                }
                QQC2.Label { text: "ms" }
            }
        }

        KanteSettingRow {
            Layout.fillWidth: true
            title: qsTr("Auto off")
            hint: qsTr("Goes off by itself this long after it went on")
            RowLayout {
                QQC2.CheckBox {
                    id: autoOff
                    text: qsTr("After")
                    KanteCheckSkin { control: autoOff }
                }
                QQC2.SpinBox {
                    id: seconds
                    enabled: autoOff.checked
                    from: 1
                    to: 3600
                    editable: true
                    Accessible.name: qsTr("Seconds until off")
                    KanteFieldSkin { control: seconds }
                }
                QQC2.Label { text: qsTr("s") }
            }
        }

        KanteSettingRow {
            Layout.fillWidth: true
            title: qsTr("Picture in picture")
            hint: qsTr("Off: the overlay fills the picture")
            QQC2.CheckBox {
                id: pip
                text: qsTr("In a box")
                KanteCheckSkin { control: pip }
            }
        }

        KanteSettingRow {
            Layout.fillWidth: true
            visible: pip.checked
            title: qsTr("Place")
            QQC2.ComboBox {
                id: placeBox
                model: dialog.places.map(p => p.label)
                displayText: qsTr("Choose…")
                onActivated: index => dialog.setRect(dialog.places[index].rect)
                KanteFieldSkin { control: placeBox }
            }
        }

        // Left, top, width, height in per cent of the picture.
        GridLayout {
            Layout.fillWidth: true
            visible: pip.checked
            columns: 4
            columnSpacing: Kirigami.Units.smallSpacing

            Repeater {
                model: [qsTr("Left %"), qsTr("Top %"), qsTr("Width %"), qsTr("Height %")]
                QQC2.Label {
                    required property string modelData
                    text: modelData
                    color: KanteStyle.mutedTextColor
                }
            }
            QQC2.SpinBox {
                id: xField
                from: 0; to: 95; editable: true
                Accessible.name: qsTr("Left %")
                KanteFieldSkin { control: xField; invalid: !dialog.rectInside }
            }
            QQC2.SpinBox {
                id: yField
                from: 0; to: 95; editable: true
                Accessible.name: qsTr("Top %")
                KanteFieldSkin { control: yField; invalid: !dialog.rectInside }
            }
            QQC2.SpinBox {
                id: wField
                from: 5; to: 100; editable: true
                Accessible.name: qsTr("Width %")
                KanteFieldSkin { control: wField; invalid: !dialog.rectInside }
            }
            QQC2.SpinBox {
                id: hField
                from: 5; to: 100; editable: true
                Accessible.name: qsTr("Height %")
                KanteFieldSkin { control: hField; invalid: !dialog.rectInside }
            }
        }
        QQC2.Label {
            Layout.fillWidth: true
            visible: pip.checked && !dialog.rectInside
            wrapMode: Text.WordWrap
            color: KanteStyle.negativeTextColor
            text: qsTr("The box reaches outside the picture.")
        }
    }
}
