import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Inspector under the cue list (edit mode): the chosen cue's fields on
 * tabs, Basics, Triggers, Time & loops, Levels and Actions. Each change
 * goes to lmix-core at once (Enter or leaving a field); standby and
 * running cues stay (IMP.5). A list from a script is only shown.
 */
KanteCard {
    id: inspector

    /** The ShowView: chosen cue, list, act, helpers. */
    required property var showView
    /** The window's Core, for the colour names. */
    required property var core

    /** While a field here has the keyboard or a menu or dialog of it is open,
     *  the inspector keeps the cue it began with, also when standby moves
     *  on (a follow, a GO from MIDI, OSC or another client): what is typed
     *  goes to that cue, or nowhere when it is gone from the active list. */
    readonly property bool holding: actionDialog.visible || colourMenu.visible || hasFocusInside(Window.activeFocusItem)
    property string heldId: ""
    // Let go only after the field that lost the keyboard saved its value
    // (editingFinished may come after the focus moved).
    onHoldingChanged: {
        if (holding) {
            heldId = shown !== null ? shown.id : ""
        } else {
            Qt.callLater(() => { if (!holding) { heldId = "" } })
        }
    }
    readonly property var shown: heldId !== "" ? (showView.list ? showView.findCue(showView.list.cues, heldId) : null)
                                               : showView.selectedCue
    readonly property var cue: shown
    readonly property bool editable: !showView.fromScript
    readonly property bool timed: cue !== null && ["wait", "actions", "sound", "picture", "fade"].indexOf(cue.kind) >= 0
    /** What the cue plays or shows, whose fades and repeats are edited. */
    readonly property var media: cue === null ? null : (cue.kind === "sound" ? cue.sound : (cue.kind === "picture" ? cue.picture : null))
    property int tab: 0

    function hasFocusInside(item) {
        for (; item; item = item.parent) {
            if (item === inspector) {
                return true
            }
        }
        return false
    }

    /** Changes the shown cue's top-level fields. */
    function save(changes) {
        if (cue !== null) {
            showView.updateCue(cue.id, changes)
        }
    }

    /** Changes fields of the cue's sound, picture or fade; the others of
     *  it stay (merged in lmix-core). */
    function saveIn(part, changes) {
        var next = {}
        next[part] = changes
        save(next)
    }

    function refocus() {
        showView.forceActiveFocus()
    }

    // Escape in a field is the show's panic as everywhere on the page
    // (nothing may keep playing), and it leaves the field without saving
    // what was typed.
    Keys.onEscapePressed: event => {
        for (let item = Window.activeFocusItem; item && item !== inspector; item = item.parent) {
            if (item.typing !== undefined) {
                item.typing = false
                break
            }
            // A spin box takes its text when it loses the keys: give it
            // back its value first.
            if (item.valueFromText !== undefined && item.contentItem) {
                item.contentItem.text = item.textFromValue(item.value, item.locale)
                break
            }
        }
        refocus()
        showView.panic()
        event.accepted = true
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            KanteTabBar {
                Layout.fillWidth: true
                activeFocusOnTab: false
                model: [qsTr("Basics"), qsTr("Triggers"), qsTr("Time & loops"), qsTr("Levels"), qsTr("Actions")]
                currentIndex: inspector.tab
                onActivated: index => {
                    inspector.tab = index
                    inspector.refocus()
                }
            }
            QQC2.Label {
                Layout.maximumWidth: Kirigami.Units.gridUnit * 16
                visible: inspector.cue !== null
                text: inspector.cue ? (inspector.cue.number !== "" ? inspector.cue.number + "  " : "") + inspector.showView.cueName(inspector.cue) : ""
                font.bold: true
                elide: Text.ElideRight
            }
            QQC2.Label {
                visible: !inspector.editable
                text: qsTr("From the script")
                color: KanteStyle.mutedTextColor
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: inspector.cue === null
            text: qsTr("Choose a cue in the list.")
            color: KanteStyle.mutedTextColor
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: inspector.cue !== null
            enabled: inspector.editable
            currentIndex: inspector.tab

            // Basics: number, name, colour, note.
            GridLayout {
                columns: 4
                columnSpacing: Kirigami.Units.largeSpacing
                rowSpacing: Kirigami.Units.smallSpacing

                QQC2.Label { text: qsTr("Number") }
                KanteTextField {
                    id: numberField
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    Accessible.name: qsTr("Cue number")
                    onAccepted: inspector.refocus()
                    onEditingFinished: {
                        if (inspector.cue && text.trim() !== inspector.cue.number) {
                            inspector.save({ number: text.trim() })
                        }
                        typing = false
                    }
                    // What is typed stays until it is saved; a new state
                    // meanwhile does not overwrite it.
                    property bool typing: false
                    onActiveFocusChanged: if (activeFocus) { typing = true }
                    Binding on text {
                        when: !numberField.typing
                        value: inspector.cue ? inspector.cue.number : ""
                        restoreMode: Binding.RestoreNone
                    }
                }
                QQC2.Label { text: qsTr("Name") }
                KanteTextField {
                    id: nameField
                    Layout.fillWidth: true
                    placeholderText: inspector.cue ? inspector.showView.cueName(Object.assign({}, inspector.cue, { name: "" })) : ""
                    Accessible.name: qsTr("Cue name")
                    onAccepted: inspector.refocus()
                    onEditingFinished: {
                        if (inspector.cue && text !== inspector.cue.name) {
                            inspector.save({ name: text })
                        }
                        typing = false
                    }
                    // What is typed stays until it is saved; a new state
                    // meanwhile does not overwrite it.
                    property bool typing: false
                    onActiveFocusChanged: if (activeFocus) { typing = true }
                    Binding on text {
                        when: !nameField.typing
                        value: inspector.cue ? inspector.cue.name : ""
                        restoreMode: Binding.RestoreNone
                    }
                }

                QQC2.Label { text: qsTr("Kind") }
                QQC2.Label {
                    text: inspector.cue ? inspector.showView.kindLabel(inspector.cue) : ""
                    color: KanteStyle.mutedTextColor
                }
                QQC2.Label { text: qsTr("Colour") }
                KanteButton {
                    id: colourButton
                    focusPolicy: Qt.NoFocus
                    text: inspector.cue && inspector.cue.color ? Texts.label("colors", inspector.cue.color) : qsTr("None")
                    onClicked: colourMenu.popup(colourButton, 0, colourButton.height)
                    ChoiceMenu {
                        id: colourMenu
                        choices: [{ value: null, label: qsTr("None") }].concat(Texts.choices("colors", inspector.core.choices("colors")))
                        current: inspector.cue ? (inspector.cue.color || null) : null
                        onPicked: value => inspector.save({ color: value })
                    }
                }

                QQC2.Label {
                    Layout.alignment: Qt.AlignTop
                    text: qsTr("Note")
                }
                QQC2.TextArea {
                    id: noteArea
                    Layout.columnSpan: 3
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: Kirigami.Units.gridUnit * 2
                    wrapMode: TextEdit.Wrap
                    placeholderText: qsTr("For the operator, e.g. a stage direction; shown with standby")
                    Accessible.name: qsTr("Cue note")
                    onEditingFinished: {
                        if (inspector.cue && text !== (inspector.cue.note || "")) {
                            inspector.save({ note: text })
                        }
                        typing = false
                    }
                    // What is typed stays until it is saved; a new state
                    // meanwhile does not overwrite it.
                    property bool typing: false
                    onActiveFocusChanged: if (activeFocus) { typing = true }
                    Binding on text {
                        when: !noteArea.typing
                        value: inspector.cue ? (inspector.cue.note || "") : ""
                        restoreMode: Binding.RestoreNone
                    }
                    KanteFieldSkin { control: noteArea }
                }
            }

            // Triggers: what starts the next cue, what this one stops.
            GridLayout {
                id: triggers
                columns: 2
                columnSpacing: Kirigami.Units.largeSpacing
                rowSpacing: Kirigami.Units.smallSpacing

                readonly property var follows: ["none", "after_wait", "after_end"]
                readonly property var scopes: [null, "siblings", "list", "all"]
                readonly property var stopOthers: inspector.cue ? (inspector.cue.stop_others || null) : null

                QQC2.Label { text: qsTr("Next cue") }
                KanteSegmented {
                    activeFocusOnTab: false
                    model: [qsTr("Waits for GO"), qsTr("After the post wait"), qsTr("When this one ends")]
                    tooltips: [qsTr("The next cue starts only with GO"), qsTr("The next cue starts when the post wait is over"), qsTr("The next cue starts when this one ends")]
                    currentIndex: inspector.cue ? Math.max(0, triggers.follows.indexOf(inspector.cue.follow)) : 0
                    onActivated: index => {
                        inspector.save({ follow: triggers.follows[index] })
                        inspector.refocus()
                    }
                }

                QQC2.Label { text: qsTr("Stops others") }
                RowLayout {
                    spacing: Kirigami.Units.smallSpacing
                    KanteSegmented {
                        activeFocusOnTab: false
                        model: [qsTr("No"), qsTr("Same group"), qsTr("Same list"), qsTr("All")]
                        tooltips: [qsTr("Other cues play on"), qsTr("Running cues of the same list or group stop when this one starts"),
                                   qsTr("Running cues of the same list stop when this one starts"), qsTr("Every running cue stops when this one starts")]
                        currentIndex: Math.max(0, triggers.scopes.indexOf(triggers.stopOthers ? triggers.stopOthers.scope : null))
                        onActivated: index => {
                            var scope = triggers.scopes[index]
                            inspector.save({ stop_others: scope === null ? null : { scope: scope } })
                            inspector.refocus()
                        }
                    }
                    QQC2.Label {
                        visible: triggers.stopOthers !== null
                        text: qsTr("fading over")
                    }
                    DurationField {
                        visible: triggers.stopOthers !== null
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                        ms: triggers.stopOthers ? triggers.stopOthers.fade_ms : 0
                        maxMs: 600000
                        Accessible.name: qsTr("Fade of the cues it stops")
                        onAccepted: inspector.refocus()
                        onEdited: value => inspector.save({ stop_others: { fade_ms: value } })
                    }
                }
                Item { Layout.fillHeight: true }
            }

            // Time & loops: waits, duration, fades and repeats.
            GridLayout {
                id: timing
                columns: 4
                columnSpacing: Kirigami.Units.largeSpacing
                rowSpacing: Kirigami.Units.smallSpacing

                readonly property var repeat: inspector.media ? (inspector.media.repeat || "once") : "once"
                readonly property int repeatKind: repeat === "forever" ? 2 : (typeof repeat === "object" ? 1 : 0)

                QQC2.Label { text: qsTr("Pre wait") }
                DurationField {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    ms: inspector.cue ? inspector.cue.pre_wait_ms : 0
                    Accessible.name: qsTr("Pre wait")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("From GO to what the cue does")
                    onAccepted: inspector.refocus()
                    onEdited: value => inspector.save({ pre_wait_ms: value })
                }
                QQC2.Label { text: qsTr("Post wait") }
                DurationField {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    ms: inspector.cue ? inspector.cue.post_wait_ms : 0
                    Accessible.name: qsTr("Post wait")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("From what the cue does to the next cue, when it follows after the post wait")
                    onAccepted: inspector.refocus()
                    onEdited: value => inspector.save({ post_wait_ms: value })
                }

                QQC2.Label {
                    visible: inspector.timed
                    text: qsTr("Duration")
                }
                DurationField {
                    visible: inspector.timed
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    optional: inspector.cue !== null && inspector.cue.kind !== "wait"
                    ms: inspector.cue && inspector.cue.duration_ms !== undefined ? inspector.cue.duration_ms : null
                    Accessible.name: qsTr("Duration")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: {
                        switch (inspector.cue ? inspector.cue.kind : "") {
                        case "wait": return qsTr("How long the wait runs")
                        case "fade": return qsTr("How long the fade takes")
                        case "actions": return qsTr("How long it counts as running, e.g. the length of the clip it starts; empty: it ends at once")
                        default: return qsTr("Ends the cue after this time; empty: at the end of its file")
                        }
                    }
                    onAccepted: inspector.refocus()
                    onEdited: value => inspector.save({ duration_ms: value })
                }
                Item {
                    visible: inspector.timed
                    Layout.columnSpan: 2
                }

                QQC2.Label {
                    visible: inspector.media !== null
                    text: qsTr("Fade in")
                }
                DurationField {
                    visible: inspector.media !== null
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    ms: inspector.media ? inspector.media.fade_in_ms : 0
                    maxMs: 600000
                    Accessible.name: qsTr("Fade in")
                    onAccepted: inspector.refocus()
                    onEdited: value => inspector.saveIn(inspector.cue.kind, { fade_in_ms: value })
                }
                QQC2.Label {
                    visible: inspector.media !== null
                    text: qsTr("Fade out")
                }
                DurationField {
                    visible: inspector.media !== null
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                    ms: inspector.media ? inspector.media.fade_out_ms : 0
                    maxMs: 600000
                    Accessible.name: qsTr("Fade out")
                    QQC2.ToolTip.visible: hovered
                    QQC2.ToolTip.text: qsTr("When it is stopped; a panic uses the panic time")
                    onAccepted: inspector.refocus()
                    onEdited: value => inspector.saveIn(inspector.cue.kind, { fade_out_ms: value })
                }

                QQC2.Label {
                    visible: inspector.media !== null
                    text: qsTr("Plays")
                }
                RowLayout {
                    visible: inspector.media !== null
                    Layout.columnSpan: 3
                    spacing: Kirigami.Units.smallSpacing
                    KanteSegmented {
                        id: repeatChoice
                        activeFocusOnTab: false
                        model: [qsTr("Once"), qsTr("Times"), qsTr("Until stopped")]
                        currentIndex: timing.repeatKind
                        onActivated: index => {
                            inspector.saveIn(inspector.cue.kind, { repeat: index === 0 ? "once" : (index === 1 ? { times: 2 } : "forever") })
                            inspector.refocus()
                        }
                    }
                    KanteSpinBox {
                        visible: repeatChoice.currentIndex === 1
                        from: 2
                        to: 10000
                        editable: true
                        value: typeof timing.repeat === "object" ? timing.repeat.times : 2
                        Accessible.name: qsTr("Plays in all")
                        onValueModified: {
                            inspector.saveIn(inspector.cue.kind, { repeat: { times: value } })
                            inspector.refocus()
                        }
                    }
                }
                Item {
                    Layout.columnSpan: 4
                    Layout.fillHeight: true
                }
            }

            // Levels: of a sound, a picture or what a fade changes.
            GridLayout {
                id: levels
                columns: 3
                columnSpacing: Kirigami.Units.largeSpacing
                rowSpacing: Kirigami.Units.smallSpacing

                readonly property string kind: inspector.cue ? inspector.cue.kind : ""
                readonly property var fade: kind === "fade" && inspector.cue.fade ? inspector.cue.fade : null
                readonly property var fadeTarget: fade ? fade.target : null

                /** dB in tenths for a spin box. */
                function db(text) {
                    return Math.round(parseFloat(text.replace(",", ".").replace(/[^0-9.\-]/g, "")) * 10)
                }

                QQC2.Label {
                    Layout.columnSpan: 3
                    visible: ["sound", "picture", "fade"].indexOf(levels.kind) < 0
                    text: qsTr("A cue of this kind has no levels.")
                    color: KanteStyle.mutedTextColor
                }

                // Sound: level of the whole cue.
                QQC2.Label {
                    visible: levels.kind === "sound"
                    text: qsTr("Level")
                }
                KanteSpinBox {
                    visible: levels.kind === "sound"
                    from: -600
                    to: 120
                    stepSize: 5
                    editable: true
                    value: inspector.cue && inspector.cue.sound ? Math.round(inspector.cue.sound.gain_db * 10) : 0
                    textFromValue: v => (v / 10).toFixed(1) + " dB"
                    valueFromText: t => levels.db(t)
                    Accessible.name: qsTr("Level of the sound")
                    onValueModified: {
                        inspector.saveIn("sound", { gain_db: value / 10 })
                        inspector.refocus()
                    }
                }
                QQC2.Label {
                    visible: levels.kind === "sound"
                    text: qsTr("0 dB: as in the file; −60 dB: silence")
                    color: KanteStyle.mutedTextColor
                }

                // Picture: opacity and layer.
                QQC2.Label {
                    visible: levels.kind === "picture"
                    text: qsTr("Opacity")
                }
                KanteSpinBox {
                    visible: levels.kind === "picture"
                    from: 0
                    to: 100
                    stepSize: 5
                    editable: true
                    value: inspector.cue && inspector.cue.picture ? Math.round(inspector.cue.picture.opacity * 100) : 100
                    textFromValue: v => v + " %"
                    valueFromText: t => parseInt(t)
                    Accessible.name: qsTr("Opacity of the picture")
                    onValueModified: {
                        inspector.saveIn("picture", { opacity: value / 100 })
                        inspector.refocus()
                    }
                }
                Item { visible: levels.kind === "picture" }
                QQC2.Label {
                    visible: levels.kind === "picture"
                    text: qsTr("Layer")
                }
                KanteSpinBox {
                    visible: levels.kind === "picture"
                    from: 0
                    to: 999
                    editable: true
                    value: inspector.cue && inspector.cue.picture ? inspector.cue.picture.layer : 1
                    Accessible.name: qsTr("Layer of the picture")
                    onValueModified: {
                        inspector.saveIn("picture", { layer: value })
                        inspector.refocus()
                    }
                }
                QQC2.Label {
                    visible: levels.kind === "picture"
                    text: qsTr("On the same layer the later cue is in front")
                    color: KanteStyle.mutedTextColor
                }

                // Fade: only the values that are ticked change. A fixed
                // model: a new state does not rebuild the rows (and the
                // box being typed in).
                Repeater {
                    model: [
                        { key: "gain_db", label: qsTr("Level"), from: -600, to: 120, scale: 10, unit: " dB", fallback: 0 },
                        { key: "opacity", label: qsTr("Opacity"), from: 0, to: 100, scale: 100, unit: " %", fallback: 1 },
                        { key: "brightness", label: qsTr("Brightness"), from: 0, to: 100, scale: 100, unit: " %", fallback: 1 }
                    ]

                    delegate: RowLayout {
                        id: fadeLevel
                        required property var modelData
                        readonly property bool set: levels.fade !== null && levels.fade[modelData.key] !== undefined && levels.fade[modelData.key] !== null
                        // Level for a cue or zone, opacity for a cue,
                        // brightness for a stage.
                        visible: levels.fadeTarget !== null && (modelData.key === "gain_db" ? levels.fadeTarget.stage === undefined
                                                                : modelData.key === "opacity" ? levels.fadeTarget.cue !== undefined
                                                                : levels.fadeTarget.stage !== undefined)
                        Layout.columnSpan: 3
                        spacing: Kirigami.Units.largeSpacing

                        QQC2.CheckBox {
                            id: fadeCheck
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                            text: fadeLevel.modelData.label
                            checked: fadeLevel.set
                            focusPolicy: Qt.NoFocus
                            onToggled: {
                                var change = {}
                                change[fadeLevel.modelData.key] = checked ? fadeLevel.modelData.fallback : null
                                inspector.saveIn("fade", change)
                            }
                            KanteCheckSkin { control: fadeCheck }
                        }
                        KanteSpinBox {
                            enabled: fadeLevel.set
                            from: fadeLevel.modelData.from
                            to: fadeLevel.modelData.to
                            stepSize: 5
                            editable: true
                            value: fadeLevel.set ? Math.round(levels.fade[fadeLevel.modelData.key] * fadeLevel.modelData.scale)
                                                 : Math.round(fadeLevel.modelData.fallback * fadeLevel.modelData.scale)
                            textFromValue: v => (fadeLevel.modelData.scale === 10 ? (v / 10).toFixed(1) : String(v)) + fadeLevel.modelData.unit
                            valueFromText: t => fadeLevel.modelData.scale === 10 ? levels.db(t) : parseInt(t)
                            Accessible.name: qsTr("%1 at the end of the fade").arg(fadeLevel.modelData.label)
                            onValueModified: {
                                var change = {}
                                change[fadeLevel.modelData.key] = value / fadeLevel.modelData.scale
                                inspector.saveIn("fade", change)
                                inspector.refocus()
                            }
                        }
                    }
                }
                QQC2.CheckBox {
                    id: stopAfter
                    Layout.columnSpan: 3
                    visible: levels.fadeTarget !== null && levels.fadeTarget.cue !== undefined
                    text: qsTr("Stop the cue when the fade is over")
                    checked: levels.fade !== null && levels.fade.then === "stop"
                    focusPolicy: Qt.NoFocus
                    onToggled: inspector.saveIn("fade", { then: checked ? "stop" : "keep" })
                    KanteCheckSkin { control: stopAfter }
                }
                Item {
                    Layout.columnSpan: 3
                    Layout.fillHeight: true
                }
            }

            // Actions of an actions cue.
            ColumnLayout {
                id: actionsTab
                spacing: Kirigami.Units.smallSpacing

                readonly property var actions: inspector.cue && inspector.cue.kind === "actions" ? (inspector.cue.actions || []) : null

                QQC2.Label {
                    visible: actionsTab.actions === null
                    text: qsTr("Only an actions cue has actions.")
                    color: KanteStyle.mutedTextColor
                }
                Repeater {
                    model: actionsTab.actions === null ? [] : actionsTab.actions.slice(0, 6)
                    QQC2.Label {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        text: (index + 1) + ". " + modelData.action
                        font.family: KanteStyle.monoFamily
                        elide: Text.ElideRight
                    }
                }
                QQC2.Label {
                    visible: actionsTab.actions !== null && actionsTab.actions.length > 6
                    text: actionsTab.actions ? qsTr("… and %1 more").arg(actionsTab.actions.length - 6) : ""
                    color: KanteStyle.mutedTextColor
                }
                KanteButton {
                    visible: actionsTab.actions !== null
                    focusPolicy: Qt.NoFocus
                    text: qsTr("Edit actions…")
                    onClicked: {
                        actionDialog.actions = inspector.cue.actions || []
                        actionDialog.open()
                    }
                }
                Item { Layout.fillHeight: true }
            }
        }
    }

    ActionListDialog {
        id: actionDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        inputs: inspector.showView.show ? inspector.showView.show.inputs : []
        outputs: inspector.showView.show ? inspector.showView.show.outputs : []
        needsAction: true
        title: inspector.cue ? qsTr("Actions of %1").arg(inspector.showView.cueName(inspector.cue)) : ""
        onDone: (label, actions) => inspector.save({ actions: actions })
        onClosed: inspector.refocus()
    }
}
