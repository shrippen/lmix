import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/** New output: stream, recording, NDI or screen, of program or one input. */
KanteDialog {
    id: dialog

    required property var core
    required property var inputs

    readonly property var kinds: Texts.choices("outputs", core.choices("outputs"))
    readonly property var containers: Texts.choices("containers", core.choices("containers"))
    readonly property string kind: kinds[kindBox.currentIndex] ? kinds[kindBox.currentIndex].value : "srt"
    readonly property bool needsText: kind !== "window" && kind !== "fullscreen"
    readonly property bool encodes: kind === "srt" || kind === "record"
    /** Screens show picture only. */
    readonly property bool carriesSound: kind !== "window" && kind !== "fullscreen" && kind !== "drm"
    readonly property var hints: ({
        srt: "srt://host:9000",
        record: qsTr("folder in the engine's media directory"),
        ndi: qsTr("NDI name, e.g. lmix PGM"),
        drm: qsTr("DRM connector id")
    })

    title: qsTr("Add output")
    modal: true
    anchors.centerIn: parent
    width: Kirigami.Units.gridUnit * 26
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel

    onOpened: {
        textField.text = ""
        nameField.text = ""
        soundBox.currentIndex = 0
    }

    onAccepted: {
        var source = sourceBox.currentIndex <= 0 ? "program" : dialog.inputs[sourceBox.currentIndex - 1].id
        var container = dialog.containers[containerBox.currentIndex] ? dialog.containers[containerBox.currentIndex].value : "mkv"
        core.addOutput(JSON.stringify({
            kind: dialog.kind,
            text: textField.text,
            source: source,
            container: container,
            video_kbps: kbps.value,
            name: nameField.text,
            aux: dialog.carriesSound ? soundBox.currentIndex : 0
        }))
    }

    Component.onCompleted: standardButton(QQC2.Dialog.Ok).enabled = Qt.binding(() => !dialog.needsText || textField.text.trim().length > 0)

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

        KanteSectionLabel { text: qsTr("Carries") }
        QQC2.ComboBox {
            id: sourceBox
            Layout.fillWidth: true
            model: [qsTr("Program")].concat(dialog.inputs.map(i => i.number + " " + i.name))
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
            // Index = aux bus number; 0 keeps the source's own sound.
            model: [qsTr("As carried"), qsTr("Aux 1 (before the faders)"), qsTr("Aux 2 (after the faders)")]
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
                id: containerBox
                visible: dialog.kind === "record"
                model: dialog.containers
                textRole: "label"
                KanteFieldSkin { control: containerBox }
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
