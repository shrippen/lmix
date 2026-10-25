import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/** New output: stream, recording, NDI or screen, of program, preview, the multiview or one input. */
KanteDialog {
    id: dialog

    required property var core
    required property var inputs
    /** `buses` of the show state. */
    property var buses: []
    // The sound choices change only with the buses' names: every state
    // brings a new list, and a new model would reset the choice.
    property var soundNames: []
    function syncSoundNames() {
        const names = buses.map(b => b.name)
        if (JSON.stringify(names) !== JSON.stringify(soundNames)) {
            soundNames = names
        }
    }
    onBusesChanged: syncSoundNames()
    Component.onCompleted: syncSoundNames()

    readonly property var kinds: Texts.choices("outputs", core.choices("outputs"))
    readonly property var containers: Texts.choices("containers", core.choices("containers"))
    readonly property string kind: kinds[kindBox.currentIndex] ? kinds[kindBox.currentIndex].value : "srt"
    readonly property bool needsText: kind !== "window" && kind !== "fullscreen"
    readonly property bool encodes: kind === "srt" || kind === "rtmp" || kind === "record"
    /** Screens show picture only. */
    readonly property bool carriesSound: kind !== "window" && kind !== "fullscreen" && kind !== "drm"
    readonly property var hints: ({
        srt: "srt://host:9000",
        rtmp: "rtmp://a.rtmp.youtube.com/live2",
        record: qsTr("folder in the engine's media directory"),
        ndi: qsTr("NDI name, e.g. lmix PGM"),
        drm: qsTr("DRM connector id")
    })

    title: qsTr("Add output")
    modal: true
    anchors.centerIn: parent
    width: Kirigami.Units.gridUnit * 26
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: qsTr("Add output")

    onOpened: {
        textField.text = ""
        nameField.text = ""
        soundBox.currentIndex = 0
    }

    onAccepted: {
        // Program, Preview, Multiview, then the inputs.
        var source = sourceBox.currentIndex <= 0 ? "program"
                   : sourceBox.currentIndex === 1 ? "preview"
                   : sourceBox.currentIndex === 2 ? "multiview"
                   : dialog.inputs[sourceBox.currentIndex - 3].id
        var container = dialog.containers[containerBox.currentIndex] ? dialog.containers[containerBox.currentIndex].value : "mkv"
        core.addOutput(JSON.stringify({
            kind: dialog.kind,
            text: textField.text,
            source: source,
            container: container,
            video_kbps: kbps.value,
            key: keyField.text,
            encoding: {
                codec: codecBox.currentIndex === 1 ? "h265" : "h264",
                rate: rateBox.currentIndex === 1 ? "cbr" : "vbr",
                keyframe_ms: Math.round(keyframes.value * 1000)
            },
            name: nameField.text,
            bus: dialog.carriesSound && soundBox.currentIndex > 0 && dialog.buses[soundBox.currentIndex - 1] ? dialog.buses[soundBox.currentIndex - 1].id : ""
        }))
    }

    acceptEnabled: !dialog.needsText || textField.text.trim().length > 0

    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        KanteSectionLabel { text: qsTr("Output") }
        QQC2.ComboBox {
            id: kindBox
            Layout.fillWidth: true
            model: dialog.kinds
            textRole: "label"
            KanteFieldSkin { control: kindBox }
        }
        KanteTextField {
            id: textField
            visible: dialog.needsText
            Layout.fillWidth: true
            placeholderText: dialog.hints[dialog.kind] || ""
        }
        // STR.2: the platform's stream key, apart from the server.
        KanteTextField {
            id: keyField
            visible: dialog.kind === "rtmp"
            Layout.fillWidth: true
            placeholderText: qsTr("Stream key")
            echoMode: TextInput.Password
        }

        KanteSectionLabel { text: qsTr("Carries") }
        QQC2.ComboBox {
            id: sourceBox
            Layout.fillWidth: true
            model: [qsTr("Program"), qsTr("Preview"), qsTr("Multiview")].concat(dialog.inputs.map(i => i.number + " " + i.name))
            KanteFieldSkin { control: sourceBox }
        }

        KanteSectionLabel {
            visible: dialog.carriesSound
            text: qsTr("Sound")
        }
        QQC2.ComboBox {
            id: soundBox
            visible: dialog.carriesSound
            Layout.fillWidth: true
            // 0 keeps the source's own sound, then the buses.
            model: [qsTr("As carried")].concat(dialog.soundNames)
            KanteFieldSkin { control: soundBox }
        }

        RowLayout {
            visible: dialog.encodes
            KanteSectionLabel { text: qsTr("Video kb/s") }
            QQC2.SpinBox {
                id: kbps
                from: 500
                to: 100000
                stepSize: 500
                value: 6000
                editable: true
                KanteFieldSkin { control: kbps }
            }
            QQC2.ComboBox {
                id: codecBox
                model: ["H.264", "H.265 (HEVC)"]
                KanteFieldSkin { control: codecBox }
            }
            QQC2.ComboBox {
                id: containerBox
                visible: dialog.kind === "record"
                model: dialog.containers
                textRole: "label"
                KanteFieldSkin { control: containerBox }
            }
        }

        // STR.3: constant bitrate for platforms and fixed links, keyframes
        // every so many seconds (0: the encoder's choice).
        RowLayout {
            visible: dialog.encodes
            QQC2.ComboBox {
                id: rateBox
                model: [qsTr("Variable bitrate"), qsTr("Constant bitrate")]
                KanteFieldSkin { control: rateBox }
            }
            KanteSectionLabel { text: qsTr("Keyframe every (s)") }
            QQC2.SpinBox {
                id: keyframes
                from: 0
                to: 10
                value: 2
                editable: true
                KanteFieldSkin { control: keyframes }
            }
        }

        KanteSectionLabel { text: qsTr("Name") }
        KanteTextField {
            id: nameField
            Layout.fillWidth: true
            placeholderText: qsTr("optional")
        }
    }
}
