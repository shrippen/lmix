pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Edits the moods of the show (MOOD.12): groups with base and variants,
 * the layers and change times, collections, and as an advanced tab the
 * panic mood (MOOD.9). Every edit goes to lmix-core at once as the whole
 * (`set_moods`), which checks it; a refused one leaves the moods as they
 * were.
 *
 *   MOODS | LAYERS | COLLECTIONS | PANIC MOOD
 *   ▌Tavern          │ Name   [Tavern      ]  Colour [■]
 *     › Night        │ Light  [Actions ▾] [Actions…]
 *     › Fight        │ Music  [Sound   ▾] [/sounds/lute.wav] [Choose…]
 *   [+ Group] [+ Variant] [Up] [Down] [Remove]
 */
KanteDialog {
    id: dialog

    required property var act
    /** The show (state): moods, stages, inputs, outputs. */
    required property var show
    /** The core, for the colour names. */
    required property var core

    readonly property var moods: show && show.moods ? show.moods : ({})
    readonly property var layers: moods.layers || []
    readonly property var groups: moods.groups || []
    readonly property var collections: moods.collections || []
    readonly property var stages: show && show.stages ? show.stages.map(s => s.name) : []

    /** Groups and variants in order: [{ id, group, variant }]. */
    readonly property var places: {
        const out = []
        groups.forEach(g => {
            out.push({ id: g.id, group: g, variant: null })
            ;(g.variants || []).forEach(v => out.push({ id: v.id, group: g, variant: v }))
        })
        return out
    }
    /** The group or variant being edited, by id. */
    property string chosenId: ""
    /** Layers of the chosen mood whose new sound or picture waits for its
        file, with what is chosen so far: { layer id: content }. */
    property var pending: ({})
    readonly property var chosen: places.find(p => p.id === chosenId) || null
    property int tab: 0

    onChosenIdChanged: pending = {}

    /** What an action list dialog edits: { where: "mood" | "panic", layer }. */
    property var editingActions: null
    /** The moods before the last removal, for Undo; null after any other edit. */
    property var undoMoods: null
    property string undoText: ""

    onClosed: {
        pending = {}
        undoMoods = null
    }

    title: qsTr("Moods")
    modal: true
    width: Kirigami.Units.gridUnit * 46
    standardButtons: QQC2.Dialog.Close
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    /** A random id (UUID version 4). */
    function newId() {
        return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, c => {
            const r = Math.floor(Math.random() * 16)
            return (c === "x" ? r : (r % 4) + 8).toString(16)
        })
    }
    /** Changes a copy of the moods with `change` and sends it whole. */
    function edit(change) {
        undoMoods = null
        const copy = Object.assign({ layers: [], groups: [], collections: [], panic: {}, change_ms: 3000, inside_ms: 1000 },
                                   JSON.parse(JSON.stringify(moods)))
        change(copy)
        act({ action: "set_moods", moods: copy })
    }
    /** Asks before a removal (docs/DESIGN-DIALOGE.md rule 1), then edits
        with `request.change` and offers Undo: lmix-core keeps no way back
        for the moods, so Undo sends them as they were. */
    function askRemove(request) {
        removeDialog.request = request
        removeDialog.open()
    }
    function remove(request) {
        const before = JSON.parse(JSON.stringify(moods))
        edit(request.change)
        undoMoods = before
        undoText = request.removedText
    }
    function undo() {
        const before = undoMoods
        undoMoods = null
        act({ action: "set_moods", moods: before })
    }
    /** In `copy`: the group of `place`, and its variant or null. */
    function find(copy, place) {
        const group = copy.groups.find(g => g.id === place.group.id)
        return { group: group, variant: place.variant ? group.variants.find(v => v.id === place.variant.id) : null }
    }
    /** A name not yet among `names`: "Name", "Name 2" … */
    function freeName(base, names) {
        let name = base
        // lmix-core compares names without case.
        const taken = names.map(n => n.toLowerCase())
        for (let n = 2; taken.indexOf(name.toLowerCase()) >= 0; n++) {
            name = base + " " + n
        }
        return name
    }
    /** What the chosen mood has on `layerId` itself; undefined: from the base. */
    function own(layerId) {
        if (!chosen) {
            return undefined
        }
        return chosen.variant ? (chosen.variant.layers || {})[layerId] : (chosen.group.base || {})[layerId]
    }
    /** Contents to choose from, for a layer that has `current`; a variant also takes the base's. */
    function kinds(current) {
        const out = [{ kind: "off", label: qsTr("Off") }, { kind: "sound", label: qsTr("Sound") }]
        // A picture names its stage.
        if (stages.length > 0 || current === "picture") {
            out.push({ kind: "picture", label: qsTr("Picture") })
        }
        out.push({ kind: "actions", label: qsTr("Actions") })
        return chosen && chosen.variant ? [{ kind: "base", label: qsTr("From the base") }].concat(out) : out
    }
    /** Sets what the chosen mood has on `layerId`; null takes the base's. */
    function setContent(layerId, content) {
        const place = chosen
        waitFor(layerId, null)
        edit(copy => {
            const at = find(copy, place)
            const map = at.variant ? (at.variant.layers = at.variant.layers || {}) : (at.group.base = at.group.base || {})
            if (content === null) {
                delete map[layerId]
            } else {
                map[layerId] = content
            }
        })
    }
    /** A new, empty content of `kind`; sound and picture loop, like an ambience. */
    function fresh(kind) {
        switch (kind) {
        case "sound": return { kind: "sound", sound: { file: "", repeat: "forever" } }
        case "picture": return { kind: "picture", picture: { file: "", stage: stages.length > 0 ? stages[0] : "", repeat: "forever" } }
        case "actions": return { kind: "actions", actions: [] }
        default: return { kind: kind }
        }
    }
    /** Keeps `content` waiting for its file on `layerId`; null: nothing waits. */
    function waitFor(layerId, content) {
        const next = Object.assign({}, pending)
        if (content === null) {
            delete next[layerId]
        } else {
            next[layerId] = content
        }
        pending = next
    }
    /** A changed sound or picture: out with its file, else it waits on. */
    function changeMedia(layerId, content) {
        if (content[content.kind].file === "") {
            waitFor(layerId, content)
        } else {
            setContent(layerId, content)
        }
    }
    /** A new content of `kind` for a layer; sound and picture wait for their file, actions ask for theirs. */
    function chooseKind(layerId, kind) {
        switch (kind) {
        case "base": setContent(layerId, null); break
        case "off": setContent(layerId, { kind: "off" }); break
        case "sound": case "picture": {
            // Back to what is kept: nothing waits.
            const stored = own(layerId)
            waitFor(layerId, stored !== undefined && stored.kind === kind ? null : fresh(kind))
            break
        }
        case "actions": waitFor(layerId, null); openActions("mood", layerId, []); break
        }
    }
    function openActions(where, layerId, actions) {
        editingActions = { where: where, layer: layerId }
        actionsDialog.actions = actions
        actionsDialog.open()
    }
    function move(list, index, by) {
        const to = index + by
        if (to >= 0 && to < list.length) {
            list.splice(to, 0, list.splice(index, 1)[0])
        }
    }

    ActionListDialog {
        id: actionsDialog
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        inputs: dialog.show && dialog.show.inputs ? dialog.show.inputs : []
        outputs: dialog.show && dialog.show.outputs ? dialog.show.outputs : []
        needsAction: true
        title: qsTr("Actions of the layer")
        onDone: (label, actions) => {
            const what = dialog.editingActions
            const content = { kind: "actions", actions: actions }
            if (what.where === "panic") {
                dialog.edit(copy => { copy.panic[what.layer] = content })
            } else {
                dialog.setContent(what.layer, content)
            }
        }
    }

    KanteDialog {
        id: removeDialog
        objectName: "removeDialog"
        property var request: null
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: request ? request.question : ""
        destructive: true
        size: KanteDialog.Size.Small
        standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
        acceptText: request ? request.acceptText : ""
        // KanteDialog has a child of its own, so it does not size to this.
        contentHeight: consequence.implicitHeight
        onOpened: consequence.forceActiveFocus()
        onAccepted: dialog.remove(request)

        QQC2.Label {
            id: consequence
            width: parent.width
            text: removeDialog.request ? removeDialog.request.consequence : ""
            wrapMode: Text.WordWrap
            // Enter keeps it: removing takes the button.
            focus: true
            Keys.onReturnPressed: removeDialog.reject()
            Keys.onEnterPressed: removeDialog.reject()
        }
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        // After a removal: what went, and the way back (rule 1).
        RowLayout {
            visible: dialog.undoMoods !== null
            Layout.fillWidth: true
            QQC2.Label {
                Layout.fillWidth: true
                elide: Text.ElideRight
                text: dialog.undoText
            }
            KanteButton {
                objectName: "undo"
                text: qsTr("Undo")
                onClicked: dialog.undo()
            }
        }

        KanteTabBar {
            Layout.fillWidth: true
            model: [qsTr("Moods"), qsTr("Layers"), qsTr("Collections"), qsTr("Panic mood")]
            currentIndex: dialog.tab
            onActivated: index => dialog.tab = index
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 22
            currentIndex: dialog.tab

            // Groups and variants: the list left, the chosen one right.
            RowLayout {
                spacing: Kirigami.Units.largeSpacing

                ColumnLayout {
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 14
                    Layout.fillHeight: true

                    ListView {
                        objectName: "places"
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        model: dialog.places
                        boundsBehavior: Flickable.StopAtBounds
                        QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                        delegate: KanteListRow {
                            required property var modelData
                            objectName: "place:" + (modelData.variant ? modelData.group.name + "/" + modelData.variant.name : modelData.group.name)
                            width: ListView.view.width
                            density: KanteListRow.Density.Compact
                            leadingText: modelData.variant ? "›" : ""
                            leadingWidth: Kirigami.Units.gridUnit
                            text: modelData.variant ? modelData.variant.name : modelData.group.name
                            selected: dialog.chosenId === modelData.id
                            onClicked: dialog.chosenId = modelData.id
                        }
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        KanteButton {
                            objectName: "addGroup"
                            text: qsTr("+ Group")
                            onClicked: {
                                const id = dialog.newId()
                                const name = dialog.freeName(qsTr("Group"), dialog.groups.map(g => g.name))
                                dialog.edit(copy => copy.groups.push({ id: id, name: name, base: {}, variants: [] }))
                                dialog.chosenId = id
                            }
                        }
                        KanteButton {
                            objectName: "addVariant"
                            text: qsTr("+ Variant")
                            enabled: dialog.chosen !== null
                            onClicked: {
                                const place = dialog.chosen
                                const id = dialog.newId()
                                const name = dialog.freeName(qsTr("Variant"), (place.group.variants || []).map(v => v.name))
                                dialog.edit(copy => {
                                    const group = dialog.find(copy, place).group
                                    group.variants = group.variants || []
                                    group.variants.push({ id: id, name: name, layers: {} })
                                })
                                dialog.chosenId = id
                            }
                        }
                        KanteButton {
                            text: qsTr("Up")
                            enabled: dialog.chosen !== null
                            onClicked: {
                                const place = dialog.chosen
                                dialog.edit(copy => {
                                    const at = dialog.find(copy, place)
                                    if (at.variant) {
                                        dialog.move(at.group.variants, at.group.variants.indexOf(at.variant), -1)
                                    } else {
                                        dialog.move(copy.groups, copy.groups.indexOf(at.group), -1)
                                    }
                                })
                            }
                        }
                        KanteButton {
                            text: qsTr("Down")
                            enabled: dialog.chosen !== null
                            onClicked: {
                                const place = dialog.chosen
                                dialog.edit(copy => {
                                    const at = dialog.find(copy, place)
                                    if (at.variant) {
                                        dialog.move(at.group.variants, at.group.variants.indexOf(at.variant), 1)
                                    } else {
                                        dialog.move(copy.groups, copy.groups.indexOf(at.group), 1)
                                    }
                                })
                            }
                        }
                        KanteButton {
                            objectName: "removePlace"
                            text: qsTr("Remove")
                            emphasis: KanteButton.Emphasis.Destructive
                            enabled: dialog.chosen !== null
                            onClicked: {
                                const place = dialog.chosen
                                const gone = place.variant ? [place.variant.id] : [place.group.id].concat((place.group.variants || []).map(v => v.id))
                                const variants = place.variant ? 0 : (place.group.variants || []).length
                                const named = "\u201c" + (place.variant ? place.group.name + " / " + place.variant.name : place.group.name) + "\u201d"
                                dialog.askRemove({
                                    question: place.variant ? qsTr("Remove variant %1?").arg(named) : qsTr("Remove group %1?").arg(named),
                                    consequence: (variants > 0 ? qsTr("Its variants go with it (%1). ").arg(variants) : "")
                                        + qsTr("Collections forget it; mood cues that set it are marked."),
                                    acceptText: place.variant ? qsTr("Remove variant") : qsTr("Remove group"),
                                    removedText: place.variant ? qsTr("Variant %1 removed").arg(named) : qsTr("Group %1 removed").arg(named),
                                    change: copy => {
                                        const at = dialog.find(copy, place)
                                        if (at.variant) {
                                            at.group.variants.splice(at.group.variants.indexOf(at.variant), 1)
                                        } else {
                                            copy.groups.splice(copy.groups.indexOf(at.group), 1)
                                        }
                                        copy.collections.forEach(c => c.moods = (c.moods || []).filter(m => gone.indexOf(m) < 0))
                                        dialog.chosenId = ""
                                    }
                                })
                            }
                        }
                    }
                }

                // The chosen mood: its name, a group's colour, each layer.
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: Kirigami.Units.smallSpacing

                    QQC2.Label {
                        visible: dialog.chosen === null
                        Layout.fillWidth: true
                        wrapMode: Text.WordWrap
                        color: KanteStyle.mutedTextColor
                        text: dialog.layers.length === 0
                            ? qsTr("Add layers first (Layers tab): what a mood sets, e.g. light, music, ambience.")
                            : qsTr("Choose a group or variant, or add one.")
                    }
                    RowLayout {
                        visible: dialog.chosen !== null
                        Layout.fillWidth: true
                        QQC2.Label { text: qsTr("Name") }
                        KanteTextField {
                            id: nameField
                            objectName: "moodName"
                            Layout.fillWidth: true
                            text: dialog.chosen ? (dialog.chosen.variant ? dialog.chosen.variant.name : dialog.chosen.group.name) : ""
                            onEditingFinished: {
                                const place = dialog.chosen
                                const name = text.trim()
                                const was = place ? (place.variant ? place.variant.name : place.group.name) : ""
                                if (place && name !== "" && name !== was) {
                                    dialog.edit(copy => { const at = dialog.find(copy, place); (at.variant || at.group).name = name })
                                }
                                // The kept name again; a refused one does not stay.
                                text = Qt.binding(() => dialog.chosen ? (dialog.chosen.variant ? dialog.chosen.variant.name : dialog.chosen.group.name) : "")
                            }
                        }
                        CueColourButton {
                            visible: dialog.chosen !== null && dialog.chosen.variant === null
                            tags: dialog.core ? JSON.parse(dialog.core.choices("colors")) : []
                            current: dialog.chosen && dialog.chosen.group.color ? dialog.chosen.group.color : null
                            onPicked: value => {
                                const place = dialog.chosen
                                dialog.edit(copy => { dialog.find(copy, place).group.color = value })
                            }
                        }
                    }
                    QQC2.ScrollView {
                        visible: dialog.chosen !== null
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        contentWidth: availableWidth

                        ColumnLayout {
                            width: parent.width
                            spacing: Kirigami.Units.smallSpacing

                            Repeater {
                                model: dialog.chosen ? dialog.layers : []
                                delegate: RowLayout {
                                    id: layerEdit
                                    required property var modelData
                                    readonly property var stored: dialog.own(modelData.id)
                                    // Sound and picture wait for their file before they go out.
                                    readonly property var waiting: dialog.pending[modelData.id] || null
                                    readonly property string kind: waiting ? waiting.kind
                                        : stored === undefined ? (dialog.chosen && dialog.chosen.variant ? "base" : "off") : stored.kind
                                    readonly property var content: waiting ? waiting
                                        : stored !== undefined && stored.kind === kind ? stored : dialog.fresh(kind)
                                    /** The sound or picture, while it is one. */
                                    readonly property var media: kind === "sound" || kind === "picture" ? content[kind] || null : null
                                    Layout.fillWidth: true
                                    spacing: Kirigami.Units.smallSpacing

                                    QQC2.Label {
                                        Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                                        elide: Text.ElideRight
                                        text: layerEdit.modelData.name
                                    }
                                    KanteComboBox {
                                        objectName: "kind:" + layerEdit.modelData.name
                                        Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                                        readonly property var choices: dialog.kinds(layerEdit.kind)
                                        model: choices.map(c => c.label)
                                        currentIndex: choices.findIndex(c => c.kind === layerEdit.kind)
                                        Accessible.name: layerEdit.modelData.name
                                        onActivated: index => {
                                            if (choices[index].kind !== layerEdit.kind) {
                                                dialog.chooseKind(layerEdit.modelData.id, choices[index].kind)
                                            }
                                            // Shows what is kept, also when the actions are cancelled.
                                            currentIndex = Qt.binding(() => choices.findIndex(c => c.kind === layerEdit.kind))
                                        }
                                    }
                                    PathField {
                                        objectName: "file:" + layerEdit.modelData.name
                                        Layout.fillWidth: true
                                        visible: layerEdit.kind === "sound" || layerEdit.kind === "picture"
                                        key: dialog.chosenId + layerEdit.modelData.id + layerEdit.kind
                                        path: layerEdit.media ? layerEdit.media.file : ""
                                        title: layerEdit.kind === "picture" ? qsTr("Video or still") : qsTr("Sound file")
                                        nameFilters: layerEdit.kind === "picture"
                                            ? [qsTr("Videos and stills (*.mp4 *.mov *.mkv *.webm *.png *.jpg *.jpeg)"), qsTr("All files (*)")]
                                            : [qsTr("Sound files (*.wav *.flac *.ogg *.opus *.mp3 *.aif *.aiff *.m4a)"), qsTr("All files (*)")]
                                        onEdited: path => {
                                            const next = JSON.parse(JSON.stringify(layerEdit.content))
                                            next[layerEdit.kind].file = path
                                            dialog.changeMedia(layerEdit.modelData.id, next)
                                        }
                                    }
                                    KanteComboBox {
                                        visible: layerEdit.kind === "picture" && dialog.stages.length > 1
                                        Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                                        objectName: "stage:" + layerEdit.modelData.name
                                        model: dialog.stages
                                        currentIndex: layerEdit.kind === "picture" && layerEdit.media ? dialog.stages.indexOf(layerEdit.media.stage) : -1
                                        Accessible.name: qsTr("Stage of the picture")
                                        onActivated: index => {
                                            const next = JSON.parse(JSON.stringify(layerEdit.content))
                                            next.picture.stage = dialog.stages[index]
                                            dialog.changeMedia(layerEdit.modelData.id, next)
                                        }
                                    }
                                    QQC2.CheckBox {
                                        visible: layerEdit.media !== null
                                        text: qsTr("Loop")
                                        checked: layerEdit.media !== null && layerEdit.media.repeat === "forever"
                                        onToggled: {
                                            const next = JSON.parse(JSON.stringify(layerEdit.content))
                                            next[layerEdit.kind].repeat = checked ? "forever" : "once"
                                            dialog.changeMedia(layerEdit.modelData.id, next)
                                        }
                                    }
                                    KanteButton {
                                        visible: layerEdit.kind === "actions"
                                        text: layerEdit.kind === "actions" && layerEdit.content.actions ? qsTr("Actions: %1…").arg(layerEdit.content.actions.length) : ""
                                        onClicked: dialog.openActions("mood", layerEdit.modelData.id, layerEdit.content.actions)
                                    }
                                    Item { Layout.fillWidth: true; visible: layerEdit.kind === "off" || layerEdit.kind === "base" }
                                }
                            }
                        }
                    }
                }
            }

            // Layers and the change times (MOOD.1).
            ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                GridLayout {
                    Layout.fillWidth: true
                    columns: 2
                    QQC2.Label { text: qsTr("Change time") }
                    DurationField {
                        key: "change"
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                        ms: dialog.moods.change_ms !== undefined ? dialog.moods.change_ms : 3000
                        maxMs: 600000
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: qsTr("How long a layer that changes fades, unless it has its own")
                        onEdited: value => dialog.edit(copy => copy.change_ms = value)
                    }
                    QQC2.Label { text: qsTr("Within a group") }
                    DurationField {
                        key: "inside"
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                        ms: dialog.moods.inside_ms !== undefined ? dialog.moods.inside_ms : 1000
                        maxMs: 600000
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: qsTr("The change time between the base and the variants of one group")
                        onEdited: value => dialog.edit(copy => copy.inside_ms = value)
                    }
                }
                KanteSectionLabel { text: qsTr("Layers") }
                Repeater {
                    model: dialog.layers
                    delegate: RowLayout {
                        id: layerRow
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        KanteTextField {
                            objectName: "layerName:" + layerRow.index
                            Layout.fillWidth: true
                            text: layerRow.modelData.name
                            onEditingFinished: {
                                const name = text.trim()
                                if (name !== "" && name !== layerRow.modelData.name) {
                                    dialog.edit(copy => copy.layers[layerRow.index].name = name)
                                }
                                text = Qt.binding(() => layerRow.modelData.name)
                            }
                        }
                        DurationField {
                            key: layerRow.modelData.id
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 6
                            optional: true
                            ms: layerRow.modelData.fade_ms !== undefined ? layerRow.modelData.fade_ms : null
                            maxMs: 600000
                            Accessible.name: qsTr("Fade of the layer")
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: qsTr("The layer's own fade; empty: the change time")
                            onEdited: value => dialog.edit(copy => {
                                if (value === null) {
                                    delete copy.layers[layerRow.index].fade_ms
                                } else {
                                    copy.layers[layerRow.index].fade_ms = value
                                }
                            })
                        }
                        KanteButton {
                            text: qsTr("Up")
                            enabled: layerRow.index > 0
                            onClicked: dialog.edit(copy => dialog.move(copy.layers, layerRow.index, -1))
                        }
                        KanteButton {
                            objectName: "removeLayer:" + layerRow.index
                            text: qsTr("Remove")
                            emphasis: KanteButton.Emphasis.Destructive
                            onClicked: {
                                const id = layerRow.modelData.id
                                const named = "\u201c" + layerRow.modelData.name + "\u201d"
                                dialog.askRemove({
                                    question: qsTr("Remove layer %1?").arg(named),
                                    consequence: qsTr("Every mood and the panic mood forget what they had on it; cues that take it are marked."),
                                    acceptText: qsTr("Remove layer"),
                                    removedText: qsTr("Layer %1 removed").arg(named),
                                    change: copy => {
                                        // By id: the order may change while it asks.
                                        copy.layers = copy.layers.filter(l => l.id !== id)
                                        copy.groups.forEach(g => {
                                            delete (g.base || {})[id]
                                            ;(g.variants || []).forEach(v => delete (v.layers || {})[id])
                                        })
                                        delete copy.panic[id]
                                    }
                                })
                            }
                        }
                    }
                }
                KanteButton {
                    objectName: "addLayer"
                    text: qsTr("+ Layer")
                    onClicked: dialog.edit(copy => copy.layers.push({ id: dialog.newId(), name: dialog.freeName(qsTr("Layer"), dialog.layers.map(l => l.name)) }))
                }
                Item { Layout.fillHeight: true }
            }

            // Collections (MOOD.7): their content is chosen on the page.
            ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: KanteStyle.mutedTextColor
                    text: qsTr("What a collection holds is put together on the Moods page (Compose).")
                }
                Repeater {
                    model: dialog.collections
                    delegate: RowLayout {
                        id: collectionRow
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        KanteTextField {
                            objectName: "collectionName:" + collectionRow.index
                            Layout.fillWidth: true
                            text: collectionRow.modelData.name
                            onEditingFinished: {
                                const name = text.trim()
                                if (name !== "" && name !== collectionRow.modelData.name) {
                                    dialog.edit(copy => copy.collections[collectionRow.index].name = name)
                                }
                                text = Qt.binding(() => collectionRow.modelData.name)
                            }
                        }
                        KanteButton {
                            text: qsTr("Up")
                            enabled: collectionRow.index > 0
                            onClicked: dialog.edit(copy => dialog.move(copy.collections, collectionRow.index, -1))
                        }
                        KanteButton {
                            objectName: "removeCollection:" + collectionRow.index
                            text: qsTr("Remove")
                            emphasis: KanteButton.Emphasis.Destructive
                            onClicked: {
                                const id = collectionRow.modelData.id
                                const named = "\u201c" + collectionRow.modelData.name + "\u201d"
                                dialog.askRemove({
                                    question: qsTr("Remove collection %1?").arg(named),
                                    consequence: qsTr("Its moods and one-shots stay."),
                                    acceptText: qsTr("Remove collection"),
                                    removedText: qsTr("Collection %1 removed").arg(named),
                                    change: copy => copy.collections = copy.collections.filter(c => c.id !== id)
                                })
                            }
                        }
                    }
                }
                KanteButton {
                    objectName: "addCollection"
                    text: qsTr("+ Collection")
                    onClicked: dialog.edit(copy => copy.collections.push({ id: dialog.newId(), name: dialog.freeName(qsTr("Collection"), dialog.collections.map(c => c.name)), moods: [], pads: [] }))
                }
                Item { Layout.fillHeight: true }
            }

            // The panic mood (MOOD.9): per layer off or actions, never sound
            // or picture; they run after the emergency list.
            ColumnLayout {
                spacing: Kirigami.Units.smallSpacing

                QQC2.Label {
                    Layout.fillWidth: true
                    wrapMode: Text.WordWrap
                    color: KanteStyle.mutedTextColor
                    text: qsTr("Panic and hard stop fade every sound and picture out. Here a layer can run actions then, e.g. the house light on; they run after the emergency list.")
                }
                Repeater {
                    model: dialog.layers
                    delegate: RowLayout {
                        id: panicEdit
                        required property var modelData
                        readonly property var content: (dialog.moods.panic || {})[modelData.id]
                        readonly property bool acts: content !== undefined && content.kind === "actions"
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        QQC2.Label {
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 5
                            elide: Text.ElideRight
                            text: panicEdit.modelData.name
                        }
                        KanteComboBox {
                            objectName: "panic:" + panicEdit.modelData.name
                            Layout.preferredWidth: Kirigami.Units.gridUnit * 7
                            model: [qsTr("Off"), qsTr("Actions")]
                            currentIndex: panicEdit.acts ? 1 : 0
                            Accessible.name: panicEdit.modelData.name
                            onActivated: index => {
                                if (index === 0 && panicEdit.acts) {
                                    dialog.edit(copy => delete copy.panic[panicEdit.modelData.id])
                                } else if (index === 1 && !panicEdit.acts) {
                                    dialog.openActions("panic", panicEdit.modelData.id, [])
                                }
                                // Shows what is kept, also when the actions are cancelled.
                                currentIndex = Qt.binding(() => panicEdit.acts ? 1 : 0)
                            }
                        }
                        KanteButton {
                            visible: panicEdit.acts
                            text: panicEdit.acts && panicEdit.content.actions ? qsTr("Actions: %1…").arg(panicEdit.content.actions.length) : ""
                            onClicked: dialog.openActions("panic", panicEdit.modelData.id, panicEdit.content.actions)
                        }
                        Item { Layout.fillWidth: true }
                    }
                }
                Item { Layout.fillHeight: true }
            }
        }
    }
}
