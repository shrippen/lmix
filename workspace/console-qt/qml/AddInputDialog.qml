import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtQuick.Dialogs
import org.kde.kirigami as Kirigami
import Kante

/**
 * New input, or (with `replacing` set to an input) a new source for it.
 * Every source kind as a tile, grouped (devices, media, network,
 * generated); beside it what the chosen kind needs: a device from the
 * list, a file, an address, a pattern or a colour, and a camera's mode.
 *
 *   DEVICES           │ Camera / capture
 *   [Camera] [Audio]  │ Video from the device.
 *   MEDIA             │ FOUND                  ⟳
 *   [File] [Image] …  │ HDMI capture 1   free
 *   NETWORK           │ OPTIONS  Resolution  Frame rate
 *   …                 │ NAME [          ]   ☐ Then add another
 *
 * An SRT input (SRC.12) gets its address from fields: mode, host, port,
 * latency, passphrase, key length, stream id and the decoder delay; a
 * pasted srt:// address fills them, parameters without a field stay.
 */
KanteDialog {
    id: dialog

    required property var core
    /** The show's inputs: which device another input already uses. */
    property var inputs: []
    /** The input whose source is replaced; null adds a new input. */
    property var replacing: null
    /** The chosen source kind. */
    property string kind: "pattern"

    title: replacing ? qsTr("Replace source of %1").arg(replacing.name) : qsTr("Add input")
    modal: true
    anchors.centerIn: parent
    width: Math.min(Kirigami.Units.gridUnit * 46, parent ? parent.width - Kirigami.Units.gridUnit * 2 : Kirigami.Units.gridUnit * 46)
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: replacing ? qsTr("Replace source") : qsTr("Add input")
    acceptEnabled: complete && !nameTaken

    readonly property var available: core.choices("sources")
    readonly property var groups: [
        { title: qsTr("Devices"), kinds: ["device", "card", "audio_device"] },
        { title: qsTr("Media"), kinds: ["file", "image", "playlist"] },
        { title: qsTr("Network"), kinds: ["url", "srt", "ndi", "whip", "web"] },
        { title: qsTr("Generated"), kinds: ["pattern", "color", "graphics"] }
    ].map(g => ({ title: g.title, kinds: g.kinds.filter(k => available.indexOf(k) >= 0) }))
     .filter(g => g.kinds.length > 0)

    readonly property var icons: ({
        device: "camera-web", card: "video-display", audio_device: "audio-input-microphone",
        file: "video-x-generic", image: "image-x-generic", playlist: "view-media-playlist",
        url: "link", srt: "network-transmit-receive", ndi: "network-workgroup",
        whip: "webcam", web: "internet-web-browser",
        pattern: "view-grid", color: "color-fill", graphics: "insert-text"
    })
    readonly property var hints: ({
        device: qsTr("Capture, webcam"), card: qsTr("DeckLink"), audio_device: qsTr("Mic, interface"),
        file: qsTr("Video file"), image: qsTr("PNG, JPG"), playlist: qsTr("Several files"),
        url: qsTr("HLS, RTMP, RTSP"), srt: qsTr("Caller, listener"), ndi: qsTr("In the network"),
        whip: qsTr("Browser, OBS"), web: qsTr("HTML page"),
        pattern: qsTr("Bars, ball"), color: qsTr("A plain colour"), graphics: qsTr("Titles")
    })
    readonly property var about: ({
        device: qsTr("Video from the device."),
        card: qsTr("Picture and embedded sound from a connector of the card."),
        audio_device: qsTr("Sound only, as a channel of its own."),
        file: qsTr("Plays a file, looped; what happens at the end is set on the input."),
        image: qsTr("A still picture."),
        playlist: qsTr("Files one after another, looped."),
        url: qsTr("Reads a stream from an address."),
        srt: qsTr("Receives MPEG-TS over SRT."),
        ndi: qsTr("A source in the local network."),
        whip: qsTr("Browsers, OBS or encoders send to this endpoint (WebRTC)."),
        web: qsTr("Renders a web page, with its sound."),
        pattern: qsTr("A test signal with a test tone."),
        color: qsTr("A plain colour."),
        graphics: qsTr("A graphics channel's titles over a transparent background.")
    })
    readonly property var placeholders: ({
        file: "/path/to/clip.mp4",
        image: "/path/to/logo.png",
        playlist: "/clips/a.mp4; /clips/b.mp4",
        url: "https://…",
        whip: qsTr("endpoint name, e.g. stage"),
        web: "https://…",
        graphics: qsTr("channel number or name, e.g. 1")
    })

    readonly property bool picksDevice: kind === "device" || kind === "audio_device" || kind === "ndi" || kind === "card"
    readonly property bool picksFiles: kind === "file" || kind === "image" || kind === "playlist"
    readonly property bool typesAddress: ["url", "whip", "web", "graphics"].indexOf(kind) >= 0
    readonly property bool picksSrt: kind === "srt"

    // An SRT input's fields (SRC.12); Rust puts the address together.
    readonly property var srtModes: Texts.choices("srt_modes", core.choices("srt_modes"))
    readonly property var srtLimits: {
        const limits = String(core.choices("srt_limits"))
        return limits.charAt(0) === "{" ? JSON.parse(limits)
            : { passphrase_min: 10, passphrase_max: 79, key_lengths: [16, 24, 32], latency_ms: 125,
                latency_max_ms: 10000, decoder_delay_max_ms: 10000, stream_id_max: 512 }
    }
    /** 0: the sender's key length. */
    readonly property var srtKeyLengths: [{ value: 0, label: qsTr("The sender's") }]
        .concat(srtLimits.key_lengths.map(k => ({ value: k, label: qsTr("AES-%1").arg(k * 8) })))
    property int srtModeIndex: 0
    readonly property string srtMode: srtModes[srtModeIndex] ? srtModes[srtModeIndex].value : "caller"
    /** Parameters of a replaced or pasted address that have no field: kept. */
    property var srtExtra: []
    readonly property int srtPortNumber: /^\d{1,5}$/.test(srtPort.text.trim()) ? parseInt(srtPort.text.trim()) : 0
    /** `SrtAddress` as JSON; without the passphrase for the line that shows
     *  it. The latency at its default stays out of the address. */
    function srtFieldsJson(withPassphrase) {
        const key = srtKeyLengths[Math.max(0, srtKeyBox.currentIndex)].value
        return JSON.stringify({
            mode: srtMode, host: srtHost.text.trim(), port: srtPortNumber,
            latency_ms: srtLatency.value === srtLimits.latency_ms ? undefined : srtLatency.value,
            passphrase: withPassphrase ? srtPassphrase.text : "",
            key_length: key > 0 ? key : undefined, stream_id: srtStreamId.text, extra: srtExtra
        })
    }
    readonly property string srtAddress: picksSrt ? core.srtUri(srtFieldsJson(true)) : ""
    readonly property string srtShown: picksSrt ? core.srtUri(srtFieldsJson(false)) : ""
    /** A latency or decoder delay above the limits that a replaced input
     *  has: the boxes go that high, so the check can say so. */
    property int srtLatencyKept: 0
    property int srtDelayKept: 0
    /** Why a pasted srt:// address does not read (`SrtProblem`), or null. */
    property var srtPasteProblem: null
    /** What lmix-core would refuse in these fields (its own check,
     *  `forms::srt_check`), as `SrtProblem`; null when it takes them. */
    readonly property var srtRefusal: {
        if (!picksSrt)
            return null
        const fields = JSON.parse(srtFieldsJson(true))
        fields.decoder_delay_ms = srtDecoderDelay.value
        const problem = String(core.srtCheck(JSON.stringify(fields)))
        return problem.charAt(0) === "{" ? JSON.parse(problem) : null
    }
    /** Why the fields make no address SRT accepts; empty when they do. A
     *  missing host or port on a form not yet filled in only holds the
     *  button: nothing is wrong yet. */
    readonly property bool srtUntouched: srtHost.text.trim() === "" && srtPort.text.trim() === ""
    readonly property string srtProblem: !picksSrt ? ""
        : srtPasteProblem ? qsTr("This address does not read: %1").arg(Texts.srtProblem(srtPasteProblem))
        : srtRefusal && !(srtUntouched && ["needs_host", "no_port"].indexOf(srtRefusal.problem) >= 0)
            ? Texts.srtProblem(srtRefusal) : ""
    readonly property var srtModeHints: ({
        caller: qsTr("lmix calls the sender at this address."),
        listener: qsTr("lmix waits on this port until the sender calls. Without an address it listens on all of this machine's."),
        rendezvous: qsTr("Both sides call each other at once, through firewalls; the sender uses the same port.")
    })
    /** The fields of `uri` (true), or false when it is no SRT address; then
     *  `srtPasteProblem` says why. */
    function applySrt(uri) {
        const json = String(core.srtFields(uri))
        const a = json.charAt(0) === "{" ? JSON.parse(json) : { problem: "no_srt" }
        if (a.problem) {
            srtPasteProblem = a
            return false
        }
        srtPasteProblem = null
        srtModeIndex = Math.max(0, srtModes.findIndex(m => m.value === a.mode))
        srtHost.text = a.host || ""
        srtHost.lastText = srtHost.text
        srtPort.text = a.port ? String(a.port) : ""
        srtLatencyKept = a.latency_ms || 0
        srtLatency.value = a.latency_ms !== undefined ? a.latency_ms : srtLimits.latency_ms
        srtPassphrase.text = a.passphrase || ""
        srtKeyBox.currentIndex = Math.max(0, srtKeyLengths.findIndex(k => k.value === (a.key_length || 0)))
        srtStreamId.text = a.stream_id || ""
        srtExtra = a.extra || []
        return true
    }
    function resetSrt() {
        srtModeIndex = 0
        srtHost.text = ""
        srtPort.text = ""
        srtLatency.value = srtLimits.latency_ms
        srtPassphrase.text = ""
        srtKeyBox.currentIndex = 0
        srtStreamId.text = ""
        srtDecoderDelay.value = 0
        srtExtra = []
        srtLatencyKept = 0
        srtDelayKept = 0
        srtPasteProblem = null
        srtHost.lastText = ""
    }

    readonly property var patterns: Texts.choices("patterns", core.choices("patterns"))
    property int patternIndex: 0
    /** Signal colours for a colour input (black, white, the two keying
     *  colours): data the input shows, not decoration, so not Kante's. */
    readonly property var signalColors: ["#000000", "#ffffff", "#00b140", "#0047bb"]
    readonly property var signalColorNames: [qsTr("Black"), qsTr("White"), qsTr("Chroma green"), qsTr("Chroma blue")]
    property string colorValue: "#000000"

    readonly property var allDevices: core.devices.length > 0 ? JSON.parse(core.devices) : []
    readonly property string deviceKind: kind === "device" ? "video" : (kind === "audio_device" ? "audio" : (kind === "ndi" || kind === "card" ? kind : ""))
    readonly property var deviceChoices: allDevices.filter(d => d.kind === deviceKind)
    /** The input that already shows a device (by id, NDI by name, a card by number), or null. */
    function inputOfDevice(pick) {
        return inputs.find(i => i.source && (i.source.id === pick || i.source.name === pick
            || (i.source.kind === "card" && String(i.source.device) === pick))) || null
    }
    /** A card's modes; the first lets the card detect it. */
    readonly property var cardModes: [{ value: "", label: qsTr("Detect") }].concat(Texts.choices("card_modes", core.choices("card_modes")))
    /** The eight embedded pairs SDI carries: 1/2 … 15/16. */
    readonly property var cardPairs: [1, 2, 3, 4, 5, 6, 7, 8].map(p => qsTr("Channels %1/%2").arg(2 * p - 1).arg(2 * p))

    // A mode the replaced input has outside these lists comes second, so a
    // replacement keeps it.
    property string keptResolution: ""
    property string keptFramerate: ""
    readonly property var resolutions: withKept([
        { value: "", label: qsTr("The device's own") },
        { value: "3840x2160", label: "3840 × 2160" },
        { value: "1920x1080", label: "1920 × 1080" },
        { value: "1280x720", label: "1280 × 720" },
        { value: "640x480", label: "640 × 480" }
    ], keptResolution)
    readonly property var framerates: withKept([
        { value: "", label: qsTr("The device's own") },
        { value: "60/1", label: "60" },
        { value: "60000/1001", label: "59.94" },
        { value: "50/1", label: "50" },
        { value: "30/1", label: "30" },
        { value: "30000/1001", label: "29.97" },
        { value: "25/1", label: "25" },
        { value: "24/1", label: "24" }
    ], keptFramerate)
    function withKept(list, kept) {
        return kept === "" || list.some(m => m.value === kept)
            ? list : [list[0], { value: kept, label: kept }].concat(list.slice(1))
    }

    readonly property string value: {
        switch (kind) {
        case "pattern": return patterns[patternIndex] ? patterns[patternIndex].value : ""
        case "color": return colorValue
        case "srt": return srtAddress
        default: return textField.text
        }
    }
    /** `forms::SourceOptions` as JSON: a camera's or a card's mode. */
    readonly property string options: kind === "device"
        ? JSON.stringify({ resolution: resolutions[Math.max(0, resolutionBox.currentIndex)].value,
                           framerate: framerates[Math.max(0, framerateBox.currentIndex)].value })
        : kind === "card" ? JSON.stringify({ mode: cardModes[Math.max(0, cardModeBox.currentIndex)].value,
                                             pair: Math.max(0, cardPairBox.currentIndex) + 1 })
        : kind === "srt" ? JSON.stringify({ decoder_delay_ms: srtDecoderDelay.value })
        : "{}"
    /** Another input has the typed name (case aside): lmix-core would
     *  refuse it and what was typed here would be gone (INP.2). */
    readonly property bool nameTaken: {
        const typed = nameField.text.trim().toLowerCase()
        return !replacing && typed.length > 0 && inputs.some(i => i.name.toLowerCase() === typed)
    }
    readonly property bool complete: kind === "pattern" || kind === "color"
        || (kind === "srt" ? !srtRefusal && !srtPasteProblem && srtAddress !== "" : textField.text.trim().length > 0)

    function choose(k) {
        if (k !== kind) {
            kind = k
            textField.text = ""
        }
    }
    function localPath(url) {
        return decodeURIComponent(url.toString().replace(/^file:\/\//, ""))
    }
    /** Files picked or dropped: a playlist adds them, other kinds take the first. */
    function addPaths(paths) {
        if (kind === "playlist" && textField.text.trim().length > 0) {
            textField.text = textField.text.trim() + "; " + paths.join("; ")
        } else {
            textField.text = kind === "playlist" ? paths.join("; ") : (paths[0] || "")
        }
    }

    onOpened: {
        core.refreshLists()
        textField.text = ""
        nameField.text = ""
        // Each opening starts from the standard values, a replacement from what
        // the input shows now: nothing of an earlier dialog stays chosen.
        patternIndex = 0
        colorValue = signalColors[0]
        keptResolution = ""
        keptFramerate = ""
        resolutionBox.currentIndex = 0
        framerateBox.currentIndex = 0
        cardModeBox.currentIndex = 0
        cardPairBox.currentIndex = 0
        resetSrt()
        if (replacing && available.indexOf(replacing.source.kind) >= 0) {
            const source = replacing.source
            kind = source.kind
            textField.text = sourceText(source)
            if (source.kind === "pattern") {
                patternIndex = Math.max(0, patterns.findIndex(p => p.value === source.pattern))
            } else if (source.kind === "color") {
                colorValue = source.color
            } else if (source.kind === "device") {
                keptResolution = source.resolution || ""
                keptFramerate = source.framerate || ""
                resolutionBox.currentIndex = Math.max(0, resolutions.findIndex(r => r.value === (source.resolution || "")))
                framerateBox.currentIndex = Math.max(0, framerates.findIndex(r => r.value === (source.framerate || "")))
            } else if (source.kind === "card") {
                cardModeBox.currentIndex = Math.max(0, cardModes.findIndex(m => m.value === (source.mode || "")))
                cardPairBox.currentIndex = (source.pair || 1) - 1
            } else if (source.kind === "srt") {
                applySrt(source.uri)
                srtDelayKept = source.decoder_delay_ms || 0
                srtDecoderDelay.value = source.decoder_delay_ms || 0
            }
        }
    }

    /** The text field's value of a source (the reverse of forms::source_of). */
    function sourceText(source) {
        switch (source.kind) {
        case "image":
        case "file": return source.path
        case "playlist": return source.paths.join("; ")
        case "url":
        case "web": return source.url
        case "srt": return source.uri
        case "ndi": return source.name
        case "whip": return source.endpoint
        case "graphics": return source.graphics
        case "device":
        case "audio_device": return source.id
        case "card": return String(source.device)
        }
        return ""
    }

    onAccepted: {
        if (replacing) {
            core.replaceSource(replacing.number, kind, value, options)
        } else {
            core.addInput(kind, value, options, nameField.text)
            if (another.checked) {
                Qt.callLater(dialog.open)
            }
        }
    }

    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: body.implicitHeight

    RowLayout {
        id: body
        width: parent.width
        spacing: Kirigami.Units.largeSpacing

        // The kinds, grouped.
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            Layout.preferredWidth: Kirigami.Units.gridUnit * 22
            Layout.maximumWidth: Kirigami.Units.gridUnit * 22
            spacing: Kirigami.Units.smallSpacing

            Repeater {
                model: dialog.groups
                delegate: ColumnLayout {
                    id: group
                    required property var modelData
                    Layout.fillWidth: true
                    spacing: Kirigami.Units.smallSpacing
                    KanteSectionLabel { text: group.modelData.title }
                    GridLayout {
                        Layout.fillWidth: true
                        columns: 3
                        columnSpacing: Kirigami.Units.smallSpacing
                        rowSpacing: Kirigami.Units.smallSpacing
                        Repeater {
                            model: group.modelData.kinds
                            delegate: KanteOptionTile {
                                required property string modelData
                                Layout.fillWidth: true
                                Layout.preferredWidth: 1
                                // One choice across the groups: the dialog keeps it.
                                checkable: false
                                autoExclusive: false
                                checked: dialog.kind === modelData
                                icon.name: dialog.icons[modelData] || ""
                                text: Texts.label("sources", modelData)
                                hint: dialog.hints[modelData] || ""
                                onClicked: dialog.choose(modelData)
                            }
                        }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillHeight: true
            implicitWidth: 1
            color: KanteStyle.ruleColor
        }

        // What the chosen kind needs.
        ColumnLayout {
            Layout.alignment: Qt.AlignTop
            Layout.fillWidth: true
            spacing: Kirigami.Units.smallSpacing

            KanteHeading {
                Layout.fillWidth: true
                level: 3
                text: Texts.label("sources", dialog.kind)
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: dialog.about[dialog.kind] || ""
                wrapMode: Text.WordWrap
                color: KanteStyle.mutedTextColor
            }

            // Test pattern.
            KanteSectionLabel {
                visible: dialog.kind === "pattern"
                Layout.topMargin: Kirigami.Units.smallSpacing
                text: qsTr("Pattern")
            }
            KanteSegmented {
                visible: dialog.kind === "pattern"
                model: dialog.patterns.map(p => p.label)
                currentIndex: dialog.patternIndex
                onActivated: index => dialog.patternIndex = index
            }

            // Colour.
            KanteSectionLabel {
                visible: dialog.kind === "color"
                Layout.topMargin: Kirigami.Units.smallSpacing
                text: qsTr("Colour")
            }
            RowLayout {
                visible: dialog.kind === "color"
                spacing: Kirigami.Units.smallSpacing
                KanteSwatchPicker {
                    colors: dialog.signalColors
                    names: dialog.signalColorNames
                    currentIndex: dialog.signalColors.indexOf(dialog.colorValue)
                    onPicked: index => dialog.colorValue = dialog.signalColors[index]
                }
                KanteButton {
                    text: qsTr("Other colour…")
                    onClicked: colorDialog.open()
                }
                QQC2.Label {
                    text: dialog.colorValue
                    font.family: KanteStyle.monoFamily
                    color: KanteStyle.mutedTextColor
                }
            }

            // A device from the list.
            RowLayout {
                visible: dialog.picksDevice
                Layout.fillWidth: true
                Layout.topMargin: Kirigami.Units.smallSpacing
                KanteSectionLabel {
                    Layout.fillWidth: true
                    text: qsTr("Found")
                }
                KanteToolButton {
                    icon.name: "view-refresh"
                    text: qsTr("Search devices again")
                    display: QQC2.AbstractButton.IconOnly
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: text
                    onClicked: dialog.core.refreshLists()
                }
            }
            ColumnLayout {
                visible: dialog.picksDevice
                Layout.fillWidth: true
                spacing: 0
                Repeater {
                    model: dialog.deviceChoices
                    delegate: KanteListRow {
                        required property var modelData
                        // NDI sources go by name, devices by id.
                        readonly property string pick: dialog.kind === "ndi" ? modelData.name : modelData.id
                        readonly property var user: dialog.inputOfDevice(pick)
                        Layout.fillWidth: true
                        density: KanteListRow.Density.Compact
                        text: modelData.name
                        subtitle: modelData.api || ""
                        meta: user ? qsTr("Input %1").arg(user.number) : qsTr("free")
                        selected: textField.text === pick
                        onClicked: textField.text = pick
                    }
                }
                KanteEmptyState {
                    Layout.fillWidth: true
                    visible: dialog.deviceChoices.length === 0
                    text: qsTr("No devices found")
                }
            }

            // A file (dropped here, picked or typed), an address, or a device id by hand.
            KanteSectionLabel {
                visible: dialog.picksFiles || dialog.typesAddress
                Layout.topMargin: Kirigami.Units.smallSpacing
                text: dialog.picksFiles ? (dialog.kind === "playlist" ? qsTr("Files") : qsTr("File"))
                    : qsTr("Address")
            }
            Item {
                visible: dialog.picksFiles || dialog.typesAddress || dialog.picksDevice
                Layout.fillWidth: true
                implicitHeight: pathRow.implicitHeight

                RowLayout {
                    id: pathRow
                    anchors.left: parent.left
                    anchors.right: parent.right
                    KanteTextField {
                        id: textField
                        Layout.fillWidth: true
                        placeholderText: dialog.picksDevice ? qsTr("device id") : (dialog.placeholders[dialog.kind] || "")
                    }
                    KanteButton {
                        visible: dialog.picksFiles
                        text: qsTr("Choose…")
                        onClicked: fileDialog.open()
                    }
                }
                KanteDropZone {
                    anchors.fill: parent
                    visible: drop.containsDrag
                    kind: KanteDropZone.Kind.Gap
                }
                DropArea {
                    id: drop
                    anchors.fill: parent
                    enabled: dialog.picksFiles
                    keys: ["text/uri-list"]
                    onDropped: event => {
                        dialog.addPaths(event.urls.map(u => dialog.localPath(u)))
                        event.acceptProposedAction()
                    }
                }
            }
            QQC2.Label {
                visible: dialog.picksFiles
                Layout.fillWidth: true
                text: qsTr("Or drop files from the file manager here.")
                font: KanteStyle.smallFont
                color: KanteStyle.mutedTextColor
            }

            // An SRT input's address and options (SRC.12).
            KanteSectionLabel {
                visible: dialog.picksSrt
                Layout.topMargin: Kirigami.Units.smallSpacing
                text: qsTr("Connection")
            }
            KanteSegmented {
                objectName: "srtMode"
                visible: dialog.picksSrt
                model: dialog.srtModes.map(m => m.label)
                currentIndex: dialog.srtModeIndex
                onActivated: index => dialog.srtModeIndex = index
            }
            QQC2.Label {
                visible: dialog.picksSrt
                Layout.fillWidth: true
                text: dialog.srtModeHints[dialog.srtMode] || ""
                wrapMode: Text.WordWrap
                font: KanteStyle.smallFont
                color: KanteStyle.mutedTextColor
            }
            GridLayout {
                visible: dialog.picksSrt
                Layout.fillWidth: true
                columns: 2
                columnSpacing: Kirigami.Units.largeSpacing
                QQC2.Label { text: dialog.srtMode === "listener" ? qsTr("Local address") : qsTr("Address") }
                RowLayout {
                    Layout.fillWidth: true
                    KanteTextField {
                        id: srtHost
                        objectName: "srtHost"
                        Layout.fillWidth: true
                        placeholderText: dialog.srtMode === "listener" ? qsTr("all") : qsTr("host or IP, or paste srt://…")
                        invalid: !!dialog.srtPasteProblem || (!!dialog.srtRefusal && dialog.srtRefusal.problem === "bad_host")
                        // A pasted srt:// address fills the fields at once; one
                        // typed by hand only when the field is left, or the
                        // fields would fill after the port's first digit.
                        // Pasted: the text grew by two or more characters, or
                        // it is an srt:// address now and was none before
                        // (a paste over a selection). Typing over a selected
                        // character inside an address is neither.
                        property string lastText: ""
                        function looksPasted(before, now) {
                            const address = /^srt:\/\/./i
                            return address.test(now.trim())
                                && (now.length - before.length >= 2 || !/^srt:\/\//i.test(before.trim()))
                        }
                        onTextEdited: {
                            const pasted = looksPasted(lastText, text)
                            lastText = text
                            if (pasted)
                                dialog.applySrt(text.trim())
                            else
                                dialog.srtPasteProblem = null
                        }
                        onEditingFinished: if (/^srt:\/\//i.test(text.trim())) dialog.applySrt(text.trim())
                        onTextChanged: if (!activeFocus) lastText = text
                    }
                    QQC2.Label { text: qsTr("Port") }
                    KanteTextField {
                        id: srtPort
                        objectName: "srtPort"
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                        inputMethodHints: Qt.ImhDigitsOnly
                        validator: IntValidator { bottom: 1; top: 65535 }
                        placeholderText: "9000"
                    }
                }
                QQC2.Label { text: qsTr("Latency (ms)") }
                QQC2.SpinBox {
                    id: srtLatency
                    objectName: "srtLatency"
                    from: 0
                    to: Math.max(dialog.srtLimits.latency_max_ms, dialog.srtLatencyKept)
                    stepSize: 20
                    value: dialog.srtLimits.latency_ms
                    editable: true
                    KanteFieldSkin { control: srtLatency }
                }
                QQC2.Label { text: qsTr("Passphrase") }
                KanteTextField {
                    id: srtPassphrase
                    objectName: "srtPassphrase"
                    Layout.fillWidth: true
                    echoMode: TextInput.Password
                    placeholderText: qsTr("none: unencrypted")
                    invalid: !!dialog.srtRefusal && dialog.srtRefusal.problem === "passphrase_length"
                }
                QQC2.Label { text: qsTr("Key length") }
                KanteComboBox {
                    id: srtKeyBox
                    objectName: "srtKeyLength"
                    Layout.fillWidth: true
                    model: dialog.srtKeyLengths
                    textRole: "label"
                }
                QQC2.Label { text: qsTr("Stream ID") }
                KanteTextField {
                    id: srtStreamId
                    objectName: "srtStreamId"
                    Layout.fillWidth: true
                    placeholderText: qsTr("optional, e.g. #!::r=cam1")
                }
                QQC2.Label { text: qsTr("Decoder delay (ms)") }
                QQC2.SpinBox {
                    id: srtDecoderDelay
                    objectName: "srtDecoderDelay"
                    from: 0
                    to: Math.max(dialog.srtLimits.decoder_delay_max_ms, dialog.srtDelayKept)
                    stepSize: 50
                    editable: true
                    KanteFieldSkin { control: srtDecoderDelay }
                }
            }
            QQC2.Label {
                visible: dialog.picksSrt
                Layout.fillWidth: true
                text: qsTr("Latency is the receive buffer: about four times the round trip, the same on both sides. The decoder delay waits for late packets; 0 keeps the delay lowest.")
                wrapMode: Text.WordWrap
                font: KanteStyle.smallFont
                color: KanteStyle.mutedTextColor
            }
            QQC2.Label {
                objectName: "srtProblem"
                visible: dialog.picksSrt && dialog.srtProblem !== ""
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: KanteStyle.negativeTextColor
                text: dialog.srtProblem
            }
            KanteSectionLabel {
                visible: dialog.picksSrt && !dialog.srtRefusal && !dialog.srtPasteProblem
                text: srtPassphrase.text.length > 0 ? qsTr("Address (passphrase not shown)") : qsTr("Address")
            }
            TextEdit {
                objectName: "srtAddress"
                visible: dialog.picksSrt && !dialog.srtRefusal && !dialog.srtPasteProblem
                Layout.fillWidth: true
                text: dialog.srtShown
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.WrapAnywhere
                color: Kirigami.Theme.textColor
                font.family: KanteStyle.monoFamily
            }

            // A camera's mode.
            KanteSectionLabel {
                visible: dialog.kind === "device"
                Layout.topMargin: Kirigami.Units.smallSpacing
                text: qsTr("Options")
            }
            GridLayout {
                visible: dialog.kind === "device"
                Layout.fillWidth: true
                columns: 2
                columnSpacing: Kirigami.Units.largeSpacing
                QQC2.Label { text: qsTr("Resolution") }
                KanteComboBox {
                    id: resolutionBox
                    Layout.fillWidth: true
                    model: dialog.resolutions
                    textRole: "label"
                }
                QQC2.Label { text: qsTr("Frame rate") }
                KanteComboBox {
                    id: framerateBox
                    Layout.fillWidth: true
                    model: dialog.framerates
                    textRole: "label"
                }
            }
            // A card's mode (SRC.1).
            KanteSectionLabel {
                visible: dialog.kind === "card"
                Layout.topMargin: Kirigami.Units.smallSpacing
                text: qsTr("Options")
            }
            GridLayout {
                visible: dialog.kind === "card"
                Layout.fillWidth: true
                columns: 2
                columnSpacing: Kirigami.Units.largeSpacing
                QQC2.Label { text: qsTr("Video mode") }
                KanteComboBox {
                    id: cardModeBox
                    Layout.fillWidth: true
                    model: dialog.cardModes
                    textRole: "label"
                }
                QQC2.Label { text: qsTr("Sound") }
                KanteComboBox {
                    id: cardPairBox
                    Layout.fillWidth: true
                    model: dialog.cardPairs
                }
            }
            QQC2.Label {
                visible: dialog.kind === "device"
                Layout.fillWidth: true
                text: qsTr("Only a mode the device offers uncompressed (not as MJPEG): with any other the input stays without a picture. “The device's own” always works.")
                wrapMode: Text.WordWrap
                font: KanteStyle.smallFont
                color: KanteStyle.mutedTextColor
            }

            KanteSectionLabel {
                visible: !dialog.replacing
                Layout.topMargin: Kirigami.Units.smallSpacing
                text: qsTr("Name")
            }
            KanteTextField {
                id: nameField
                objectName: "inputName"
                visible: !dialog.replacing
                Layout.fillWidth: true
                placeholderText: qsTr("optional")
                invalid: dialog.nameTaken
            }
            QQC2.Label {
                objectName: "nameTaken"
                visible: dialog.nameTaken
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: KanteStyle.negativeTextColor
                text: qsTr("Another input has this name.")
            }
            QQC2.CheckBox {
                id: another
                visible: !dialog.replacing
                text: qsTr("Then add another")
                KanteCheckSkin { control: another }
            }
        }
    }

    ColorDialog {
        id: colorDialog
        selectedColor: dialog.colorValue
        onAccepted: dialog.colorValue = selectedColor.toString()
    }

    FileDialog {
        id: fileDialog
        fileMode: dialog.kind === "playlist" ? FileDialog.OpenFiles : FileDialog.OpenFile
        onAccepted: dialog.addPaths((dialog.kind === "playlist" ? selectedFiles : [selectedFile]).map(u => dialog.localPath(u)))
    }
}
