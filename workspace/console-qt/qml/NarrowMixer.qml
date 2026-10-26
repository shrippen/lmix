import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * The sound at the edge of the Mix page (design TonFest): the inputs scroll
 * at the top, the ways out stay at the bottom, a grip between them sets the
 * height of the ways out (at least one row each side).
 *
 *   AUDIO · 12 INPUTS     Solo off  Audio page ›
 *   ▮│ ▮│ ▮│ ▮│
 *   ▮┼ ▮┼ ▮┼ ▮┼          meter and fader per input
 *    1  2  3  4          number, name cut short
 *   M S  M S  …          mute, solo
 *   PH ⋯ PH ⋯            on the headphones, more (sends, channel strip)
 *   ▬  ▬  ▬  ▬          the level on the headphones
 *   ═════ grip ═════
 *   OUTPUT PATHS · 4      + Bus
 *   ▮┼ ▮┼ ▮┼ ▮┼          master, headphones, the buses
 *   MSTR PHN STRM        a bus's name is a field: click and type to rename
 *   PGM MON  3 IN        what it carries (inputs that send, PRE before the faders)
 *   M ⋯  M ⋯  M ⋯ S
 *
 * Names that do not fit are cut short (Core.shortName), the full name is in
 * the tooltip. There is no listening to the master (the headphones hear the
 * inputs or a bus), so the master has no S.
 */
