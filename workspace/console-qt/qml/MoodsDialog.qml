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
 * were. Until the state brings an edit back, the next builds on what was
 * sent (`draft`), so quick edits all stay. A removal asks first and gets
 * the window's toast with Undo (Removals, `askRemove`): lmix-core keeps
 * the moods before it for `restore`.
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
    /** The core, for the colour names and how long a name is. */
    required property var core
    /** Asks a removal and sends it (AppShell.askRemoveWith). */
    required property var askRemove

    /** The moods of the state. */
    readonly property var stateMoods: show && show.moods ? show.moods : ({})
    /** What was sent and not yet back in the state, or null. */
    property var draft: null
    /** Edits sent and not yet answered, oldest first: each set_moods as
        lmix-core's types write it (`core.canonicalAction`), the form
        `answered` reports it in. */
    property var unanswered: []
    /** Edits sent and not yet answered. */
    readonly property int inFlight: unanswered.length
    /** The moods edits build on and the dialog shows. */
    readonly property var moods: draft !== null ? draft : stateMoods
    readonly property var layers: moods.layers || []
    readonly property var groups: moods.groups || []
    readonly property var collections: moods.collections || []
    readonly property var stages: show && show.stages ? show.stages.map(s => s.name) : []
    /** The longest name lmix-core takes (MOOD_NAME_MAX); 0 without a core. */
    readonly property int nameMax: core ? JSON.parse(core.choices("mood_limits")).name_max || 0 : 0

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
    // An edit that never got an answer (lmix-core gone) must not hold the
    // draft for good.
    onClosed: {
        pending = {}
        draft = null
        unanswered = []
    }
    // Once every edit is answered, the next state holds them all; without
    // a core (tests) nothing answers, and the state is taken at once.
    onStateMoodsChanged: {
        if (inFlight === 0) {
            draft = null
        }
    }

    Connections {
        target: dialog.core
        enabled: dialog.inFlight > 0
        function onAnswered(action, refusal) {
            // Only this dialog's: other parts of the Console set moods too.
            // Both sides are lmix-core's own writing, so equal strings.
            const at = dialog.unanswered.indexOf(action)
            if (at < 0) {
                return
            }
            const rest = dialog.unanswered.slice()
            rest.splice(at, 1)
            dialog.unanswered = rest
            if (refusal.length > 0) {
                dialog.draft = null
            } else if (dialog.inFlight === 0) {
                // The state may have come before the answer.
                settle.restart()
            }
        }
    }
    Timer {
        id: settle
        interval: 1000
        onTriggered: {
            if (dialog.inFlight === 0) {
                dialog.draft = null
            }
        }
    }

    title: qsTr("Edit moods")
    modal: true
    // At most 90 % of the window; lower, the tabs scroll between the tab bar
    // and Close (docs/DESIGN-DIALOGE.md rule 3).
    size: KanteDialog.Size.Large
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
    /** A copy of the moods, changed with `change`. */
    function changed(change) {
        const copy = Object.assign({ layers: [], groups: [], collections: [], panic: {}, change_ms: 3000, inside_ms: 1000 },
                                   JSON.parse(JSON.stringify(moods)))
        change(copy)
        return copy
    }
    /** The set_moods action for `copy`, shown at once as the draft until
        it is answered. An action the core cannot read is not waited for. */
    function sending(copy) {
        const action = { action: "set_moods", moods: copy }
        if (core) {
            const canonical = core.canonicalAction(JSON.stringify(action))
            if (canonical.length > 0) {
                draft = copy
                unanswered = unanswered.concat([canonical])
            }
        }
        return action
    }
    /** Changes a copy of the moods with `change` and sends it whole. */
    function edit(change) {
        const before = { draft: draft, unanswered: unanswered }
        if (act(sending(changed(change))) === false) {
            // Not sent: no answer comes, the dialog shows what it did.
            unanswered = before.unanswered
            draft = inFlight > 0 ? before.draft : null
        }
    }
    /** Asks before a removal (docs/DESIGN-DIALOGE.md rule 1) and removes
        with `request.change` from the moods as they are then; `request.name`
        is the name lmix-core gives the removal (`restorable`). */
    function remove(request) {
        askRemove({
            kind: "mood",
            name: request.name,
            question: request.question,
            consequence: request.consequence,
            acceptText: request.acceptText,
            removedText: request.removedText,
            makeAction: () => dialog.sending(dialog.changed(request.change))
        })
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

    /** A name to edit: Enter or leaving the field renames (`renamed`, only
        when it changed and is not empty), Escape goes back to the name kept
        without sending; a second Escape closes the dialog. After either the
        field shows the name kept again: a refused one does not stay. */
    component NameField: KanteTextField {
        id: nameEdit
        /** The name kept now. */
        property string kept
        /** The longest name to type; 0: Qt's own limit. */
        property int limit: 0
        signal renamed(string name)
        text: kept
        // Cut while typing: lmix-core would refuse a longer name.
        Binding on maximumLength {
            when: nameEdit.limit > 0
            value: nameEdit.limit
        }
        onEditingFinished: {
            const name = text.trim()
            if (name !== "" && name !== kept) {
                renamed(name)
            }
            text = Qt.binding(() => nameEdit.kept)
        }
        Keys.onEscapePressed: event => {
            event.accepted = text !== kept
            text = Qt.binding(() => nameEdit.kept)
        }
    }

    /** A handle to drag a row of a list to another place; the row it is
        over is marked. Only the handle: a list that scrolls stays
        flickable over the rest of the row (on touch, too). */
    component DragGrip: QQC2.Label {
        id: grip
        /** A dragged row is over this one. */
        property bool over: false
        readonly property alias active: gripDrag.active
        signal moved(point scenePosition)
        signal dropped(point scenePosition)
        /** The drag is over, dropped or taken away. */
        signal ended()
        text: "\u2807"
        font.pixelSize: Kirigami.Units.gridUnit
        color: over ? KanteStyle.focusColor : KanteStyle.mutedTextColor
        QQC2.ToolTip.visible: gripHover.hovered
        QQC2.ToolTip.text: qsTr("Drag to move")
        HoverHandler {
            id: gripHover
            cursorShape: Qt.SizeVerCursor
        }
        DropDrag {
            id: gripDrag
            xAxis.enabled: false
            // The grip is for moving: a list it is in does not take the drag over to scroll.
            grabPermissions: PointerHandler.CanTakeOverFromAnything
            onMoved: position => grip.moved(position)
            onDropped: position => grip.dropped(position)
            onEnded: grip.ended()
        }
    }

    /** A drag to reorder: which list ("places", "layers", "collections")
        and the row it would go to (a place by id, else an index). */
    property string dragList: ""
    property var dropAt: null
    /** Where the pointer of that drag is, in the window. */
    property point dragPoint
    /** The row of `repeater` under the pointer at `scenePosition`, or -1. */
    function rowUnder(repeater, scenePosition) {
        for (let i = 0; i < repeater.count; i++) {
            const row = repeater.itemAt(i)
            const p = row.mapFromItem(null, scenePosition.x, scenePosition.y)
            if (p.y >= 0 && p.y < row.height) {
                return i
            }
        }
        return -1
    }
    /** The place under the pointer, by id, or "". */
    function placeUnder(scenePosition) {
        const p = placesView.mapFromItem(null, scenePosition.x, scenePosition.y)
        const at = placesView.indexAt(p.x, p.y + placesView.contentY)
        return at >= 0 ? places[at].id : ""
    }
    /** A group goes to the place of the group dropped on; a variant to the
        place of a variant of its own group, or first when dropped on the
        head; anything else stays. */
    function dropPlace(from, toId) {
        // The edit rebuilds the rows, and the dragged row's `ended` with them.
        dragList = ""
        const to = places.find(p => p.id === toId)
        if (!to || to.id === from.id || (from.variant && to.group.id !== from.group.id)) {
            return
        }
        edit(copy => {
            if (!from.variant) {
                const at = copy.groups.findIndex(g => g.id === to.group.id)
                const index = copy.groups.findIndex(g => g.id === from.group.id)
                move(copy.groups, index, at - index)
            } else if (to.group.id === from.group.id) {
                const group = copy.groups.find(g => g.id === from.group.id)
                const index = group.variants.findIndex(v => v.id === from.variant.id)
                const at = to.variant ? group.variants.findIndex(v => v.id === to.variant.id) : 0
                move(group.variants, index, at - index)
            }
        })
    }
    /** A layer or collection from `index` to `at`. */
    function dropRow(key, index, at) {
        dragList = ""
        if (at >= 0 && at !== index) {
            edit(copy => move(copy[key], index, at - index))
        }
    }

    // A drag held at the edge of the layers or collections scrolls them;
    // the row it would go to follows the rows moving under the pointer.
    // Not the places: their ListView drops a delegate scrolled far out of
    // sight, and the drag with it.
    DragScroll {
        objectName: "layersScroll"
        flickable: layersView.contentItem
        dragging: dialog.dragList === "layers"
        scenePosition: dialog.dragPoint
        onScrolled: dialog.dropAt = dialog.rowUnder(layerRows, dialog.dragPoint)
    }
    DragScroll {
        objectName: "collectionsScroll"
        flickable: collectionsView.contentItem
        dragging: dialog.dragList === "collections"
        scenePosition: dialog.dragPoint
        onScrolled: dialog.dropAt = dialog.rowUnder(collectionRows, dialog.dragPoint)
    }

    /** The layer whose sound or picture the level dialog edits. */
    property string mediaLayer: ""
    /** Its content now: waiting for a file, or kept. */
    readonly property var mediaContent: mediaLayer === "" ? null
        : (pending[mediaLayer] || own(mediaLayer) || null)
    function openMedia(layerId) {
        mediaLayer = layerId
        mediaDialog.open()
    }
    /** The sound or picture of the level dialog with `changes`. */
    function changeMediaFields(changes) {
        const next = JSON.parse(JSON.stringify(mediaContent))
        Object.assign(next[next.kind], changes)
        changeMedia(mediaLayer, next)
    }

    // Level and crosspoints of a sound, opacity and layer of a picture. Its
    // fades are the layer's (its own or the change time), not the file's.
    KanteDialog {
        id: mediaDialog
        objectName: "mediaDialog"
        readonly property var media: dialog.mediaContent && (dialog.mediaContent.kind === "sound" || dialog.mediaContent.kind === "picture")
            ? dialog.mediaContent[dialog.mediaContent.kind] : null
        readonly property bool sound: dialog.mediaContent !== null && dialog.mediaContent.kind === "sound"
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        modal: true
        title: {
            const layer = dialog.layers.find(l => l.id === dialog.mediaLayer)
            return layer ? qsTr("Layer %1").arg(layer.name) : ""
        }
        standardButtons: QQC2.Dialog.Close
        size: KanteDialog.Size.Medium
        contentHeight: mediaColumn.implicitHeight
        onClosed: dialog.mediaLayer = ""

        ColumnLayout {
            id: mediaColumn
            width: parent.width
            spacing: Kirigami.Units.smallSpacing

            GridLayout {
                columns: 3
                columnSpacing: Kirigami.Units.largeSpacing
                QQC2.Label {
                    visible: mediaDialog.sound
                    text: qsTr("Level")
                }
                KanteSpinBox {
                    objectName: "mediaLevel"
                    visible: mediaDialog.sound
                    from: -600
                    to: 120
                    stepSize: 5
                    editable: true
                    value: mediaDialog.sound && mediaDialog.media ? Math.round((mediaDialog.media.gain_db || 0) * 10) : 0
                    textFromValue: v => (v / 10).toFixed(1) + " dB"
                    valueFromText: t => Math.round(parseFloat(t.replace(",", ".").replace("\u2212", "-").replace(/[^0-9.\-]/g, "")) * 10)
                    Accessible.name: qsTr("Level of the sound")
                    onValueModified: dialog.changeMediaFields({ gain_db: value / 10 })
                }
                QQC2.Label {
                    visible: mediaDialog.sound
                    text: qsTr("0 dB: as in the file; −60 dB: silence")
                    color: KanteStyle.mutedTextColor
                }
                QQC2.Label {
                    visible: !mediaDialog.sound
                    text: qsTr("Opacity")
                }
                KanteSpinBox {
                    objectName: "mediaOpacity"
                    visible: !mediaDialog.sound
                    from: 0
                    to: 100
                    stepSize: 5
                    editable: true
                    value: !mediaDialog.sound && mediaDialog.media ? Math.round((mediaDialog.media.opacity ?? 1) * 100) : 100
                    textFromValue: v => v + " %"
                    valueFromText: t => parseInt(t)
                    Accessible.name: qsTr("Opacity of the picture")
                    onValueModified: dialog.changeMediaFields({ opacity: value / 100 })
                }
                Item { visible: !mediaDialog.sound }
                QQC2.Label {
                    visible: !mediaDialog.sound
                    text: qsTr("Layer")
                }
                KanteSpinBox {
                    objectName: "mediaPictureLayer"
                    visible: !mediaDialog.sound
                    from: 0
                    to: 999
                    editable: true
                    value: !mediaDialog.sound && mediaDialog.media ? (mediaDialog.media.layer ?? 1) : 1
                    Accessible.name: qsTr("Layer of the picture")
                    onValueModified: dialog.changeMediaFields({ layer: value })
                }
                QQC2.Label {
                    visible: !mediaDialog.sound
                    text: qsTr("On the same layer the later picture is in front")
                    color: KanteStyle.mutedTextColor
                }
            }
            SoundRoutes {
                visible: mediaDialog.sound
                Layout.fillWidth: true
                key: dialog.chosenId + dialog.mediaLayer
                routes: mediaDialog.sound && mediaDialog.media ? (mediaDialog.media.routes || []) : []
                zones: dialog.show && dialog.show.zones ? dialog.show.zones : []
                onEdited: routes => dialog.changeMediaFields({ routes: routes })
            }
            QQC2.Label {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: KanteStyle.mutedTextColor
                text: qsTr("It fades in and out with the layer: its own fade, else the change time.")
            }
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

    ColumnLayout {
        id: column
        width: parent.width
        // The dialog's room: lower than wished for (a small window), the
        // tabs' lists scroll and their buttons stay in reach.
        height: parent.height
        spacing: Kirigami.Units.smallSpacing

        KanteTabBar {
            Layout.fillWidth: true
            model: [qsTr("Moods"), qsTr("Layers"), qsTr("Collections"), qsTr("Panic mood")]
            currentIndex: dialog.tab
            onActivated: index => dialog.tab = index
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredHeight: Kirigami.Units.gridUnit * 22
            Layout.minimumHeight: Kirigami.Units.gridUnit * 6
            currentIndex: dialog.tab

            // Groups and variants: the list left, the chosen one right.
            RowLayout {
                spacing: Kirigami.Units.largeSpacing

                // As wide with a choice as without: the buttons do not move.
                ColumnLayout {
                    objectName: "placesColumn"
                    readonly property real fixedWidth: Kirigami.Units.gridUnit * 16
                    Layout.preferredWidth: fixedWidth
                    Layout.minimumWidth: fixedWidth
                    Layout.maximumWidth: fixedWidth
                    Layout.fillHeight: true

                    ListView {
                        id: placesView
                        objectName: "places"
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        clip: true
                        model: dialog.places
                        boundsBehavior: Flickable.StopAtBounds
                        QQC2.ScrollBar.vertical: QQC2.ScrollBar {}
                        // A long name is cut: the whole one under the pointer.
                        delegate: Item {
                            id: placeItem
                            required property var modelData
                            width: ListView.view.width
                            implicitHeight: placeRow.implicitHeight
                            QQC2.ToolTip.visible: placeHover.hovered && !placeGrip.active
                            QQC2.ToolTip.text: modelData.variant ? modelData.group.name + " / " + modelData.variant.name : modelData.group.name
                            QQC2.ToolTip.delay: Kirigami.Units.toolTipDelay
                            HoverHandler { id: placeHover }

                            KanteListRow {
                                id: placeRow
                                readonly property var modelData: placeItem.modelData
                                readonly property string path: modelData.variant ? modelData.group.name + "/" + modelData.variant.name : modelData.group.name
                                objectName: "place:" + path
                                width: parent.width
                                density: KanteListRow.Density.Compact
                                leadingText: modelData.variant ? "›" : ""
                                leadingWidth: Kirigami.Units.gridUnit
                                text: modelData.variant ? modelData.variant.name : modelData.group.name
                                selected: dialog.chosenId === modelData.id
                                dropTarget: dialog.dragList === "places" && dialog.dropAt === modelData.id
                                opacity: placeGrip.active ? 0.5 : 1
                                onClicked: dialog.chosenId = modelData.id
                                trailing: DragGrip {
                                    id: placeGrip
                                    objectName: "placeGrip:" + placeRow.path
                                    over: placeRow.dropTarget
                                    onMoved: position => {
                                        dialog.dragList = "places"
                                        dialog.dropAt = dialog.placeUnder(position)
                                    }
                                    onDropped: position => dialog.dropPlace(placeRow.modelData, dialog.placeUnder(position))
                                    onEnded: dialog.dragList = ""
                                }
                            }
                        }
                    }
                    // Two fixed rows, in every language: adding, then moving
                    // and removing the one chosen.
                    RowLayout {
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
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        KanteToolButton {
                            objectName: "placeUp"
                            icon.name: "go-up"
                            text: qsTr("Up")
                            display: QQC2.AbstractButton.IconOnly
                            enabled: dialog.chosen !== null
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: qsTr("Move up")
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
                        KanteToolButton {
                            objectName: "placeDown"
                            icon.name: "go-down"
                            text: qsTr("Down")
                            display: QQC2.AbstractButton.IconOnly
                            enabled: dialog.chosen !== null
                            QQC2.ToolTip.visible: hovered
                            QQC2.ToolTip.text: qsTr("Move down")
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
                        Item { Layout.fillWidth: true }
                        KanteButton {
                            objectName: "removePlace"
                            text: qsTr("Remove")
                            emphasis: KanteButton.Emphasis.Destructive
                            enabled: dialog.chosen !== null
                            onClicked: {
                                const place = dialog.chosen
                                const gone = place.variant ? [place.variant.id] : [place.group.id].concat((place.group.variants || []).map(v => v.id))
                                const variants = place.variant ? 0 : (place.group.variants || []).length
                                const name = place.variant ? place.group.name + " / " + place.variant.name : place.group.name
                                dialog.remove({
                                    name: name,
                                    question: place.variant ? qsTr("Remove variant “%1”?").arg(name) : qsTr("Remove group “%1”?").arg(name),
                                    consequence: (variants > 0 ? qsTr("Its variants go with it (%1). ").arg(variants) : "")
                                        + qsTr("Collections forget it; mood cues that set it are marked."),
                                    acceptText: place.variant ? qsTr("Remove variant") : qsTr("Remove group"),
                                    removedText: place.variant ? qsTr("Variant “%1” removed").arg(name) : qsTr("Group “%1” removed").arg(name),
                                    // By id: the moods may change while it asks.
                                    change: copy => {
                                        copy.groups = copy.groups.filter(g => gone.indexOf(g.id) < 0)
                                        copy.groups.forEach(g => g.variants = (g.variants || []).filter(v => gone.indexOf(v.id) < 0))
                                        copy.collections.forEach(c => c.moods = (c.moods || []).filter(m => gone.indexOf(m) < 0))
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
                        NameField {
                            id: nameField
                            objectName: "moodName"
                            Layout.fillWidth: true
                            limit: dialog.nameMax
                            kept: dialog.chosen ? (dialog.chosen.variant ? dialog.chosen.variant.name : dialog.chosen.group.name) : ""
                            onRenamed: name => {
                                const place = dialog.chosen
                                if (place) {
                                    dialog.edit(copy => { const at = dialog.find(copy, place); (at.variant || at.group).name = name })
                                }
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
                            spacing: Kirigami.Units.largeSpacing

                            // Each layer: what it gets; a sound or picture has its
                            // file on a line of its own, wide enough to read.
                            Repeater {
                                model: dialog.chosen ? dialog.layers : []
                                delegate: ColumnLayout {
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

                                    RowLayout {
                                        Layout.fillWidth: true
                                        spacing: Kirigami.Units.smallSpacing

                                        QQC2.Label {
                                            id: layerLabel
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
                                            objectName: "loop:" + layerEdit.modelData.name
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
                                            objectName: "media:" + layerEdit.modelData.name
                                            visible: layerEdit.media !== null
                                            // A sound has a level, a picture an opacity and a place.
                                            text: layerEdit.kind === "picture" ? qsTr("Opacity…") : qsTr("Level…")
                                            QQC2.ToolTip.visible: hovered
                                            QQC2.ToolTip.text: layerEdit.kind === "sound" ? qsTr("Level and crosspoints of the sound") : qsTr("Opacity and layer of the picture")
                                            onClicked: dialog.openMedia(layerEdit.modelData.id)
                                        }
                                        KanteButton {
                                            visible: layerEdit.kind === "actions"
                                            text: layerEdit.kind === "actions" && layerEdit.content.actions ? qsTr("Actions: %1…").arg(layerEdit.content.actions.length) : ""
                                            onClicked: dialog.openActions("mood", layerEdit.modelData.id, layerEdit.content.actions)
                                        }
                                        Item { Layout.fillWidth: true }
                                    }
                                    PathField {
                                        objectName: "file:" + layerEdit.modelData.name
                                        Layout.fillWidth: true
                                        // Under the choice, after the layer's name.
                                        Layout.leftMargin: layerLabel.width + Kirigami.Units.smallSpacing
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
                                }
                            }
                        }
                    }
                }
            }

            // Layers and the change times (MOOD.1). The layers scroll; the
            // times above and "+ Layer" below stay in reach.
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
                // The columns of the rows below, over their fields.
                Item {
                    objectName: "layersHead"
                    visible: dialog.layers.length > 0
                    Layout.fillWidth: true
                    implicitHeight: fadeHead.implicitHeight
                    readonly property Item first: layerRows.count > 0 ? layerRows.itemAt(0) : null
                    QQC2.Label {
                        x: parent.first ? parent.first.nameX : 0
                        text: qsTr("Name")
                        color: KanteStyle.mutedTextColor
                    }
                    QQC2.Label {
                        id: fadeHead
                        objectName: "fadeHead"
                        x: parent.first ? parent.first.fadeX : 0
                        width: Kirigami.Units.gridUnit * 6
                        elide: Text.ElideRight
                        text: qsTr("Own fade")
                        color: KanteStyle.mutedTextColor
                        QQC2.ToolTip.visible: fadeHeadHover.hovered
                        QQC2.ToolTip.text: qsTr("The layer's own fade; empty: the change time")
                        HoverHandler { id: fadeHeadHover }
                    }
                }
                QQC2.ScrollView {
                    id: layersView
                    objectName: "layersView"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth

                    ColumnLayout {
                        width: layersView.availableWidth
                        spacing: Kirigami.Units.smallSpacing

                        Repeater {
                            id: layerRows
                            model: dialog.layers
                            delegate: RowLayout {
                                id: layerRow
                                required property var modelData
                                required property int index
                                /** Where the name and the fade start, for the heads. */
                                readonly property real nameX: layerName.x
                                readonly property real fadeX: layerFade.x
                                objectName: "layerRow:" + index
                                Layout.fillWidth: true
                                opacity: layerGrip.active ? 0.5 : 1
                                DragGrip {
                                    id: layerGrip
                                    objectName: "layerGrip:" + layerRow.index
                                    over: dialog.dragList === "layers" && dialog.dropAt === layerRow.index
                                    onMoved: position => {
                                        dialog.dragList = "layers"
                                        dialog.dragPoint = position
                                        dialog.dropAt = dialog.rowUnder(layerRows, position)
                                    }
                                    onDropped: position => dialog.dropRow("layers", layerRow.index, dialog.rowUnder(layerRows, position))
                                    onEnded: dialog.dragList = ""
                                }
                                NameField {
                                    id: layerName
                                    objectName: "layerName:" + layerRow.index
                                    Layout.fillWidth: true
                                    limit: dialog.nameMax
                                    kept: layerRow.modelData.name
                                    Accessible.name: qsTr("Name of the layer")
                                    onRenamed: name => dialog.edit(copy => copy.layers[layerRow.index].name = name)
                                }
                                DurationField {
                                    id: layerFade
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
                                KanteToolButton {
                                    icon.name: "go-up"
                                    text: qsTr("Up")
                                    display: QQC2.AbstractButton.IconOnly
                                    enabled: layerRow.index > 0
                                    QQC2.ToolTip.visible: hovered
                                    QQC2.ToolTip.text: qsTr("Move up")
                                    onClicked: dialog.edit(copy => dialog.move(copy.layers, layerRow.index, -1))
                                }
                                KanteToolButton {
                                    icon.name: "go-down"
                                    text: qsTr("Down")
                                    display: QQC2.AbstractButton.IconOnly
                                    enabled: layerRow.index < dialog.layers.length - 1
                                    QQC2.ToolTip.visible: hovered
                                    QQC2.ToolTip.text: qsTr("Move down")
                                    onClicked: dialog.edit(copy => dialog.move(copy.layers, layerRow.index, 1))
                                }
                                KanteButton {
                                    objectName: "removeLayer:" + layerRow.index
                                    text: qsTr("Remove")
                                    emphasis: KanteButton.Emphasis.Destructive
                                    onClicked: {
                                        const id = layerRow.modelData.id
                                        dialog.remove({
                                            name: layerRow.modelData.name,
                                            question: qsTr("Remove layer “%1”?").arg(layerRow.modelData.name),
                                            consequence: qsTr("Every mood and the panic mood forget what they had on it; cues that take it are marked."),
                                            acceptText: qsTr("Remove layer"),
                                            removedText: qsTr("Layer “%1” removed").arg(layerRow.modelData.name),
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
                    }
                }
                KanteButton {
                    objectName: "addLayer"
                    text: qsTr("+ Layer")
                    onClicked: dialog.edit(copy => copy.layers.push({ id: dialog.newId(), name: dialog.freeName(qsTr("Layer"), dialog.layers.map(l => l.name)) }))
                }
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
                QQC2.ScrollView {
                    id: collectionsView
                    objectName: "collectionsView"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth

                    ColumnLayout {
                        width: collectionsView.availableWidth
                        spacing: Kirigami.Units.smallSpacing

                        Repeater {
                            id: collectionRows
                            model: dialog.collections
                            delegate: RowLayout {
                                id: collectionRow
                                required property var modelData
                                required property int index
                                Layout.fillWidth: true
                                opacity: collectionGrip.active ? 0.5 : 1
                                DragGrip {
                                    id: collectionGrip
                                    objectName: "collectionGrip:" + collectionRow.index
                                    over: dialog.dragList === "collections" && dialog.dropAt === collectionRow.index
                                    onMoved: position => {
                                        dialog.dragList = "collections"
                                        dialog.dragPoint = position
                                        dialog.dropAt = dialog.rowUnder(collectionRows, position)
                                    }
                                    onDropped: position => dialog.dropRow("collections", collectionRow.index, dialog.rowUnder(collectionRows, position))
                                    onEnded: dialog.dragList = ""
                                }
                                NameField {
                                    objectName: "collectionName:" + collectionRow.index
                                    Layout.fillWidth: true
                                    limit: dialog.nameMax
                                    kept: collectionRow.modelData.name
                                    Accessible.name: qsTr("Name of the collection")
                                    onRenamed: name => dialog.edit(copy => copy.collections[collectionRow.index].name = name)
                                }
                                KanteToolButton {
                                    icon.name: "go-up"
                                    text: qsTr("Up")
                                    display: QQC2.AbstractButton.IconOnly
                                    enabled: collectionRow.index > 0
                                    QQC2.ToolTip.visible: hovered
                                    QQC2.ToolTip.text: qsTr("Move up")
                                    onClicked: dialog.edit(copy => dialog.move(copy.collections, collectionRow.index, -1))
                                }
                                KanteToolButton {
                                    icon.name: "go-down"
                                    text: qsTr("Down")
                                    display: QQC2.AbstractButton.IconOnly
                                    enabled: collectionRow.index < dialog.collections.length - 1
                                    QQC2.ToolTip.visible: hovered
                                    QQC2.ToolTip.text: qsTr("Move down")
                                    onClicked: dialog.edit(copy => dialog.move(copy.collections, collectionRow.index, 1))
                                }
                                KanteButton {
                                    objectName: "removeCollection:" + collectionRow.index
                                    text: qsTr("Remove")
                                    emphasis: KanteButton.Emphasis.Destructive
                                    onClicked: {
                                        const id = collectionRow.modelData.id
                                        dialog.remove({
                                            name: collectionRow.modelData.name,
                                            question: qsTr("Remove collection “%1”?").arg(collectionRow.modelData.name),
                                            consequence: qsTr("Its moods and one-shots stay."),
                                            acceptText: qsTr("Remove collection"),
                                            removedText: qsTr("Collection “%1” removed").arg(collectionRow.modelData.name),
                                            change: copy => copy.collections = copy.collections.filter(c => c.id !== id)
                                        })
                                    }
                                }
                            }
                        }
                    }
                }
                KanteButton {
                    objectName: "addCollection"
                    text: qsTr("+ Collection")
                    onClicked: dialog.edit(copy => copy.collections.push({ id: dialog.newId(), name: dialog.freeName(qsTr("Collection"), dialog.collections.map(c => c.name)), moods: [], pads: [] }))
                }
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
                QQC2.ScrollView {
                    id: panicView
                    objectName: "panicView"
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    contentWidth: availableWidth

                    ColumnLayout {
                        width: panicView.availableWidth
                        spacing: Kirigami.Units.smallSpacing

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
                    }
                }
            }
        }
    }
}
