import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * An input's settings in tabs (UI.8): general (name, colour tag, signal
 * loss, source), colour, audio (channel strip and aux sends), triggers,
 * PTZ and advanced (id, slot, links). Colour and audio are the panels of
 * their own dialogs; triggers, PTZ controls and a new source open those.
 * Changes go to lmix-core as they are made.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var core
    /** lmix `Input`s of the show. */
    required property var inputs
    /** Looks need the GL mixer. */
    property bool looksAvailable: false
    /** Id of the input; set before `open()`. */
    property string inputId: ""
    readonly property var input: inputs.find(i => i.id === inputId) || null

    signal replaceRequested(var input)
    signal triggersRequested(var input)
    signal ptzRequested(var input)

    readonly property var tabs: [qsTr("General"), qsTr("Colour"), qsTr("Audio"), qsTr("Triggers"), qsTr("PTZ"), qsTr("Advanced")]
    property int tab: 0

    function ref() { return dialog.input.id }

    title: input ? qsTr("Settings of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 36
    standardButtons: QQC2.Dialog.Close
    contentHeight: Math.min(column.implicitHeight, (parent ? parent.height : 800) * 0.75)

    onOpened: {
        tab = 0
        name.text = input ? input.name : ""
        look.load()
        if (input && input.has_audio)
            strip.load()
        ptzAddress.text = input && input.ptz ? input.ptz.address : ""
    }

    QQC2.ScrollView {
        id: scroll
        anchors.fill: parent
        contentWidth: availableWidth

        ColumnLayout {
            id: column
            width: scroll.availableWidth
            spacing: Kirigami.Units.smallSpacing

            KanteTabBar {
                Layout.fillWidth: true
                model: dialog.tabs
                currentIndex: dialog.tab
                onActivated: index => dialog.tab = index
            }

            StackLayout {
                Layout.fillWidth: true
                currentIndex: dialog.tab

                // General
                ColumnLayout {
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("Name")
                        KanteTextField {
                            id: name
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 16
                            onEditingFinished: if (dialog.input && text.trim() !== "" && text.trim() !== dialog.input.name)
                                dialog.act({ action: "rename_input", input: dialog.ref(), name: text.trim() })
                        }
                    }
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("Colour tag")
                        QQC2.ComboBox {
                            id: colourBox
                            readonly property var choices: [{ value: null, label: qsTr("None") }]
                                .concat(Texts.choices("colors", dialog.core.choices("colors")))
                            model: choices.map(c => c.label)
                            currentIndex: dialog.input ? Math.max(0, choices.findIndex(c => c.value === dialog.input.color)) : 0
                            onActivated: index => dialog.act({ action: "set_input_color", input: dialog.ref(), color: choices[index].value })
                            KanteFieldSkin { control: colourBox }
                        }
                    }
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("When the signal is lost")
                        visible: dialog.input !== null && dialog.input.source.kind !== "audio_device"
                        QQC2.ComboBox {
                            id: lossBox
                            readonly property var modes: ["hold", "black"]
                            model: [qsTr("Hold last frame"), qsTr("Black")]
                            currentIndex: dialog.input ? Math.max(0, modes.indexOf(dialog.input.signal_loss)) : 0
                            onActivated: index => dialog.act({ action: "set_signal_loss", input: dialog.ref(), mode: modes[index] })
                            KanteFieldSkin { control: lossBox }
                        }
                    }
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("Source")
                        RowLayout {
                            QQC2.Label {
                                Layout.maximumWidth: Kirigami.Units.gridUnit * 14
                                text: dialog.input ? Texts.sourceBadge(dialog.input.source.kind) + " " + dialog.sourceText(dialog.input.source) : ""
                                elide: Text.ElideMiddle
                            }
                            KanteButton {
                                text: qsTr("Replace…")
                                onClicked: dialog.replaceRequested(dialog.input)
                            }
                        }
                    }
                }

                // Colour
                ColumnLayout {
                    QQC2.Label {
                        Layout.fillWidth: true
                        visible: !dialog.looksAvailable
                        wrapMode: Text.WordWrap
                        color: KanteStyle.mutedTextColor
                        text: qsTr("Colour correction needs the GL mixer (lmix-core --mixer gl).")
                    }
                    LookPanel {
                        id: look
                        Layout.fillWidth: true
                        enabled: dialog.looksAvailable
                        act: dialog.act
                        input: dialog.input
                    }
                }

                // Audio
                ColumnLayout {
                    QQC2.Label {
                        Layout.fillWidth: true
                        visible: dialog.input !== null && !dialog.input.has_audio
                        text: qsTr("This input has no sound.")
                        color: KanteStyle.mutedTextColor
                    }
                    ChannelStripPanel {
                        id: strip
                        Layout.fillWidth: true
                        visible: dialog.input !== null && dialog.input.has_audio && strip.strip !== null
                        act: dialog.act
                        input: dialog.input
                    }
                }

                // Triggers
                ColumnLayout {
                    QQC2.Label {
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        text: dialog.input && dialog.input.triggers && dialog.input.triggers.length > 0
                            ? qsTr("%1 triggers run actions when this input goes on air, off air or ends.").arg(dialog.input.triggers.length)
                            : qsTr("No triggers.")
                    }
                    KanteButton {
                        text: qsTr("Edit triggers…")
                        onClicked: dialog.triggersRequested(dialog.input)
                    }
                }

                // PTZ
                ColumnLayout {
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("Camera address")
                        RowLayout {
                            KanteTextField {
                                id: ptzAddress
                                Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                                placeholderText: "192.168.1.20:52381"
                            }
                            KanteButton {
                                text: qsTr("Set")
                                onClicked: dialog.act({ action: "set_ptz", input: dialog.ref(), ptz: ptzAddress.text.trim() === "" ? null
                                    : { address: ptzAddress.text.trim(), protocol: dialog.input.ptz ? dialog.input.ptz.protocol : "visca_ip" } })
                            }
                        }
                    }
                    KanteButton {
                        text: qsTr("Controls…")
                        enabled: dialog.input !== null && !!dialog.input.ptz
                        onClicked: dialog.ptzRequested(dialog.input)
                    }
                }

                // Advanced
                GridLayout {
                    columns: 2
                    columnSpacing: Kirigami.Units.largeSpacing
                    QQC2.Label { text: qsTr("Id"); color: KanteStyle.mutedTextColor }
                    TextEdit {
                        text: dialog.input ? dialog.input.id : ""
                        readOnly: true
                        selectByMouse: true
                        color: Kirigami.Theme.textColor
                        font.family: KanteStyle.monoFamily
                    }
                    QQC2.Label { text: qsTr("Mixer slot"); color: KanteStyle.mutedTextColor }
                    QQC2.Label { text: dialog.input ? String(dialog.input.slot + 1) : "" }
                    QQC2.Label { text: qsTr("State"); color: KanteStyle.mutedTextColor }
                    QQC2.Label { text: dialog.input ? dialog.input.status : "" }
                    QQC2.Label {
                        visible: dialog.input !== null && !!dialog.input.ingest
                        text: qsTr("WHIP")
                        color: KanteStyle.mutedTextColor
                    }
                    TextEdit {
                        visible: dialog.input !== null && !!dialog.input.ingest
                        text: dialog.input && dialog.input.ingest ? dialog.input.ingest.whip_url : ""
                        readOnly: true
                        selectByMouse: true
                        color: Kirigami.Theme.textColor
                        font.family: KanteStyle.monoFamily
                    }
                }
            }
        }
    }

    /** What a source points at, in a word or a path. */
    function sourceText(source) {
        const value = source.path || source.url || source.uri || source.name || source.id || source.color
            || source.endpoint || source.pattern || (source.paths ? source.paths.join(", ") : "")
        return value || ""
    }
}
