import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * PTZ of an input's camera over VISCA (PER.2): its address, pan and tilt
 * while an arrow is held, zoom, focus, home, and presets 1–8 to go to or
 * to store. Moves go to the camera at once; the address is saved with the
 * show.
 */
KanteDialog {
    id: dialog

    required property var act
    /** lmix `Input`s of the show. */
    required property var inputs
    /** Id of the input whose camera moves; set before `open()`. */
    property string inputId: ""
    readonly property var input: StatePatch.byId(inputs, inputId) || null
    /** Store the next preset pressed instead of going there. */
    property bool storing: false

    function send(action) {
        dialog.act(Object.assign({ input: dialog.input.id }, action))
    }
    function move(pan, tilt) {
        send({ action: "ptz_move", pan: pan * speed.value, tilt: tilt * speed.value })
    }
    /** A stop, and once more shortly after: UDP may lose one, and a camera
     *  that missed it keeps moving. */
    property var lastStop: null
    function stop(action) {
        send(action)
        lastStop = action
        stopAgain.restart()
    }
    Timer {
        id: stopAgain
        interval: 150
        onTriggered: if (dialog.lastStop) dialog.send(dialog.lastStop)
    }
    // Closing the dialog while a key is held ends the move as well.
    onClosed: if (input && input.ptz) {
        stop({ action: "ptz_move", pan: 0, tilt: 0 })
        send({ action: "ptz_zoom", speed: 0 })
        send({ action: "ptz_focus", focus: "stop" })
    }

    title: input ? qsTr("PTZ of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 30
    standardButtons: QQC2.Dialog.Close
    contentHeight: column.implicitHeight

    onOpened: {
        storing = false
        address.text = input && input.ptz ? input.ptz.address : ""
        protocol.currentIndex = input && input.ptz && input.ptz.protocol === "visca_udp" ? 1 : 0
    }

    /** A key that moves while held and stops when let go. */
    component HoldKey: KanteButton {
        required property real pan
        required property real tilt
        property string hint
        Layout.preferredWidth: Kirigami.Units.gridUnit * 3
        Layout.preferredHeight: Kirigami.Units.gridUnit * 3
        enabled: dialog.input !== null && !!dialog.input.ptz
        Accessible.name: hint
        onPressed: dialog.move(pan, tilt)
        onReleased: dialog.stop({ action: "ptz_move", pan: 0, tilt: 0 })
        onCanceled: dialog.stop({ action: "ptz_move", pan: 0, tilt: 0 })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            KanteTextField {
                id: address
                Layout.fillWidth: true
                placeholderText: qsTr("Camera host:port, e.g. 192.168.1.20:52381")
            }
            QQC2.ComboBox {
                id: protocol
                model: [qsTr("VISCA over IP (Sony)"), qsTr("VISCA in UDP")]
                KanteFieldSkin { control: protocol }
            }
            KanteButton {
                text: qsTr("Set")
                onClicked: dialog.send({ action: "set_ptz", ptz: address.text.trim() === "" ? null
                    : { address: address.text.trim(), protocol: protocol.currentIndex === 1 ? "visca_udp" : "visca_ip" } })
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Kirigami.Units.largeSpacing

            GridLayout {
                columns: 3
                rowSpacing: 2
                columnSpacing: 2
                Item { width: 1; height: 1 }
                HoldKey { text: "▲"; pan: 0; tilt: 1; hint: qsTr("Up") }
                Item { width: 1; height: 1 }
                HoldKey { text: "◀"; pan: -1; tilt: 0; hint: qsTr("Left") }
                KanteButton {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 3
                    Layout.preferredHeight: Kirigami.Units.gridUnit * 3
                    text: "⌂"
                    enabled: dialog.input !== null && !!dialog.input.ptz
                    Accessible.name: qsTr("Home")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("Home")
                    onClicked: dialog.send({ action: "ptz_home" })
                }
                HoldKey { text: "▶"; pan: 1; tilt: 0; hint: qsTr("Right") }
                Item { width: 1; height: 1 }
                HoldKey { text: "▼"; pan: 0; tilt: -1; hint: qsTr("Down") }
                Item { width: 1; height: 1 }
            }

            ColumnLayout {
                Layout.fillWidth: true
                RowLayout {
                    QQC2.Label { text: qsTr("Speed") }
                    QQC2.Slider {
                        id: speed
                        Layout.fillWidth: true
                        from: 0.05
                        to: 1
                        value: 0.5
                        Accessible.name: qsTr("Speed")
                        KanteSliderSkin { control: speed }
                    }
                }
                RowLayout {
                    QQC2.Label { text: qsTr("Zoom") }
                    KanteButton {
                        text: "−"
                        enabled: dialog.input !== null && !!dialog.input.ptz
                        Accessible.name: qsTr("Zoom out")
                        onPressed: dialog.send({ action: "ptz_zoom", speed: -speed.value })
                        onReleased: dialog.stop({ action: "ptz_zoom", speed: 0 })
                        onCanceled: dialog.stop({ action: "ptz_zoom", speed: 0 })
                    }
                    KanteButton {
                        text: "+"
                        enabled: dialog.input !== null && !!dialog.input.ptz
                        Accessible.name: qsTr("Zoom in")
                        onPressed: dialog.send({ action: "ptz_zoom", speed: speed.value })
                        onReleased: dialog.stop({ action: "ptz_zoom", speed: 0 })
                        onCanceled: dialog.stop({ action: "ptz_zoom", speed: 0 })
                    }
                }
                RowLayout {
                    QQC2.Label { text: qsTr("Focus") }
                    KanteButton {
                        text: qsTr("Auto")
                        enabled: dialog.input !== null && !!dialog.input.ptz
                        onClicked: dialog.send({ action: "ptz_focus", focus: "auto" })
                    }
                    KanteButton {
                        text: qsTr("Near")
                        enabled: dialog.input !== null && !!dialog.input.ptz
                        onPressed: dialog.send({ action: "ptz_focus", focus: "near" })
                        onReleased: dialog.stop({ action: "ptz_focus", focus: "stop" })
                        onCanceled: dialog.stop({ action: "ptz_focus", focus: "stop" })
                    }
                    KanteButton {
                        text: qsTr("Far")
                        enabled: dialog.input !== null && !!dialog.input.ptz
                        onPressed: dialog.send({ action: "ptz_focus", focus: "far" })
                        onReleased: dialog.stop({ action: "ptz_focus", focus: "stop" })
                        onCanceled: dialog.stop({ action: "ptz_focus", focus: "stop" })
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            KanteSectionLabel { text: qsTr("Presets") }
            Repeater {
                model: 8
                KanteButton {
                    required property int index
                    text: String(index + 1)
                    emphasis: dialog.storing ? KanteButton.Emphasis.Destructive : KanteButton.Emphasis.Normal
                    enabled: dialog.input !== null && !!dialog.input.ptz
                    Accessible.name: dialog.storing ? qsTr("Store preset %1").arg(index + 1) : qsTr("Go to preset %1").arg(index + 1)
                    onClicked: {
                        dialog.send({ action: "ptz_preset", preset: index + 1, store: dialog.storing })
                        dialog.storing = false
                    }
                }
            }
            Item { Layout.fillWidth: true }
            KanteButton {
                text: dialog.storing ? qsTr("Pick a preset…") : qsTr("Store")
                enabled: dialog.input !== null && !!dialog.input.ptz
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Store where the camera is on the next preset pressed")
                onClicked: dialog.storing = !dialog.storing
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: KanteStyle.mutedTextColor
            text: qsTr("A preset as a trigger of this input (goes into preview) moves the camera before it is taken.")
        }
    }
}
