import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * An input's settings in tabs (UI.8): general (name, colour tag, signal
 * loss, click and GO, automatic playback of clips, source), colour, audio (channel strip and aux sends), triggers,
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

    /** Clips, streams and playlists play, restart and pause by themselves
     *  on and off air (INP.11). */
    readonly property bool plays: input !== null && ["file", "url", "playlist"].indexOf(input.source.kind) >= 0

    /** `set_auto_play` with one switch changed. */
    function setAutoPlay(key, on) {
        var auto = Object.assign({ play: false, restart: false, pause: false }, dialog.input.auto_play)
        auto[key] = on
        dialog.act({ action: "set_auto_play", input: dialog.ref(), auto_play: auto })
    }

    title: input ? qsTr("Settings of %1 %2").arg(input.number).arg(input.name) : ""
    modal: true
    width: Kirigami.Units.gridUnit * 36
    standardButtons: QQC2.Dialog.Close
    contentHeight: Math.min(column.implicitHeight, (parent ? parent.height : 800) * 0.75)

    /** Combo boxes lose their bindings once used: set them from the
     *  input when the dialog opens and whenever the input changes. */
    function sync() {
        if (!input)
            return
        colourBox.currentIndex = Math.max(0, colourBox.choices.findIndex(c => c.value === input.color))
        lossBox.currentIndex = Math.max(0, lossBox.modes.indexOf(input.signal_loss))
        clickBox.currentIndex = Math.max(0, presses.findIndex(p => p.value === input.click))
        goBox.currentIndex = Math.max(0, presses.findIndex(p => p.value === input.go))
    }
    /** What a click and GO can do (INP.8, INP.9). */
    readonly property var presses: Texts.choices("presses", core.choices("presses"))
    onInputChanged: sync()

    onOpened: {
        tab = 0
        tabBar.currentIndex = 0
        sync()
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
                id: tabBar
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
                            onActivated: index => dialog.act({ action: "set_signal_loss", input: dialog.ref(), mode: modes[index] })
                            KanteFieldSkin { control: lossBox }
                        }
                    }
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("Click")
                        QQC2.ComboBox {
                            id: clickBox
                            model: dialog.presses.map(p => p.label)
                            onActivated: index => dialog.act({ action: "set_input_press", input: dialog.ref(), click: dialog.presses[index].value })
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: qsTr("What a click on its tile does.")
                            KanteFieldSkin { control: clickBox }
                        }
                    }
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("GO")
                        QQC2.ComboBox {
                            id: goBox
                            model: dialog.presses.map(p => p.label)
                            onActivated: index => dialog.act({ action: "set_input_press", input: dialog.ref(), go: dialog.presses[index].value })
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: qsTr("What a double click on its tile, GO in its menu and the action go_input do. Quick play: to program with its own effect, and play.")
                            KanteFieldSkin { control: goBox }
                        }
                    }
                    KanteSettingRow {
                        Layout.fillWidth: true
                        title: qsTr("On air")
                        visible: dialog.plays
                        RowLayout {
                            Repeater {
                                model: [
                                    { key: "play", label: qsTr("Play"), tip: qsTr("Starts playing as it goes on air: program, a layout on program, an overlay, or the T-bar.") },
                                    { key: "restart", label: qsTr("From the start"), tip: qsTr("Goes back to the start of its clip as it goes on air.") },
                                    { key: "pause", label: qsTr("Pause after"), tip: qsTr("Pauses once it is off air, after the transition.") }
                                ]
                                QQC2.CheckBox {
                                    id: autoBox
                                    required property var modelData
                                    text: autoBox.modelData.label
                                    checked: dialog.input !== null && !!dialog.input.auto_play && dialog.input.auto_play[autoBox.modelData.key] === true
                                    onToggled: dialog.setAutoPlay(autoBox.modelData.key, autoBox.checked)
                                    QQC2.ToolTip.visible: autoBox.hovered
                                    QQC2.ToolTip.text: autoBox.modelData.tip
                                    KanteCheckSkin { control: autoBox }
                                }
                            }
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