KanteCard {
    id: mixer

    required property var core
    required property var show
    required property var act
    /** Peaks per input id, `program`, `phones` and `bus:<id>` (Core.levels as an object). */
    property var levels: ({})
    /** Height of the ways out, in px (stored by the page); 0: every row of them, at most half the column. */
    property real pathsHeight: 0
    signal pathsHeightMoved(real height)
    signal audioPageRequested()

    readonly property var soundInputs: show ? show.inputs.filter(i => i.has_audio) : []
    readonly property var buses: show && show.buses ? show.buses : []
    readonly property var master: show && show.master ? show.master : ({ volume: 100, mute: false })
    readonly property var phones: show && show.phones ? show.phones : ({ volume: 100, mute: false, pre_fader: false })
    readonly property var outputs: show && show.outputs ? show.outputs : []
    /** `buses` at most and `name_max` characters of a name (lmix-types). */
    readonly property var limits: JSON.parse(core.choices("bus_limits"))
    readonly property bool soloActive: soundInputs.some(i => i.listen && i.listen !== "off")
        || buses.some(b => b.listen)

    // Keys of the ways out (input and bus ids never start with a colon).
    readonly property string masterKey: ":master"
    readonly property string phonesKey: ":phones"

    /** Strips side by side and the height a row needs at least. */
    readonly property int perRow: Math.max(1, Math.floor((width - 2 * Kirigami.Units.smallSpacing)
        / (Kirigami.Units.gridUnit * 2.75)))
    readonly property real cellWidth: Math.floor((width - 2 * Kirigami.Units.smallSpacing) / perRow)
    readonly property real minRow: Kirigami.Units.gridUnit * 11
    /** Characters of a short name under a strip. */
    readonly property int nameChars: Math.max(3, Math.floor((cellWidth - Kirigami.Units.smallSpacing)
        / smallMono.averageCharacterWidth))

    readonly property font smallFont: smallMono.font
    readonly property real smallCharWidth: smallMono.averageCharacterWidth
    /** The strip whose ⋯ menu is open, "" without. */
    readonly property string menuKey: menu.key
    function openMenu(key, item) {
        menu.openFor(key, item)
    }
    /** A bus's menu with its name to edit. */
    function renameBus(key, item) {
        menu.focusName = true
        menu.openFor(key, item)
    }

    FontMetrics {
        id: smallMono
        font.family: KanteStyle.monoFamily
        font.pointSize: Kirigami.Theme.smallFont.pointSize
    }

    // Strips by id: a new state (every fader step brings one) keeps the
    // delegates, so a fader drag or a name being typed goes on.
    KeyedModel { id: inputStrips }
    KeyedModel { id: pathStrips }
    KeyedModel { id: menuBuses }
    function syncStrips() {
        inputStrips.sync(soundInputs.map(i => i.id))
        menuBuses.sync(buses.map(b => b.id))
        pathStrips.sync([masterKey, phonesKey].concat(buses.map(b => b.id)))
    }
    Component.onCompleted: syncStrips()
    onSoundInputsChanged: syncStrips()
    onBusesChanged: syncStrips()

    function inputOf(key) {
        return soundInputs.find(i => i.id === key)
            || ({ id: key, number: 0, name: "", audio: { on: false, volume: 0, follow_video: false }, listen: "off", phones: { on: false, level: 100 }, sends: {} })
    }
    function busOf(key) {
        return buses.find(b => b.id === key) || ({ id: key, name: "", pre_fader: false, volume: 100, mute: false, listen: false })
    }
    /** Inputs that send to `bus`. */
    function sendersOf(bus) {
        return soundInputs.filter(i => i.sends && i.sends[bus.id] > 0).length
    }
    /** What a bus carries, in at most 5 characters: "3 IN", before the faders "PRE 3". */
    function busLine(bus) {
        const n = sendersOf(bus)
        const text = bus.pre_fader ? qsTr("PRE %1", "bus taken before the faders, n inputs").arg(n)
            : qsTr("%1 IN", "n inputs send to the bus").arg(n)
        return text.length > 5 ? text.replace(" ", "") : text
    }
    function busLineLong(bus) {
        const n = sendersOf(bus)
        return bus.pre_fader ? qsTr("Inputs sending: %1, before the faders").arg(n)
            : qsTr("Inputs sending: %1, after the faders").arg(n)
    }
    /** Names of the outputs that carry `bus`. */
    function outputsOf(bus) {
        return outputs.filter(o => o.bus === bus.id).map(o => o.name)
    }

    /** Fits as many rows as the height takes, at least one; more scroll. */
    function rowHeight(height, rows) {
        const fit = Math.max(1, Math.min(rows, Math.floor(height / minRow)))
        return Math.max(minRow, Math.floor(height / fit))
    }

    Accessible.name: qsTr("Audio mixer")

    ColumnLayout {
        id: column
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            id: header
            Layout.fillWidth: true
            KanteSectionLabel {
                Layout.fillWidth: true
                text: qsTr("Audio · %1 inputs").arg(mixer.soundInputs.length)
                elide: Text.ElideRight
            }
            KanteButton {
                visible: mixer.soloActive
                size: KanteButton.Size.Small
                focusPolicy: Qt.NoFocus
                emphasis: KanteButton.Emphasis.Primary
                text: qsTr("Solo off")
                QQC2.ToolTip.visible: hovered
                QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                QQC2.ToolTip.text: qsTr("The headphones hear only the solos. Click: all again.")
                onClicked: mixer.act({ action: "clear_listen" })
            }
            KanteButton {
                emphasis: KanteButton.Emphasis.Quiet
                size: KanteButton.Size.Small
                focusPolicy: Qt.NoFocus
                text: qsTr("Audio page ›")
                Accessible.name: qsTr("Open the Audio page")
                onClicked: mixer.audioPageRequested()
            }
        }

        KanteEmptyState {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: mixer.soundInputs.length === 0
            text: qsTr("No sound")
        }

        GridView {
            id: inputGrid
            visible: mixer.soundInputs.length > 0
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.minimumHeight: mixer.minRow
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: inputStrips
            QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
            cellWidth: mixer.cellWidth
            // The model's count: the view's own changes while it lays out (a binding loop).
            cellHeight: mixer.rowHeight(height, Math.ceil(inputStrips.count / mixer.perRow))

            delegate: InputStrip {
                required property string key
                host: mixer
                input: mixer.inputOf(key)
                width: inputGrid.cellWidth - Kirigami.Units.smallSpacing
                height: inputGrid.cellHeight - Kirigami.Units.smallSpacing
            }
        }

        // Drag: the height of the ways out; double click: back.
        KanteSplitHandle {
            id: pathsHandle
            Layout.fillWidth: true
            value: paths.height
            inverted: true
            from: mixer.minRow + pathsLabel.implicitHeight + paths.spacing
            to: Math.max(from, column.height - header.height - mixer.minRow - height - 3 * column.spacing)
            defaultValue: Math.max(from, Math.min(to, paths.allRows, column.height / 2))
            accessibleName: qsTr("Height of the output paths")
            // Back to the default: back to following the rows.
            onMoved: v => mixer.pathsHeightMoved(v === defaultValue ? 0 : v)
        }

        ColumnLayout {
            id: paths
            Layout.fillWidth: true
            // A layout fills by default; the grip sets this one's height.
            Layout.fillHeight: false
            // Rows of the ways out, each as high as a row needs at least.
            readonly property real allRows: pathsLabel.implicitHeight + spacing
                + Math.ceil(pathStrips.count / mixer.perRow) * mixer.minRow
            Layout.preferredHeight: Math.max(pathsHandle.from, Math.min(pathsHandle.to,
                mixer.pathsHeight > 0 ? mixer.pathsHeight : Math.min(allRows, column.height / 2)))
            spacing: Kirigami.Units.smallSpacing

            RowLayout {
                id: pathsLabel
                Layout.fillWidth: true
                KanteSectionLabel {
                    Layout.fillWidth: true
                    text: qsTr("Output paths · %1").arg(2 + mixer.buses.length)
                    elide: Text.ElideRight
                }
                KanteButton {
                    enabled: mixer.buses.length < mixer.limits.buses
                    size: KanteButton.Size.Small
                    focusPolicy: Qt.NoFocus
                    text: qsTr("+ Bus")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                    QQC2.ToolTip.text: qsTr("A mix of its own for an output: in-ear, a stream, a recording.")
                    onClicked: mixer.act({ action: "add_bus" })
                }
            }

            GridView {
                id: pathGrid
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                model: pathStrips
                QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                cellWidth: mixer.cellWidth
                cellHeight: mixer.rowHeight(height, Math.ceil(pathStrips.count / mixer.perRow))
                // A bus added: show it; removed: from the top (the master first).
                property int lastCount: 0
                onCountChanged: {
                    if (count > lastCount && lastCount > 0) {
                        positionViewAtEnd()
                    } else {
                        positionViewAtBeginning()
                    }
                    lastCount = count
                }

                delegate: Loader {
                    id: path
                    required property string key
                    width: pathGrid.cellWidth - Kirigami.Units.smallSpacing
                    height: pathGrid.cellHeight - Kirigami.Units.smallSpacing
                    sourceComponent: key === mixer.masterKey ? masterStrip
                        : key === mixer.phonesKey ? phonesStrip : busStrip

                    Component {
                        id: masterStrip
                        PathStrip {
                            host: mixer
                            key: path.key
                            name: qsTr("Master")
                            line: qsTr("PGM", "program sound, short")
                            lineLong: qsTr("Program sound")
                            nameColor: KanteStyle.negativeTextColor
                            level: mixer.levels.program
                            volume: mixer.master.volume
                            on: !mixer.master.mute
                            onVolumeMoved: v => mixer.act({ action: "set_master", volume: v, mute: mixer.master.mute })
                            onToggled: mixer.act({ action: "set_master", volume: mixer.master.volume, mute: !mixer.master.mute })
                        }
                    }
                    Component {
                        id: phonesStrip
                        PathStrip {
                            host: mixer
                            key: path.key
                            name: qsTr("Headphones")
                            line: qsTr("MON", "listening, short")
                            lineLong: qsTr("Listening")
                            level: mixer.levels.phones
                            volume: mixer.phones.volume
                            on: !mixer.phones.mute
                            onVolumeMoved: v => mixer.act({ action: "set_phones", volume: v, mute: mixer.phones.mute })
                            onToggled: mixer.act({ action: "set_phones", volume: mixer.phones.volume, mute: !mixer.phones.mute })
                        }
                    }
                    Component {
                        id: busStrip
                        PathStrip {
                            host: mixer
                            readonly property var bus: mixer.busOf(path.key)
                            key: path.key
                            isBus: true
                            name: bus.name
                            line: mixer.busLine(bus)
                            lineLong: mixer.busLineLong(bus)
                            level: mixer.levels["bus:" + bus.id]
                            volume: bus.volume
                            on: !bus.mute
                            listening: bus.listen
                            onVolumeMoved: v => mixer.act({ action: "set_bus", bus: bus.id, volume: v, mute: bus.mute })
                            onToggled: mixer.act({ action: "set_bus", bus: bus.id, volume: bus.volume, mute: !bus.mute })
                            onListenToggled: mixer.act({ action: "set_bus_listen", bus: bus.id, on: !bus.listen })
                        }
                    }
                }
            }
        }
    }

    // One menu for every strip's ⋯: what it shows follows `key`.
    KantePopover {
        id: menu
        property string key: ""
        readonly property bool isMaster: key === mixer.masterKey
        readonly property bool isPhones: key === mixer.phonesKey
        readonly property var bus: mixer.buses.find(b => b.id === key) || null
        readonly property var input: !isMaster && !isPhones && !bus ? mixer.soundInputs.find(i => i.id === key) || null : null
        title: isMaster ? qsTr("Master") : isPhones ? qsTr("Headphones")
            : bus ? qsTr("Bus “%1”").arg(bus.name) : input ? qsTr("Input %1 %2").arg(input.number).arg(input.name) : ""
        onClosed: {
            // Closed to open another strip's: with a fade out, Qt may say
            // so only while the new one opens.
            if (!reopening) {
                key = ""
                focusName = false
            }
        }
        property bool reopening: false
        /** Opened from the bus's name: its field takes the keys, but only
         *  once open. A click that just closed the menu does not reopen it,
         *  and a field in the closed menu would keep the show's keys. */
        property bool focusName: false
        onOpened: {
            if (focusName && nameField.visible) {
                nameField.forceActiveFocus()
                nameField.selectAll()
            }
            focusName = false
        }
        // The content is a Column (no Layout): texts wrap at a set width.

        function openFor(key, item) {
            if (menu.key === key && menu.opened) {
                focusName = false
                close()
                return
            }
            // Another strip's while open: closed first, so its name is kept
            // or renamed and the new one starts afresh.
            if (menu.opened) {
                reopening = true
                close()
            }
            menu.key = key
            // The column lays out on the next frame: measured now, the menu
            // would take the size of the one before and cover its button.
            contentItem.forceLayout()
            openAt(item)
            reopening = false
            // The press that closed it was on this button: it stays closed.
            if (!menu.visible) {
                menu.key = ""
                focusName = false
            }
        }

        // An input: its sends, the channel strip, sound follows picture.
        KanteSectionLabel {
            visible: menu.input !== null
            text: qsTr("Sends to buses")
        }
        QQC2.Label {
            visible: menu.input !== null && mixer.buses.length === 0
            width: Kirigami.Units.gridUnit * 14
            wrapMode: Text.WordWrap
            color: KanteStyle.mutedTextColor
            text: qsTr("No bus yet. “+ Bus” below adds one.")
        }
        // Rows by bus id: a new state (every send step brings one) keeps
        // them, so a send drag goes on.
        Repeater {
            model: menu.input ? menuBuses : null
            RowLayout {
                id: sendRow
                required property string key
                readonly property var modelData: mixer.busOf(key)
                readonly property int level: menu.input && menu.input.sends && menu.input.sends[key] !== undefined
                    ? menu.input.sends[key] : 0
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                    text: sendRow.modelData.name
                    elide: Text.ElideRight
                }
                QQC2.Slider {
                    id: sendSlider
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                    from: 0
                    to: 100
                    stepSize: 1
                    value: sendRow.level
                    Accessible.name: qsTr("Send to %1").arg(sendRow.modelData.name)
                    onMoved: mixer.act({ action: "set_send", input: menu.input.number, bus: sendRow.modelData.id, level: Math.round(value) })
                    KanteSliderSkin { control: sendSlider }
                }
                QQC2.Label {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                    horizontalAlignment: Text.AlignRight
                    font.family: KanteStyle.monoFamily
                    text: sendSlider.value > 0 ? sendSlider.value.toFixed(0) : qsTr("off")
                }
            }
        }
        KanteButton {
            visible: menu.input !== null
            emphasis: KanteButton.Emphasis.Quiet
            text: qsTr("Channel strip: EQ, compressor …")
            onClicked: {
                stripDialog.inputId = menu.input.id
                menu.close()
                stripDialog.open()
            }
        }
        QQC2.Switch {
            id: followSwitch
            visible: menu.input !== null
            text: qsTr("Sound follows picture")
            checked: menu.input ? menu.input.audio.follow_video : false
            onToggled: {
                const want = checked
                checked = Qt.binding(() => menu.input ? menu.input.audio.follow_video : false)
                mixer.act({ action: "set_audio_follow", input: menu.input.number, follow: want })
            }
            KanteCheckSkin { control: followSwitch; shape: KanteCheckSkin.Shape.Switch }
        }

        // A bus: its name. In the menu, not the strip: a field there kept
        // the focus and took the shortcut keys.
        KanteTextField {
            id: nameField
            visible: menu.bus !== null
            width: Kirigami.Units.gridUnit * 14
            maximumLength: mixer.limits.name_max
            Accessible.name: qsTr("Name of the bus")
            placeholderText: qsTr("Name")
            /** The bus being renamed. Only closing renames (Enter closes):
             *  losing the focus to another window must not. */
            property string busId: ""
            /** The name when it opened: only what was typed renames, not a
             *  rename from elsewhere while the menu was open. */
            property string openedName: ""
            // Renamed elsewhere before anything was typed: show the new name.
            readonly property string busName: menu.bus ? menu.bus.name : ""
            onBusNameChanged: {
                if (menu.opened && busId === (menu.bus ? menu.bus.id : "") && text === openedName) {
                    openedName = busName
                    text = busName
                }
            }
            Connections {
                target: menu
                function onAboutToShow() {
                    nameField.busId = menu.bus ? menu.bus.id : ""
                    nameField.openedName = menu.bus ? menu.bus.name : ""
                    nameField.text = nameField.openedName
                }
                function onAboutToHide() {
                    const bus = mixer.buses.find(b => b.id === nameField.busId)
                    nameField.busId = ""
                    const wanted = nameField.text.trim()
                    if (bus && wanted.length > 0 && wanted !== nameField.openedName && wanted !== bus.name) {
                        mixer.act({ action: "rename_bus", bus: bus.id, name: wanted })
                    }
                }
            }
            onAccepted: menu.close()
            // Esc: the name stays as it was.
            Keys.onEscapePressed: {
                busId = ""
                menu.close()
            }
        }

        // A bus or the headphones: where the sends are taken.
        QQC2.Switch {
            id: tapSwitch
            visible: menu.bus !== null || menu.isPhones
            readonly property bool tap: menu.bus ? menu.bus.pre_fader : mixer.phones.pre_fader
            text: qsTr("Before the faders")
            checked: tap
            onToggled: {
                const want = checked
                checked = Qt.binding(() => tapSwitch.tap)
                mixer.act(menu.bus ? { action: "set_bus_tap", bus: menu.bus.id, pre_fader: want }
                    : { action: "set_phones_tap", pre_fader: want })
            }
            KanteCheckSkin { control: tapSwitch; shape: KanteCheckSkin.Shape.Switch }
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            QQC2.ToolTip.text: qsTr("On: the sends ignore the channel faders and on/off (a monitor mix).")
        }
        QQC2.Label {
            visible: menu.isPhones
            width: Kirigami.Units.gridUnit * 14
            wrapMode: Text.WordWrap
            color: KanteStyle.mutedTextColor
            text: qsTr("Hears every input with PH at its level there; while one is solo, only the solos.")
        }
        QQC2.Label {
            visible: menu.bus !== null
            width: Kirigami.Units.gridUnit * 14
            wrapMode: Text.WordWrap
            color: KanteStyle.mutedTextColor
            text: {
                const names = menu.bus ? mixer.outputsOf(menu.bus) : []
                return names.length > 0 ? qsTr("Outputs with it: %1").arg(names.join(", "))
                    : qsTr("No output carries it yet; an output takes it when it is added.")
            }
        }
        KanteButton {
            visible: menu.bus !== null
            emphasis: KanteButton.Emphasis.Destructive
            text: qsTr("Remove bus")
            onClicked: {
                removeDialog.bus = menu.bus
                // Removed, not renamed.
                nameField.busId = ""
                menu.close()
                removeDialog.open()
            }
        }
        QQC2.Label {
            visible: menu.isMaster
            width: Kirigami.Units.gridUnit * 14
            wrapMode: Text.WordWrap
            color: KanteStyle.mutedTextColor
            text: qsTr("Every output without a bus carries the master.")
        }
        KanteButton {
            emphasis: KanteButton.Emphasis.Quiet
            text: qsTr("Audio page ›")
            onClicked: {
                menu.close()
                mixer.audioPageRequested()
            }
        }
    }

    ChannelStripDialog {
        id: stripDialog
        /** The input's id: the dialog follows its state (and its number, if moved). */
        property string inputId: ""
        input: mixer.soundInputs.find(i => i.id === inputId) || null
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: mixer.act
        buses: mixer.buses
    }

    KanteDialog {
        id: removeDialog
        property var bus: null
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        width: Kirigami.Units.gridUnit * 24
        title: bus ? qsTr("Remove %1?").arg(bus.name) : ""
        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
        // KanteDialog has a child of its own, so it does not size to this.
        contentHeight: removeText.implicitHeight
        onAccepted: mixer.act({ action: "remove_bus", bus: bus.id })
        QQC2.Label {
            id: removeText
            width: parent.width
            text: qsTr("Every input's send to it goes with it.")
            wrapMode: Text.WordWrap
        }
    }

    /** A small button of a strip; `active` shows it on. */
    component StripButton: KanteButton {
        property bool active: false
        property string tip: ""
        Layout.fillWidth: true
        Layout.preferredWidth: 1
        size: KanteButton.Size.Small
        // Two side by side in a narrow strip: the text needs the room.
        leftPadding: KanteStyle.unit(2)
        rightPadding: KanteStyle.unit(2)
        focusPolicy: Qt.NoFocus
        emphasis: active ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
        Accessible.checkable: true
        Accessible.checked: active
        QQC2.ToolTip.visible: hovered
        QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
        QQC2.ToolTip.text: tip
    }

    /** Meter and fader side by side. */
    component Fader: RowLayout {
        id: fader
        property string name: ""
        property var level: undefined
        property int volume: 0
        signal moved(int volume)
        Layout.fillHeight: true
        Layout.alignment: Qt.AlignHCenter
        spacing: KanteStyle.unit(2)
        KanteAudioMeter {
            Layout.fillHeight: true
            vertical: true
            level: fader.level
        }
        QQC2.Slider {
            id: slider
            Layout.fillHeight: true
            orientation: Qt.Vertical
            from: 0
            to: 100
            stepSize: 1
            value: fader.volume
            Accessible.name: qsTr("Volume of %1").arg(fader.name)
            QQC2.ToolTip.visible: hovered || pressed
            QQC2.ToolTip.text: qsTr("%1: %2").arg(fader.name).arg(Math.round(value))
            onMoved: fader.moved(Math.round(value))
            KanteSliderSkin { control: slider }
        }
    }

    /** One input: fader, number and name, M S PH ⋯, the level on the headphones. */
    component InputStrip: ColumnLayout {
        id: strip
        /** The mixer (an inline component does not see the file's ids). */
        required property var host
        property var input
        readonly property string fullName: input.number + " " + input.name
        readonly property var phonesSend: input.phones || ({ on: true, level: 100 })
        readonly property bool solo: !!input.listen && input.listen !== "off"
        spacing: KanteStyle.unit(2)

        Fader {
            name: strip.fullName
            level: host.levels[strip.input.id]
            volume: strip.input.audio.volume
            onMoved: v => host.act({ action: "set_volume", input: strip.input.number, volume: v })
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: String(strip.input.number)
            horizontalAlignment: Text.AlignHCenter
            font.family: KanteStyle.monoFamily
            Accessible.ignored: true
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: host.core.shortName(strip.input.name, host.nameChars)
            horizontalAlignment: Text.AlignHCenter
            clip: true
            font: host.smallFont
            color: KanteStyle.mutedTextColor
            Accessible.ignored: true
            HoverHandler { id: nameHover }
            QQC2.ToolTip.visible: nameHover.hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            QQC2.ToolTip.text: strip.fullName
        }
        GridLayout {
            Layout.fillWidth: true
            columns: 2
            rowSpacing: KanteStyle.unit(2)
            columnSpacing: KanteStyle.unit(2)
            StripButton {
                text: qsTr("M", "mute, short")
                // The name says the state; "checked" would mean the opposite.
                Accessible.checkable: false
                emphasis: strip.input.audio.on ? KanteButton.Emphasis.Normal : KanteButton.Emphasis.Destructive
                Accessible.name: strip.input.audio.on ? qsTr("Mute %1").arg(strip.fullName) : qsTr("Unmute %1").arg(strip.fullName)
                tip: strip.input.audio.on ? qsTr("%1 is on. Click: mute.").arg(strip.fullName) : qsTr("%1 is muted. Click: on.").arg(strip.fullName)
                onClicked: host.act({ action: "set_audio", input: strip.input.number, on: !strip.input.audio.on })
            }
            StripButton {
                text: qsTr("S", "solo, short")
                active: strip.solo
                Accessible.name: qsTr("Solo %1").arg(strip.fullName)
                tip: qsTr("Solo: only this input on the headphones")
                onClicked: host.act({ action: "set_listen", input: strip.input.number, listen: strip.solo ? "off" : "afl" })
            }
            StripButton {
                text: qsTr("PH", "on the headphones, short")
                active: strip.phonesSend.on
                Accessible.name: qsTr("%1 on the headphones").arg(strip.fullName)
                tip: strip.phonesSend.on ? qsTr("On the headphones. Click: off.") : qsTr("Not on the headphones. Click: on.")
                onClicked: host.act({ action: "set_phones_send", input: strip.input.number, on: !strip.phonesSend.on, level: strip.phonesSend.level })
            }
            StripButton {
                id: more
                text: "⋯"
                active: host.menuKey === strip.input.id
                Accessible.name: qsTr("More for %1").arg(strip.fullName)
                tip: qsTr("Sends to buses, channel strip")
                onClicked: host.openMenu(strip.input.id, more)
            }
        }
        QQC2.Slider {
            id: phonesLevel
            Layout.fillWidth: true
            from: 0
            to: 100
            stepSize: 1
            value: strip.phonesSend.level
            Accessible.name: qsTr("Level of %1 on the headphones").arg(strip.fullName)
            QQC2.ToolTip.visible: hovered || pressed
            QQC2.ToolTip.text: qsTr("On the headphones: %1").arg(strip.phonesSend.on ? Math.round(value) : qsTr("off"))
            onMoved: host.act({ action: "set_phones_send", input: strip.input.number, on: strip.phonesSend.on, level: Math.round(value) })
            KanteSliderSkin { control: phonesLevel }
        }
    }

    /** A way out: the master, the headphones or a bus (`isBus`: the name opens the menu, S to listen). */
    component PathStrip: ColumnLayout {
        id: path
        required property var host
        property string key
        property string name: ""
        property string line: ""
        property string lineLong: ""
        property color nameColor: KanteStyle.textColor
        property var level: undefined
        property int volume: 0
        property bool on: true
        property bool isBus: false
        property bool listening: false
        signal volumeMoved(int volume)
        signal toggled()
        signal listenToggled()
        spacing: KanteStyle.unit(2)

        Fader {
            name: path.name
            level: path.level
            volume: path.volume
            onMoved: v => path.volumeMoved(v)
        }
        // The name; a bus's opens its menu with the name to edit.
        QQC2.Label {
            id: nameLabel
            Layout.fillWidth: true
            text: host.core.shortName(path.name, host.nameChars)
            horizontalAlignment: Text.AlignHCenter
            clip: true
            // The font nameChars is measured in, bold.
            font: Qt.font({ family: host.smallFont.family, pointSize: host.smallFont.pointSize, bold: true })
            color: path.nameColor
            Accessible.ignored: true
            HoverHandler {
                id: nameHover
                cursorShape: path.isBus ? Qt.PointingHandCursor : Qt.ArrowCursor
            }
            TapHandler {
                enabled: path.isBus
                onTapped: host.renameBus(path.key, pathMore)
            }
            QQC2.ToolTip.visible: nameHover.hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            QQC2.ToolTip.text: path.isBus ? qsTr("%1 · click: rename").arg(path.name) : path.name
        }
        QQC2.Label {
            Layout.fillWidth: true
            text: path.line
            horizontalAlignment: Text.AlignHCenter
            clip: true
            font: host.smallFont
            color: KanteStyle.mutedTextColor
            Accessible.name: path.lineLong
            HoverHandler { id: lineHover }
            QQC2.ToolTip.visible: lineHover.hovered
            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
            QQC2.ToolTip.text: path.lineLong
        }
        GridLayout {
            Layout.fillWidth: true
            columns: 2
            rowSpacing: KanteStyle.unit(2)
            columnSpacing: KanteStyle.unit(2)
            StripButton {
                text: qsTr("M", "mute, short")
                // The name says the state; "checked" would mean the opposite.
                Accessible.checkable: false
                emphasis: path.on ? KanteButton.Emphasis.Normal : KanteButton.Emphasis.Destructive
                Accessible.name: path.on ? qsTr("Mute %1").arg(path.name) : qsTr("Unmute %1").arg(path.name)
                tip: path.on ? qsTr("%1 is on. Click: mute.").arg(path.name) : qsTr("%1 is muted. Click: on.").arg(path.name)
                onClicked: path.toggled()
            }
            StripButton {
                id: pathMore
                text: "⋯"
                active: host.menuKey === path.key
                Accessible.name: qsTr("More for %1").arg(path.name)
                tip: path.isBus ? qsTr("Tap, outputs, remove") : qsTr("More settings")
                onClicked: host.openMenu(path.key, pathMore)
            }
            StripButton {
                visible: path.isBus
                Layout.columnSpan: 2
                text: qsTr("S", "solo, short")
                active: path.listening
                Accessible.name: qsTr("Listen to %1").arg(path.name)
                tip: qsTr("Listen to this bus on the headphones")
                onClicked: path.listenToggled()
            }
        }
    }
}
