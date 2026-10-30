import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * New output: stream, recording, NDI, screen or SDI card, of program,
 * preview, the multiview or one input. `edit(id)` opens it for an output
 * there (Settings… in its menu, M07): the fields hold its settings, Save
 * sends `update_output` (and `set_output_bus` when the sound changed)
 * through `updateRequested`. While that output sends or waits for the
 * cards the fields are locked and a line says to stop it first: lmix-core
 * refuses the change then.
 */
KanteDialog {
    id: dialog

    required property var core
    required property var inputs
    /** `buses` of the show state. */
    property var buses: []
    /** `outputs` of the show state, for the one being edited. */
    property var outputs: []
    /** Id of the output being edited; empty: a new one. */
    property string editId: ""
    readonly property bool editing: editId !== ""
    /** The output being edited as the state has it now. */
    readonly property var edited: editing ? outputs.find(o => o.id === editId) || null : null
    /** It sends or waits for the cards: stop it first (lmix-core refuses). */
    readonly property bool locked: edited !== null
        && (edited.status.state === "running" || edited.status.state === "cards_off")
    /** Save in edit mode: `before` the output as it was, `actions` to send
     *  in order (`update_output`, then maybe `set_output_bus`). */
    signal updateRequested(var before, var actions)
    /** Open for output `id` with its settings. */
    function edit(id) {
        editId = id
        open()
    }
    /** Open for a new output. */
    function add() {
        editId = ""
        open()
    }
    // Removed from elsewhere while open: nothing left to edit.
    onEditedChanged: if (visible && editing && edited === null) reject()
    /** Bus choice per further card pair while the form fills in. */
    property var pairChoices: []
    /** The sound choice: 0 the source's own, then the buses. */
    property alias soundIndex: soundBox.currentIndex
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
    // Likewise the inputs, which change with every fader move; followed
    // while the dialog is open.
    property var sourceNames: []
    function syncSourceNames() {
        const names = inputs.map(i => i.number + " " + i.name)
        if (JSON.stringify(names) !== JSON.stringify(sourceNames)) {
            sourceNames = names
        }
    }
    onInputsChanged: if (visible) syncSourceNames()
    onAboutToShow: {
        syncSourceNames()
        prepare()
    }
    Component.onCompleted: syncSoundNames()

    readonly property var kinds: Texts.choices("outputs", core.choices("outputs"))
    readonly property var containers: Texts.choices("containers", core.choices("containers"))
    readonly property string kind: kinds[kindBox.currentIndex] ? kinds[kindBox.currentIndex].value : "srt"
    readonly property bool isCard: kind === "card"
    /** Typed in the text field; a card is picked from its own list. */
    readonly property bool needsText: kind !== "window" && kind !== "fullscreen" && !isCard
    /** The card connectors the engine found (`Device`s of kind card). */
    readonly property var cards: (core.devices.length > 0 ? JSON.parse(core.devices) : []).filter(d => d.kind === "card")
    readonly property var cardModes: Texts.choices("card_modes", core.choices("card_modes"))
    /** Mode a new card output starts in. */
    readonly property string defaultCardMode: "1080p25"
    readonly property var cardChannels: Texts.choices("card_channels", core.choices("card_channels"))
    readonly property int cardSoundDelayMax: JSON.parse(core.choices("card_limits")).sound_delay_ms
    /** Channel pairs of the chosen channels: 1, 4 or 8. */
    readonly property int cardPairs: cardChannels[cardChannelsBox.currentIndex] ? Number(cardChannels[cardChannelsBox.currentIndex].value) / 2 : 1
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

    title: editing ? qsTr("Output settings") : qsTr("Add output")
    modal: true
    anchors.centerIn: parent
    width: Kirigami.Units.gridUnit * 26
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: editing ? qsTr("Save") : qsTr("Add output")

    onOpened: core.refreshLists()
    // The cards come in after opening: the edited output's card again.
    onCardsChanged: {
        if (visible && edited && edited.spec.kind === "card") {
            cardBox.currentIndex = Math.max(0, cards.findIndex(c => c.id === String(edited.spec.device)))
        }
    }

    /** The form before it shows: the edited output's settings, or empty
     *  for a new one. */
    function prepare() {
        if (editing && edited) {
            fill(edited)
            filled = true
            return
        }
        // A new one starts with the last kind, source and encoding, but
        // not with those of an output edited before.
        if (filled) {
            fill({ name: "", spec: { kind: kinds[0] ? kinds[0].value : "srt" }, source: { from: "program" }, bus: null })
            filled = false
        }
        cardBox.currentIndex = 0
        cardChannelsBox.currentIndex = 0
        cardDelay.value = 0
        cardModeBox.currentIndex = Math.max(0, cardModes.findIndex(m => m.value === defaultCardMode))
        textField.text = ""
        nameField.text = ""
        soundBox.currentIndex = 0
        pairChoices = []
    }
    /** The form last held an edited output's settings. */
    property bool filled: false

    /** The form kind of a spec: a screen by its mode. */
    function kindOf(spec) {
        return spec.kind === "display" ? spec.mode.mode : spec.kind
    }
    /** What the text field holds for a spec. */
    function textOf(spec) {
        switch (spec.kind) {
        case "srt": return spec.uri || ""
        case "rtmp": return spec.url || ""
        case "record": return spec.folder || ""
        case "ndi": return spec.name || ""
        case "display": return spec.mode.mode === "drm" ? String(spec.mode.connector) : ""
        }
        return ""
    }
    /** Index of a bus id among the choices after the first; 0 without. */
    function busChoice(id) {
        const at = id ? buses.findIndex(b => b.id === id) : -1
        return at + 1
    }
    /** The form as output `output` has it. */
    function fill(output) {
        const spec = output.spec
        const encoding = spec.encoding || {}
        // Before the channels: the pairs' rows read it as they come.
        pairChoices = (spec.more_pairs || []).map(id => busChoice(id))
        kindBox.currentIndex = Math.max(0, kinds.findIndex(k => k.value === kindOf(spec)))
        textField.text = textOf(spec)
        keyField.text = spec.key || ""
        cardBox.currentIndex = Math.max(0, cards.findIndex(c => c.id === String(spec.device)))
        cardModeBox.currentIndex = Math.max(0, cardModes.findIndex(m => m.value === (spec.mode && spec.kind === "card" ? spec.mode : defaultCardMode)))
        cardChannelsBox.currentIndex = Math.max(0, cardChannels.findIndex(c => c.value === String(spec.channels || "2")))
        cardDelay.value = spec.sound_delay_ms || 0
        kbps.value = spec.video_kbps || 6000
        codecBox.currentIndex = encoding.codec === "h265" ? 1 : 0
        rateBox.currentIndex = encoding.rate === "cbr" ? 1 : 0
        keyframes.value = encoding.keyframe_ms !== undefined ? Math.round(encoding.keyframe_ms / 1000) : 2
        containerBox.currentIndex = Math.max(0, containers.findIndex(c => c.value === (spec.container || "mkv")))
        const source = output.source || { from: "program" }
        sourceBox.currentIndex = source.from === "preview" ? 1
            : source.from === "multiview" ? 2
            : source.from === "input" ? 3 + Math.max(0, inputs.findIndex(i => i.id === source.input))
            : 0
        soundBox.currentIndex = busChoice(output.bus)
        nameField.text = output.name
    }

    /** The form as `forms::OutputForm` reads it. */
    function form() {
        // Program, Preview, Multiview, then the inputs.
        var source = sourceBox.currentIndex <= 0 ? "program"
                   : sourceBox.currentIndex === 1 ? "preview"
                   : sourceBox.currentIndex === 2 ? "multiview"
                   : dialog.inputs[sourceBox.currentIndex - 3].id
        var container = dialog.containers[containerBox.currentIndex] ? dialog.containers[containerBox.currentIndex].value : "mkv"
        return JSON.stringify({
            kind: dialog.kind,
            text: dialog.isCard ? (dialog.cards[cardBox.currentIndex] ? dialog.cards[cardBox.currentIndex].id : "") : textField.text,
            mode: dialog.isCard && dialog.cardModes[cardModeBox.currentIndex] ? dialog.cardModes[cardModeBox.currentIndex].value : "",
            card: dialog.isCard ? {
                channels: dialog.cardChannels[cardChannelsBox.currentIndex] ? dialog.cardChannels[cardChannelsBox.currentIndex].value : "",
                more_pairs: dialog.pairBuses(),
                sound_delay_ms: cardDelay.value
            } : {},
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
            bus: dialog.chosenBus()
        })
    }
    /** Bus id of the sound choice; empty: the source's own. */
    function chosenBus() {
        return dialog.carriesSound && soundBox.currentIndex > 0 && dialog.buses[soundBox.currentIndex - 1] ? dialog.buses[soundBox.currentIndex - 1].id : ""
    }

    onAccepted: {
        if (!editing) {
            core.addOutput(form())
            return
        }
        if (!edited || locked) {
            return
        }
        const update = core.updateOutputAction(editId, form())
        if (update.length === 0) {
            return
        }
        const actions = [JSON.parse(update)]
        // A screen has no sound: the update drops its bus by itself.
        const bus = chosenBus()
        if (dialog.carriesSound && bus !== (edited.bus || "")) {
            actions.push({ action: "set_output_bus", output: editId, bus: bus === "" ? null : bus })
        }
        updateRequested(edited, actions)
    }

    /** Bus id per further card pair; empty: silent. */
    function pairBuses() {
        var ids = []
        for (var i = 0; i < pairRepeater.count; i++) {
            const choice = pairRepeater.itemAt(i).choice
            ids.push(choice > 0 && buses[choice - 1] ? buses[choice - 1].id : "")
        }
        return ids
    }

    acceptEnabled: !dialog.locked
        && (dialog.isCard ? dialog.cards.length > 0 : (!dialog.needsText || textField.text.trim().length > 0))

    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        // Shown, not hidden: what it sends stays readable while it does.
        KanteCallout {
            objectName: "lockedHint"
            Layout.fillWidth: true
            visible: dialog.locked
            kind: KanteCallout.Kind.Info
            text: qsTr("The output is sending. Stop it first to change its settings.")
        }

        ColumnLayout {
            id: fields
            objectName: "fields"
            Layout.fillWidth: true
            enabled: !dialog.locked
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
            // OUT.4: a connector of a card and the mode it plays.
            RowLayout {
                visible: dialog.isCard
                Layout.fillWidth: true
                QQC2.ComboBox {
                    id: cardBox
                    Layout.fillWidth: true
                    model: dialog.cards
                    textRole: "name"
                    KanteFieldSkin { control: cardBox }
                }
                QQC2.ComboBox {
                    id: cardModeBox
                    model: dialog.cardModes
                    textRole: "label"
                    KanteFieldSkin { control: cardModeBox }
                }
            }
            KanteEmptyState {
                Layout.fillWidth: true
                visible: dialog.isCard && dialog.cards.length === 0
                text: qsTr("No card found")
            }
            // OUT.4: embedded channels, a bus on each further pair, and the
            // sound later than the picture (lip sync).
            GridLayout {
                visible: dialog.isCard
                Layout.fillWidth: true
                columns: 2
                QQC2.Label { text: qsTr("Embedded channels") }
                QQC2.ComboBox {
                    id: cardChannelsBox
                    Layout.fillWidth: true
                    model: dialog.cardChannels
                    textRole: "label"
                    KanteFieldSkin { control: cardChannelsBox }
                }
                QQC2.Label { text: qsTr("Sound delay (ms)") }
                QQC2.SpinBox {
                    id: cardDelay
                    from: 0
                    to: dialog.cardSoundDelayMax
                    stepSize: 10
                    editable: true
                    KanteFieldSkin { control: cardDelay }
                }
            }
            Repeater {
                id: pairRepeater
                model: dialog.isCard ? dialog.cardPairs - 1 : 0
                delegate: RowLayout {
                    required property int index
                    readonly property int choice: pairBox.currentIndex
                    Layout.fillWidth: true
                    // Pair 1 carries the output's sound; these start at 3/4.
                    QQC2.Label { text: qsTr("Channels %1/%2").arg(2 * index + 3).arg(2 * index + 4) }
                    QQC2.ComboBox {
                        id: pairBox
                        Layout.fillWidth: true
                        model: [qsTr("Silent")].concat(dialog.soundNames)
                        // The edited output's bus on this pair; a choice holds.
                        Component.onCompleted: currentIndex = dialog.pairChoices[index] || 0
                        KanteFieldSkin { control: pairBox }
                    }
                }
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
                model: [qsTr("Program"), qsTr("Preview"), qsTr("Multiview")].concat(dialog.sourceNames)
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
}
