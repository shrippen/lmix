import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * One input of the input bar: live picture, number and name, audio strip.
 * Program and preview are a bar in the tally colour plus the word, never
 * colour alone. Click: preview; double click: straight to program; right
 * click: everything else.
 */
KanteCard {
    id: tile

    required property var core
    required property var input
    required property var act
    property bool onProgram: false
    property bool onPreview: false
    property var peakDb: undefined
    signal replaceRequested()

    readonly property var audio: input.audio
    readonly property bool hasAudio: input.has_audio
    readonly property bool playable: ["file", "url", "playlist"].indexOf(input.source.kind) >= 0
    readonly property bool hasPicture: input.source.kind !== "audio_device"

    function ref() { return input.number }

    barColor: onProgram ? KanteStyle.negativeTextColor
        : (onPreview ? KanteStyle.positiveTextColor : "transparent")

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        anchors.topMargin: tile.barHeight + Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            LiveView {
                anchors.fill: parent
                visible: tile.hasPicture
                core: tile.core
                source: tile.hasPicture ? tile.input.id : ""
            }
            QQC2.Label {
                anchors.centerIn: parent
                visible: !tile.hasPicture
                text: qsTr("Audio only")
                color: KanteStyle.mutedTextColor
            }
            MouseArea {
                anchors.fill: parent
                acceptedButtons: Qt.LeftButton | Qt.RightButton
                onClicked: mouse => {
                    if (mouse.button === Qt.RightButton) {
                        menu.popup()
                    } else {
                        tile.act({ action: "preview_input", input: tile.ref() })
                    }
                }
                onDoubleClicked: mouse => {
                    if (mouse.button === Qt.LeftButton) {
                        tile.act({ action: "cut_direct", input: tile.ref() })
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            KanteSectionLabel {
                text: tile.input.number
                font.family: KanteStyle.monoFamily
            }
            KanteSectionLabel {
                Layout.fillWidth: true
                text: tile.input.name
                elide: Text.ElideRight
            }
            KanteSectionLabel {
                visible: tile.input.color !== null
                text: tile.input.color || ""
                color: KanteStyle.mutedTextColor
            }
            KanteSectionLabel {
                visible: tile.onProgram || tile.onPreview
                text: tile.onProgram ? qsTr("PGM") : qsTr("PRV")
                color: tile.barColor
            }
            KanteSectionLabel {
                visible: tile.input.status !== "online"
                text: tile.input.status === "offline" ? qsTr("offline") : qsTr("starting")
                color: tile.input.status === "offline" ? KanteStyle.negativeTextColor : KanteStyle.mutedTextColor
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: tile.hasAudio
            spacing: Kirigami.Units.smallSpacing

            KanteToolButton {
                icon.name: tile.audio.on ? "audio-volume-high" : "audio-volume-muted"
                checkable: true
                checked: !tile.audio.on
                display: QQC2.AbstractButton.IconOnly
                text: tile.audio.on ? qsTr("Mute") : qsTr("Unmute")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: text
                onClicked: tile.act({ action: "set_audio", input: tile.ref(), on: !tile.audio.on })
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 0
                LevelMeter {
                    Layout.fillWidth: true
                    peakDb: tile.peakDb
                }
                QQC2.Slider {
                    id: volume
                    Layout.fillWidth: true
                    from: 0
                    to: 100
                    stepSize: 1
                    value: tile.audio.volume
                    onMoved: tile.act({ action: "set_volume", input: tile.ref(), volume: Math.round(value) })
                    KanteSliderSkin { control: volume }
                    QQC2.ToolTip.visible: hovered || pressed
                    QQC2.ToolTip.text: qsTr("Volume %1").arg(Math.round(value))
                }
            }
            KanteToolButton {
                text: tile.input.listen === "afl" ? "AFL" : "PFL"
                checkable: true
                checked: tile.input.listen !== "off"
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.text: qsTr("Listen on the monitor bus (right click: before or after the fader)")
                onClicked: tile.act({ action: "set_listen", input: tile.ref(), listen: tile.input.listen === "off" ? "pfl" : "off" })
            }
        }
    }

    QQC2.Menu {
        id: menu
        KantePopupSkin { popup: menu }

        QQC2.MenuItem {
            text: qsTr("Rename…")
            onTriggered: renameDialog.open()
        }
        QQC2.MenuItem {
            text: qsTr("Replace source…")
            onTriggered: tile.replaceRequested()
        }
        ChoiceMenu {
            title: qsTr("Colour tag")
            choices: [{ value: null, label: qsTr("None") }].concat(JSON.parse(tile.core.choices("colors")))
            current: tile.input.color
            onPicked: value => tile.act({ action: "set_input_color", input: tile.ref(), color: value })
        }
        QQC2.Menu {
            id: lossMenu
            title: qsTr("When the signal is lost")
            KantePopupSkin { popup: lossMenu }
            QQC2.MenuItem {
                text: qsTr("Hold last frame")
                checkable: true
                checked: tile.input.signal_loss === "hold"
                onTriggered: tile.act({ action: "set_signal_loss", input: tile.ref(), mode: "hold" })
            }
            QQC2.MenuItem {
                text: qsTr("Black")
                checkable: true
                checked: tile.input.signal_loss === "black"
                onTriggered: tile.act({ action: "set_signal_loss", input: tile.ref(), mode: "black" })
            }
        }
        QQC2.Menu {
            id: audioMenu
            title: qsTr("Audio")
            enabled: tile.hasAudio
            KantePopupSkin { popup: audioMenu }
            QQC2.MenuItem {
                text: qsTr("Audio follows video")
                checkable: true
                checked: tile.audio.follow_video
                onTriggered: tile.act({ action: "set_audio_follow", input: tile.ref(), follow: !tile.audio.follow_video })
            }
            QQC2.MenuItem {
                text: qsTr("Listen before the fader (PFL)")
                checkable: true
                checked: tile.input.listen === "pfl"
                onTriggered: tile.act({ action: "set_listen", input: tile.ref(), listen: checked ? "pfl" : "off" })
            }
            QQC2.MenuItem {
                text: qsTr("Listen after the fader (AFL)")
                checkable: true
                checked: tile.input.listen === "afl"
                onTriggered: tile.act({ action: "set_listen", input: tile.ref(), listen: checked ? "afl" : "off" })
            }
        }
        ChoiceMenu {
            title: qsTr("Playback")
            enabled: tile.playable
            choices: [
                { value: "play", label: qsTr("Play") },
                { value: "pause", label: qsTr("Pause") },
                { value: "restart", label: qsTr("Restart") },
                { value: "previous", label: qsTr("Previous") },
                { value: "next", label: qsTr("Next") }
            ]
            onPicked: value => tile.act({ action: "transport_input", input: tile.ref(), transport: value })
        }
        QQC2.MenuSeparator {}
        QQC2.MenuItem {
            text: qsTr("Remove")
            onTriggered: tile.act({ action: "remove_input", input: tile.ref() })
        }
    }

    KanteDialog {
        id: renameDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: qsTr("Rename input %1").arg(tile.input.number)
        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
        onOpened: {
            nameField.text = tile.input.name
            nameField.forceActiveFocus()
            nameField.selectAll()
        }
        onAccepted: {
            if (nameField.text.trim().length > 0) {
                tile.act({ action: "rename_input", input: tile.ref(), name: nameField.text.trim() })
            }
        }
        KanteTextField {
            id: nameField
            width: Kirigami.Units.gridUnit * 18
            onAccepted: renameDialog.accept()
        }
    }
}
