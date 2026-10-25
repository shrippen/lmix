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
    acceptEnabled: complete

    readonly property var available: core.choices("sources")
    readonly property var groups: [
        { title: qsTr("Devices"), kinds: ["device", "audio_device"] },
        { title: qsTr("Media"), kinds: ["file", "image", "playlist"] },
        { title: qsTr("Network"), kinds: ["url", "srt", "ndi", "whip", "web"] },
        { title: qsTr("Generated"), kinds: ["pattern", "color", "graphics"] }
    ].map(g => ({ title: g.title, kinds: g.kinds.filter(k => available.indexOf(k) >= 0) }))
     .filter(g => g.kinds.length > 0)

    readonly property var icons: ({
        device: "camera-web", audio_device: "audio-input-microphone",
        file: "video-x-generic", image: "image-x-generic", playlist: "view-media-playlist",
        url: "link", srt: "network-transmit-receive", ndi: "network-workgroup",
        whip: "webcam", web: "internet-web-browser",
        pattern: "view-grid", color: "color-fill", graphics: "insert-text"
    })
    readonly property var hints: ({
        device: qsTr("Capture, webcam"), audio_device: qsTr("Mic, interface"),
        file: qsTr("Video file"), image: qsTr("PNG, JPG"), playlist: qsTr("Several files"),
        url: qsTr("HLS, RTMP, RTSP"), srt: qsTr("Caller, listener"), ndi: qsTr("In the network"),
        whip: qsTr("Browser, OBS"), web: qsTr("HTML page"),
        pattern: qsTr("Bars, ball"), color: qsTr("A plain colour"), graphics: qsTr("Titles")
    })
    readonly property var about: ({
        device: qsTr("Video from the device."),
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
        srt: "srt://host:port",
        whip: qsTr("endpoint name, e.g. stage"),
        web: "https://…",
        graphics: qsTr("channel number or name, e.g. 1")
    })

    readonly property bool picksDevice: kind === "device" || kind === "audio_device" || kind === "ndi"
    readonly property bool picksFiles: kind === "file" || kind === "image" || kind === "playlist"
    readonly property bool typesAddress: ["url", "srt", "whip", "web", "graphics"].indexOf(kind) >= 0

    readonly property var patterns: Texts.choices("patterns", core.choices("patterns"))
    property int patternIndex: 0
    /** Signal colours for a colour input (black, white, the two keying
     *  colours): data the input shows, not decoration, so not Kante's. */
    readonly property var signalColors: ["#000000", "#ffffff", "#00b140", "#0047bb"]
    readonly property var signalColorNames: [qsTr("Black"), qsTr("White"), qsTr("Chroma green"), qsTr("Chroma blue")]
    property string colorValue: "#000000"

    readonly property var allDevices: core.devices.length > 0 ? JSON.parse(core.devices) : []
    readonly property string deviceKind: kind === "device" ? "video" : (kind === "audio_device" ? "audio" : (kind === "ndi" ? "ndi" : ""))
    readonly property var deviceChoices: allDevices.filter(d => d.kind === deviceKind)
    /** The input that already shows a device (by id, NDI by name), or null. */
    function inputOfDevice(pick) {
        return inputs.find(i => i.source && (i.source.id === pick || i.source.name === pick)) || null
    }

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
        default: return textField.text
        }
    }
    /** `forms::SourceOptions` as JSON: a camera's mode. */
    readonly property string options: kind === "device"
        ? JSON.stringify({ resolution: resolutions[Math.max(0, resolutionBox.currentIndex)].value,
                           framerate: framerates[Math.max(0, framerateBox.currentIndex)].value })
        : "{}"
    readonly property bool complete: kind === "pattern" || kind === "color" || textField.text.trim().length > 0

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

            QQC2.Label {
                Layout.fillWidth: true
                text: Texts.label("sources", dialog.kind)
                font: KanteStyle.headingFont(KanteStyle.defaultFont.pointSize * 1.1)
                color: KanteStyle.strongTextColor
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
                visible: !dialog.replacing
                Layout.fillWidth: true
                placeholderText: qsTr("optional")
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
