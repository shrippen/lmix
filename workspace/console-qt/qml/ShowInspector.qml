import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Inspector under the cue list (edit mode): the chosen cue's fields on
 * tabs, Basics (with the file, stage or what a fade fades), Triggers,
 * Time & loops, Levels (with a sound's crosspoints) and Actions. Each change
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
    readonly property bool holding: actionDialog.visible || colourPicker.popover.visible || fileField.picking
                                    || hasFocusInside(Window.activeFocusItem)
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

    /** What a fade can fade, by kind: [{ref, label, kind}]. */
    readonly property var fadeKinds: ["cue", "zone", "stage"]
    readonly property var fadeChoices: {
        var show = showView.show
        return {
            cue: showView.list ? mediaCues(showView.list.cues) : [],
            zone: (show && show.zones ? show.zones : []).map(z => ({ ref: z.name, label: z.name })),
            stage: (show && show.stages ? show.stages : []).map(s => ({ ref: s.name, label: s.name }))
        }
    }
    readonly property var stageNames: showView.show && showView.show.stages ? showView.show.stages.map(s => s.name) : []

    /** The sound and picture cues of `cues` and their groups but this one. */
    function mediaCues(cues) {
        var out = []
        function walk(list) {
            list.forEach(c => {
                if ((c.kind === "sound" || c.kind === "picture") && (cue === null || c.id !== cue.id)) {
                    var ref = c.number !== "" ? c.number : c.id
                    out.push({ ref: ref, label: (c.number !== "" ? c.number + "  " : "") + showView.cueName(c), kind: c.kind })
                }
                walk(c.children || [])
            })
        }
        walk(cues)
        return out
    }

    /** The fade's target as {kind, ref}, or null. */
    function fadeTarget(fade) {
        var target = fade ? fade.target : null
        if (!target) {
            return null
        }
        var kind = fadeKinds.find(k => target[k] !== undefined)
        return kind ? { kind: kind, ref: target[kind] } : null
    }

    /** A merge patch of `fade` for a new target: the old target goes
     *  (null), and only the value the new one takes stays, set as in the
     *  Levels tab when it was not: a level for a zone or a sound, an
     *  opacity for a picture, a brightness for a stage; only a cue stops
     *  after its fade. `mediaKind` is the target cue's kind. */
    function fadePatch(fade, kind, ref, mediaKind) {
        var old = fade || {}
        var has = key => old[key] !== undefined && old[key] !== null
        var target = { cue: null, zone: null, stage: null }
        target[kind] = ref
        var out = { target: target, gain_db: null, opacity: null, brightness: null, then: "keep" }
        if (kind === "zone") {
            out.gain_db = has("gain_db") ? old.gain_db : 0
        } else if (kind === "stage") {
            out.brightness = has("brightness") ? old.brightness : 1
        } else if (mediaKind === "picture") {
            out.opacity = has("opacity") ? old.opacity : 1
            out.then = old.then || "keep"
        } else {
            out.gain_db = has("gain_db") ? old.gain_db : 0
            out.then = old.then || "keep"
        }
        return out
    }

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
            // A spin box takes its text when it loses the keys: bind it to
            // the value again first (shown at once, and still after).
            if (item.valueFromText !== undefined && item.contentItem) {
                item.contentItem.text = Qt.binding(() => item.textFromValue(item.value, item.locale))
                break
            }
        }
        refocus()
        showView.panic()
        event.accepted = true
    }
    // Enter that no field keeps (a spin box whose value did not change)
    // gives the keys back to the show too.
    Keys.onReturnPressed: event => {
        refocus()
        event.accepted = true
    }
    Keys.onEnterPressed: event => {
        refocus()
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
                CueColourButton {
                    id: colourPicker
                    tags: JSON.parse(inspector.core.choices("colors"))
                    current: inspector.cue ? (inspector.cue.color || null) : null
                    onPicked: value => inspector.save({ color: value })
                }

                // The file of a sound or picture.
                QQC2.Label {
                    visible: inspector.media !== null
                    text: qsTr("File")
                }
                PathField {
                    id: fileField
                    visible: inspector.media !== null
                    Layout.columnSpan: 3
                    Layout.fillWidth: true
                    key: inspector.cue ? inspector.cue.id : ""
                    path: inspector.media ? inspector.media.file : ""
                    title: inspector.cue && inspector.cue.kind === "picture" ? qsTr("Video or still") : qsTr("Sound file")
                    nameFilters: inspector.cue && inspector.cue.kind === "picture"
                        ? [qsTr("Videos and stills (*.mp4 *.mov *.mkv *.webm *.png *.jpg *.jpeg)"), qsTr("All files (*)")]
                        : [qsTr("Sound files (*.wav *.flac *.ogg *.opus *.mp3 *.aif *.aiff *.m4a)"), qsTr("All files (*)")]
                    onAccepted: inspector.refocus()
                    onEdited: path => inspector.saveIn(inspector.cue.kind, { file: path })
                }

                // The stage of a picture (VIS.2).
                QQC2.Label {
                    visible: inspector.cue !== null && inspector.cue.kind === "picture"
                    text: qsTr("Stage")
                }
                KanteComboBox {
                    id: stageBox
                    visible: inspector.cue !== null && inspector.cue.kind === "picture"
                    Layout.columnSpan: 3
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                    focusPolicy: Qt.NoFocus
                    readonly property string stage: inspector.cue && inspector.cue.picture ? inspector.cue.picture.stage : ""
                    // A stage that is gone stays named, marked.
                    readonly property bool missing: inspector.stageNames.indexOf(stage) < 0
                    model: missing ? [stage].concat(inspector.stageNames) : inspector.stageNames
                    currentIndex: missing ? 0 : inspector.stageNames.indexOf(stage)
                    invalid: missing
                    Accessible.name: qsTr("Stage of the picture")
                    onActivated: index => {
                        inspector.saveIn("picture", { stage: model[index] })
                        inspector.refocus()
                    }
                }

                // What a fade fades (SND.6, VIS.4, VIS.7).
                QQC2.Label {
                    visible: inspector.cue !== null && inspector.cue.kind === "fade"
                    text: qsTr("Fades")
                }
                RowLayout {
                    id: fadeTargetRow
                    visible: inspector.cue !== null && inspector.cue.kind === "fade"
                    Layout.columnSpan: 3
                    spacing: Kirigami.Units.smallSpacing

                    readonly property var target: inspector.cue ? inspector.fadeTarget(inspector.cue.fade) : null
                    readonly property string kind: target ? target.kind : "cue"
                    readonly property var choices: inspector.fadeChoices[kind]
                    readonly property var refs: choices.map(c => c.ref)
                    readonly property bool missing: target !== null && refs.indexOf(target.ref) < 0

                    /** Fades `ref` of `kind` from now on. */
                    function retarget(kind, ref) {
                        var choice = inspector.fadeChoices[kind].find(c => c.ref === ref)
                        inspector.saveIn("fade", inspector.fadePatch(inspector.cue.fade, kind, ref, choice ? choice.kind : ""))
                        inspector.refocus()
                    }

                    KanteSegmented {
                        id: fadeKind
                        activeFocusOnTab: false
                        model: [qsTr("Cue"), qsTr("Zone"), qsTr("Stage")]
                        tooltips: [qsTr("A sound or picture cue: its level or opacity"), qsTr("A zone: its level"), qsTr("A stage: its brightness")]
                        Binding on currentIndex { value: inspector.fadeKinds.indexOf(fadeTargetRow.kind) }
                        onActivated: index => {
                            var kind = inspector.fadeKinds[index]
                            var first = inspector.fadeChoices[kind][0]
                            if (first === undefined) {
                                // Nothing of that kind: the target stays.
                                currentIndex = inspector.fadeKinds.indexOf(fadeTargetRow.kind)
                                inspector.refocus()
                                return
                            }
                            fadeTargetRow.retarget(kind, first.ref)
                        }
                    }
                    KanteComboBox {
                        id: fadeTargetBox
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 12
                        focusPolicy: Qt.NoFocus
                        // A target that is gone stays named, marked.
                        model: (fadeTargetRow.missing ? [fadeTargetRow.target.ref] : []).concat(fadeTargetRow.choices.map(c => c.label))
                        currentIndex: fadeTargetRow.target === null ? -1
                            : (fadeTargetRow.missing ? 0 : fadeTargetRow.refs.indexOf(fadeTargetRow.target.ref))
                        invalid: fadeTargetRow.missing
                        Accessible.name: qsTr("What the fade fades")
                        onActivated: index => fadeTargetRow.retarget(fadeTargetRow.kind, fadeTargetRow.refs[index - (fadeTargetRow.missing ? 1 : 0)])
                    }
                    QQC2.Label {
                        visible: fadeTargetRow.choices.length === 0
                        text: qsTr("The show has none yet.")
                        color: KanteStyle.mutedTextColor
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
                    // KanteSegmented sets currentIndex on a click, which would end a
                    // plain binding: this one follows the cue still.
                    Binding on currentIndex { value: inspector.cue ? Math.max(0, triggers.follows.indexOf(inspector.cue.follow)) : 0 }
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
                        Binding on currentIndex { value: Math.max(0, triggers.scopes.indexOf(triggers.stopOthers ? triggers.stopOthers.scope : null)) }
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
                        key: inspector.cue ? inspector.cue.id : ""
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
                    key: inspector.cue ? inspector.cue.id : ""
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
                    key: inspector.cue ? inspector.cue.id : ""
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
                    key: inspector.cue ? inspector.cue.id : ""
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
                    key: inspector.cue ? inspector.cue.id : ""
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
                    key: inspector.cue ? inspector.cue.id : ""
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
                        Binding on currentIndex { value: timing.repeatKind }
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

                // Sound: where its channels go (SND.5).
                QQC2.ScrollView {
                    id: routesView
                    visible: levels.kind === "sound"
                    Layout.columnSpan: 3
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    // The rows' width: a layout without it would stay as
                    // narrow as the level row above.
                    Layout.minimumWidth: soundRoutes.implicitWidth
                    SoundRoutes {
                        id: soundRoutes
                        key: inspector.cue ? inspector.cue.id : ""
                        routes: inspector.cue && inspector.cue.sound ? (inspector.cue.sound.routes || []) : []
                        zones: inspector.showView.show && inspector.showView.show.zones ? inspector.showView.show.zones : []
                        core: inspector.core
                        onEdited: routes => inspector.saveIn("sound", { routes: routes })
                        onFinished: inspector.refocus()
                    }
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
                /** How many actions the preview lists. */
                readonly property int shownActions: 6

                QQC2.Label {
                    visible: actionsTab.actions === null
                    text: qsTr("Only an actions cue has actions.")
                    color: KanteStyle.mutedTextColor
                }
                Repeater {
                    model: actionsTab.actions === null ? [] : actionsTab.actions.slice(0, actionsTab.shownActions)
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
                    visible: actionsTab.actions !== null && actionsTab.actions.length > actionsTab.shownActions
                    text: actionsTab.actions ? qsTr("… and %1 more").arg(actionsTab.actions.length - actionsTab.shownActions) : ""
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
