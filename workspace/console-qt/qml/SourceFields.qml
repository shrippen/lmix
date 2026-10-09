import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import Kante

/**
 * Fields of a source: kind, then what that kind needs (pattern, colour,
 * path, address, device). `kind` and `value` are what Core.addInput and
 * Core.replaceSource take.
 */
ColumnLayout {
    id: fields

    required property var core

    readonly property var kinds: Texts.choices("sources", core.choices("sources"))
    readonly property var patterns: Texts.choices("patterns", core.choices("patterns"))
    readonly property var allDevices: core.devices.length > 0 ? JSON.parse(core.devices) : []

    readonly property string kind: kinds[kindBox.currentIndex] ? kinds[kindBox.currentIndex].value : "pattern"
    readonly property string value: {
        switch (kind) {
        case "pattern": return patterns[patternBox.currentIndex] ? patterns[patternBox.currentIndex].value : ""
        case "color": return colorDialog.selectedColor.toString()
        default: return textField.text
        }
    }
    readonly property bool complete: kind === "pattern" || kind === "color" || textField.text.trim().length > 0

    /** Device kind offered in the picker for this source kind ("" for none). */
    readonly property string deviceKind: kind === "device" ? "video" : (kind === "audio_device" ? "audio" : (kind === "ndi" ? "ndi" : ""))
    readonly property var deviceChoices: allDevices.filter(d => d.kind === deviceKind)
    readonly property bool picksFiles: kind === "file" || kind === "image" || kind === "playlist"

    readonly property var hints: ({
        file: "/path/to/clip.mp4",
        image: "/path/to/logo.png",
        playlist: "/clips/a.mp4; /clips/b.mp4",
        url: "https://…",
        srt: "srt://host:port",
        ndi: "HOST (Source name)",
        whip: qsTr("endpoint name, e.g. stage"),
        device: qsTr("device id"),
        audio_device: qsTr("device id")
    })

    function reset() {
        textField.text = ""
    }

    spacing: Kirigami.Units.smallSpacing

    QQC2.ComboBox {
        id: kindBox
        Layout.fillWidth: true
        model: fields.kinds
        textRole: "label"
        onActivated: textField.text = ""
        KanteFieldSkin { control: kindBox }
    }

    QQC2.ComboBox {
        id: patternBox
        Layout.fillWidth: true
        visible: fields.kind === "pattern"
        model: fields.patterns
        textRole: "label"
        KanteFieldSkin { control: patternBox }
    }

    RowLayout {
        visible: fields.kind === "color"
        KanteButton {
            text: qsTr("Pick colour…")
            onClicked: colorDialog.open()
        }
        Rectangle {
            // The colour the input will show: data, not decoration.
            implicitWidth: KanteStyle.heightMedium
            implicitHeight: KanteStyle.heightMedium
            color: colorDialog.selectedColor
            border.color: KanteStyle.frameColor
        }
        QQC2.Label {
            text: colorDialog.selectedColor.toString()
            font.family: KanteStyle.monoFamily
            color: KanteStyle.mutedTextColor
        }
    }

    RowLayout {
        visible: fields.kind !== "pattern" && fields.kind !== "color"
        Layout.fillWidth: true
        KanteTextField {
            id: textField
            Layout.fillWidth: true
            placeholderText: fields.hints[fields.kind] || ""
        }
        KanteButton {
            visible: fields.picksFiles
            text: "…"
            onClicked: fileDialog.open()
        }
    }

    QQC2.ComboBox {
        id: deviceBox
        Layout.fillWidth: true
        visible: fields.deviceKind !== ""
        enabled: fields.deviceChoices.length > 0
        model: fields.deviceChoices
        textRole: "name"
        displayText: fields.deviceChoices.length > 0 ? currentText : qsTr("No devices found")
        onActivated: index => textField.text = fields.deviceChoices[index].id
        KanteFieldSkin { control: deviceBox }
    }

    ColorDialog {
        id: colorDialog
        selectedColor: KanteStyle.accentColor
    }

    FileDialog {
        id: fileDialog
        fileMode: fields.kind === "playlist" ? FileDialog.OpenFiles : FileDialog.OpenFile
        onAccepted: {
            var paths = (fields.kind === "playlist" ? selectedFiles : [selectedFile])
                .map(u => decodeURIComponent(u.toString().replace(/^file:\/\//, "")))
            if (fields.kind === "playlist" && textField.text.trim().length > 0) {
                textField.text = textField.text.trim() + "; " + paths.join("; ")
            } else {
                textField.text = paths.join("; ")
            }
        }
    }
}
