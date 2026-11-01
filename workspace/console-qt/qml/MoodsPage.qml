pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import QtCore
import org.kde.kirigami as Kirigami
import Kante
import org.lmix.console

/**
 * The Moods page (LASTENHEFT-SHOW.md §3.16, design E): moods to change
 * between as needed, beside the order of the Show page. A click on a
 * group's head sets its base, a click on a variant the variant; the core
 * fades only the layers that change (`set_mood`, MOOD.4).
 *
 *   COLLECTIONS │ NOW  Tavern / Night   ▓▓▓▓▓░░ 1.2 s    │ LAYERS
 *   All    8·4  │ ▌Tavern  [BASE]                        │ Light  dim  Night
 *   Act 1  5·2  │   [Day] [Night] [Fight]                │ Music  lute Base
 *               │ ▌Forest  [BASE]                        │
 *               │ ONE-SHOTS  [Door] [Thunder]            │
 *
 * Collections (left) and the inspector (right) fold away. A collection is
 * a choice from the pools (MOOD.7): "Compose" puts moods and one-shots in
 * or out with a click. One-shots are the show's pads (MOOD.8).
 */
AppPage {
    id: root

    /** The window (AppShell): show, actions, helpers. */
    required property var shell

    readonly property var show: shell.show
    readonly property var moods: show && show.moods ? show.moods : ({})
    readonly property var layers: moods.layers || []
    readonly property var groups: moods.groups || []
    readonly property var collections: moods.collections || []
    readonly property var now: show && show.mood ? show.mood : ({})
    /** The mood set now as `{ group, variant }`, or null. */
    readonly property var current: placeOf(now.mood)
    /** The mood set now is no longer among the moods (removed since). */
    readonly property bool currentGone: !!now.mood && current === null
    /** A panic ran: the panic mood holds the layers, not a mood. */
    readonly property bool panicked: (now.layers || []).some(l => l.origin.kind === "panic")

    /** The collection shown; "" for all of the pools. */
    property string collectionId: settings.collection
    readonly property var collection: collections.find(c => c.id === collectionId) || null
    /** The collection marked in the list: "" for all, also when the show
     *  has no collection by the remembered id (another show). */
    readonly property string shownCollectionId: collection ? collectionId : ""
    /** Putting moods and one-shots in or out of the collection. */
    property bool composing: false
    property bool collectionsOpen: settings.collectionsOpen
    property bool inspectorOpen: settings.inspectorOpen

    /** Pads of the show, with their list: `{ list, cue }`. */
    readonly property var pads: {
        const all = []
        for (const list of (show && show.cue_lists) || []) {
            if (list.pads) {
                for (const cue of list.cues) {
                    all.push({ list: list, cue: cue })
                }
            }
        }
        return all
    }
    /** Ids of those pads: a collection keeps the cues of a list switched
     *  off (lmix-core), shown and counted only while it has pads. */
    readonly property var padIds: new Set(pads.map(p => p.cue.id))
    readonly property var runningIds: ((show && show.running_cues) || []).map(r => r.cue)

    /** Groups shown: all, or those with a mood in the collection. */
    readonly property var shownGroups: groups.filter(g => composing || !collection
        || inCollection(g.id) || (g.variants || []).some(v => inCollection(v.id)))
    /** Pads of the collection shown, to look up without a search. */
    readonly property var collectionPads: new Set(collection ? collection.pads || [] : [])
    readonly property var shownPads: pads.filter(p => composing || !collection
        || collectionPads.has(p.cue.id))

    /** The fade of the last change, 0..1; 1 when done. */
    property real fadeProgress: 1
    readonly property bool fading: now.fading !== undefined && now.fading !== null && fadeProgress < 1

    pageId: "moods"
    title: qsTr("Moods")
    subtitle: panicked ? qsTr("Panic mood") : current ? placeName(current) : ""
    busy: fading
    focusTarget: groupsView
    // Beside "Panic" for everything (PanicButtons): this one is the show's.
    panicText: qsTr("Panic mood")
    panicTip: qsTr("Stops every cue and sound; the panic mood runs")
    onPanicRequested: shell.act({ action: "cue_panic" })
    pageKeys: [
        { what: qsTr("Base of group 1…9"), key: "1…9" },
        { what: qsTr("Back to the mood before"), key: Texts.keyLabel("Backspace") }
    ]

    // Keys 1…9 and Backspace in the whole window while the page shows, as
    // on the Mix page, not only where the focus is; the page's buttons take
    // none, so space never presses one again.
    Instantiator {
        model: 9
        delegate: Shortcut {
            required property int index
            sequence: String(index + 1)
            enabled: root.visible && !root.shell.keyTaken(sequence)
            // Held down, once: a repeat would change back and forth.
            autoRepeat: false
            onActivated: root.setGroup(index)
        }
    }
    Shortcut {
        sequence: "Backspace"
        enabled: root.visible && !root.shell.keyTaken(sequence)
        autoRepeat: false
        onActivated: root.back()
    }

    Settings {
        id: settings
        category: "moods"
        property string collection: ""
        property bool collectionsOpen: true
        property bool inspectorOpen: true
    }
    onCollectionIdChanged: {
        settings.collection = collectionId
        composing = false
    }
    onCollectionsOpenChanged: settings.collectionsOpen = collectionsOpen
    onInspectorOpenChanged: settings.inspectorOpen = inspectorOpen

    // The progress runs only while a change fades: no timer otherwise. It
    // counts from when the change came, by this machine's clock (the core
    // may run on another one, ShowView does the same).
    property real fadeSince: 0
    property real fadeFrom: -1
    function updateFade() {
        const f = now.fading
        if (f && f.since_ms !== fadeFrom) {
            fadeFrom = f.since_ms
            fadeSince = Date.now()
        }
        fadeProgress = f && f.duration_ms > 0
            ? Math.max(0, Math.min(1, (Date.now() - fadeSince) / f.duration_ms)) : 1
    }
    onNowChanged: updateFade()
    Timer {
        interval: 100
        repeat: true
        running: root.fading && root.visible
        onTriggered: root.updateFade()
    }

    function placeOf(id) {
        if (!id) {
            return null
        }
        for (const g of groups) {
            if (g.id === id) {
                return { group: g, variant: null }
            }
            const v = (g.variants || []).find(x => x.id === id)
            if (v) {
                return { group: g, variant: v }
            }
        }
        return null
    }
    function placeName(place) {
        return place.variant ? place.group.name + " / " + place.variant.name : place.group.name
    }
    /** "1 mood", "4 moods": one and many, as English and German count. */
    function moodCount(n) {
        return (n === 1 ? qsTr("%1 mood") : qsTr("%1 moods")).arg(n)
    }
    function shotCount(n) {
        return (n === 1 ? qsTr("%1 one-shot") : qsTr("%1 one-shots")).arg(n)
    }
    /** What is left of the fade in seconds, in the language's numbers ("6,0 s"). */
    function secondsLeft(ms, locale) {
        return qsTr("%1 s left").arg(Number(ms / 1000).toLocaleString(locale || Qt.locale(), "f", 1))
    }
    function inCollection(id) {
        return collection !== null && (collection.moods || []).indexOf(id) >= 0
    }
    /** What `place` has on `layer`, and whether it is the variant's own. */
    function contentOf(place, layerId) {
        const own = place.variant && place.variant.layers ? place.variant.layers[layerId] : undefined
        if (own !== undefined) {
            return { content: own, own: true }
        }
        const base = place.group.base ? place.group.base[layerId] : undefined
        return { content: base !== undefined ? base : { kind: "off" }, own: false }
    }
    function fileName(path) {
        return String(path || "").split("/").pop()
    }
    function describe(content) {
        switch (content.kind) {
        case "sound": return fileName(content.sound.file)
        case "picture": return fileName(content.picture.file)
        case "actions": return content.actions.length === 1
            ? describeAction(content.actions[0]) : qsTr("%1 actions").arg(content.actions.length)
        default: return qsTr("Off")
        }
    }
    /** One action by where it goes (a target and its path), else its name. */
    function describeAction(action) {
        return action.target ? [action.target, action.path || ""].join(" ").trim() : action.action
    }
    /** Key `n` (0…8): the base of the n-th group shown, when its head
     *  could be clicked. */
    function setGroup(n) {
        const group = shownGroups[n]
        if (group && !composing && (!collection || inCollection(group.id))) {
            setMood(group.id)
        }
    }
    /** "12 Thunder" of the cue that took a layer (MOOD.5). */
    function cueLabel(origin) {
        const list = ((show && show.cue_lists) || []).find(l => l.id === origin.list)
        let found = null
        const walk = cues => (cues || []).forEach(c => {
            if (c.id === origin.cue) {
                found = c
            }
            walk(c.children)
        })
        walk(list ? list.cues : [])
        return found ? [found.number, found.name].filter(x => x).join(" ") || qsTr("Cue") : qsTr("Cue (gone)")
    }
    /** A tile's look: composing, a member is marked (cyan, with a check
     *  mark: not by colour alone); else the one set now (or running) is
     *  filled; one that cannot be clicked is no button to look at. */
    function memberEmphasis(on, member, clickable) {
        if (!clickable) {
            return KanteButton.Emphasis.Quiet
        }
        if (composing) {
            return member ? KanteButton.Emphasis.Data : KanteButton.Emphasis.Normal
        }
        return on ? KanteButton.Emphasis.Primary : KanteButton.Emphasis.Normal
    }
    function memberTip(member) {
        return member ? qsTr("In the collection: click to take it out") : qsTr("Click to put it in the collection")
    }
    function setMood(id) {
        shell.act({ action: "set_mood", mood: id })
    }
    function back() {
        if (!composing && now.previous && placeOf(now.previous)) {
            setMood(now.previous)
        }
    }
    /** The collection with `id` (a mood) or the pad `cueId` in or out. */
    function toggleMember(id, cueId) {
        if (!collection) {
            return
        }
        const copy = JSON.parse(JSON.stringify(moods))
        const target = copy.collections.find(c => c.id === collection.id)
        const key = cueId ? "pads" : "moods"
        const value = cueId || id
        const list = target[key] || []
        const at = list.indexOf(value)
        if (at >= 0) {
            list.splice(at, 1)
        } else {
            list.push(value)
        }
        target[key] = list
        shell.act({ action: "set_moods", moods: copy })
    }

    tools: [
        KanteButton {
            focusPolicy: Qt.NoFocus
            objectName: "editMoods"
            visible: root.show !== null && root.show !== undefined
            text: qsTr("Edit…")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Groups, variants, layers, collections and the panic mood")
            onClicked: editDialog.open()
        },
        KanteButton {
            focusPolicy: Qt.NoFocus
            objectName: "compose"
            visible: root.collection !== null
            checkable: true
            checked: root.composing
            text: root.composing ? qsTr("Done") : qsTr("Compose")
            QQC2.ToolTip.visible: hovered
            QQC2.ToolTip.text: qsTr("Put moods and one-shots in or out of %1").arg(root.collection ? root.collection.name : "")
            onClicked: root.composing = !root.composing
        }
    ]

    MoodsDialog {
        id: editDialog
        objectName: "moodsDialog"
        parent: QQC2.Overlay.overlay
        anchors.centerIn: parent
        act: root.shell.act
        show: root.show
        core: root.shell.backend || null
        askRemove: root.shell.askRemoveWith
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: Kirigami.Units.smallSpacing
        spacing: Kirigami.Units.smallSpacing

        // Collections, or a narrow rail to open them.
        KanteCard {
            objectName: "collections"
            Layout.fillHeight: true
            Layout.preferredWidth: root.collectionsOpen ? Kirigami.Units.gridUnit * 12 : railButton.implicitWidth + Kirigami.Units.smallSpacing * 2

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    Layout.fillWidth: true
                    KanteSectionLabel {
                        Layout.fillWidth: true
                        visible: root.collectionsOpen
                        text: qsTr("Collections")
                    }
                    KanteToolButton {
                        id: railButton
                        focusPolicy: Qt.NoFocus
                        objectName: "foldCollections"
                        icon.name: root.collectionsOpen ? "sidebar-collapse-left" : "sidebar-expand-left"
                        text: root.collectionsOpen ? qsTr("Fold collections") : qsTr("Collections")
                        display: QQC2.AbstractButton.IconOnly
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: text
                        onClicked: root.collectionsOpen = !root.collectionsOpen
                    }
                }

                Repeater {
                    model: root.collectionsOpen ? [null].concat(root.collections) : []
                    delegate: KanteListRow {
                        id: row
                        required property var modelData
                        focusOnClick: false
                        readonly property string cid: modelData ? modelData.id : ""
                        objectName: modelData ? "collection:" + modelData.name : "collection:"
                        Layout.fillWidth: true
                        text: modelData ? modelData.name : qsTr("All")
                        subtitle: modelData
                            ? root.moodCount((modelData.moods || []).length) + " · " + root.shotCount((modelData.pads || []).filter(id => root.padIds.has(id)).length)
                            : qsTr("Every mood and one-shot")
                        selected: root.shownCollectionId === cid
                        onClicked: {
                            root.collectionId = cid
                            root.composing = false
                        }
                    }
                }
                Item { Layout.fillHeight: true }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Kirigami.Units.smallSpacing

            // Now: the mood set, the change in progress, back.
            KanteCard {
                objectName: "now"
                Layout.fillWidth: true
                implicitHeight: nowRow.implicitHeight + Kirigami.Units.largeSpacing * 2

                RowLayout {
                    id: nowRow
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.largeSpacing
                    spacing: Kirigami.Units.largeSpacing

                    Rectangle {
                        Layout.preferredWidth: Kirigami.Units.smallSpacing
                        Layout.fillHeight: true
                        color: root.panicked ? KanteStyle.negativeTextColor
                            : root.current && root.current.group.color
                            ? KanteStyle.labelColor(root.current.group.color) : KanteStyle.frameColor
                    }
                    // After a panic the panic mood is what holds now, not a mood.
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 0
                        KanteSectionLabel {
                            text: qsTr("Now")
                        }
                        QQC2.Label {
                            objectName: "nowName"
                            Layout.fillWidth: true
                            elide: Text.ElideRight
                            text: root.panicked ? qsTr("Panic mood")
                                : root.current ? root.placeName(root.current)
                                : root.currentGone ? qsTr("Mood (gone)") : qsTr("No mood set")
                            font.pointSize: Kirigami.Theme.defaultFont.pointSize * 1.6
                            font.bold: true
                            color: root.panicked ? KanteStyle.negativeTextColor
                                : root.current ? KanteStyle.strongTextColor : KanteStyle.mutedTextColor
                        }
                    }
                    // Always its room, shown or not: the name beside it keeps
                    // its width while a change fades.
                    ColumnLayout {
                        objectName: "fadeColumn"
                        readonly property real fixedWidth: Kirigami.Units.gridUnit * 10
                        opacity: root.fading ? 1 : 0
                        Layout.preferredWidth: fixedWidth
                        Layout.minimumWidth: fixedWidth
                        Layout.maximumWidth: fixedWidth
                        KanteProgressBar {
                            objectName: "fade"
                            Layout.fillWidth: true
                            // Unseen while nothing fades: not read out either.
                            Accessible.ignored: !root.fading
                            segments: 20
                            value: root.fadeProgress
                        }
                        QQC2.Label {
                            objectName: "fadeLeft"
                            Layout.fillWidth: true
                            Accessible.ignored: !root.fading
                            text: root.now.fading ? root.secondsLeft(root.now.fading.duration_ms * (1 - root.fadeProgress)) : ""
                            color: KanteStyle.mutedTextColor
                        }
                    }
                    KanteButton {
                        focusPolicy: Qt.NoFocus
                        objectName: "back"
                        readonly property var before: root.placeOf(root.now.previous)
                        enabled: before !== null
                        text: before ? qsTr("Back to %1").arg(root.placeName(before)) : qsTr("Back")
                        onClicked: root.back()
                    }
                }
            }

            KanteEmptyState {
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: root.groups.length === 0
                title: qsTr("No moods yet")
                text: qsTr("A mood says what runs on each layer: light, music, ambience, picture.")
                KanteButton {
                    visible: root.show !== null && root.show !== undefined
                    emphasis: KanteButton.Emphasis.Primary
                    text: qsTr("Set up moods…")
                    onClicked: {
                        editDialog.tab = root.layers.length === 0 ? 1 : 0
                        editDialog.open()
                    }
                }
            }

            // Groups: the head sets the base, a tile a variant.
            QQC2.ScrollView {
                id: groupsView
                objectName: "groups"
                Layout.fillWidth: true
                Layout.fillHeight: true
                visible: root.groups.length > 0
                focus: true
                contentWidth: availableWidth

                ColumnLayout {
                    width: groupsView.availableWidth
                    spacing: Kirigami.Units.smallSpacing

                    Repeater {
                        model: root.shownGroups
                        delegate: KanteCard {
                            id: groupCard
                            required property var modelData
                            required property int index
                            readonly property var group: modelData
                            readonly property bool here: root.current !== null && root.current.group.id === group.id
                            readonly property color tint: group.color ? KanteStyle.labelColor(group.color) : KanteStyle.frameColor
                            objectName: "group:" + group.name
                            Layout.fillWidth: true
                            implicitHeight: groupColumn.implicitHeight + Kirigami.Units.smallSpacing * 2

                            ColumnLayout {
                                id: groupColumn
                                anchors.fill: parent
                                anchors.margins: Kirigami.Units.smallSpacing
                                spacing: Kirigami.Units.smallSpacing

                                RowLayout {
                                    Layout.fillWidth: true
                                    Rectangle {
                                        Layout.preferredWidth: Kirigami.Units.smallSpacing
                                        Layout.preferredHeight: baseButton.height
                                        color: groupCard.tint
                                    }
                                    KanteButton {
                                        id: baseButton
                                        focusPolicy: Qt.NoFocus
                                        objectName: "base:" + groupCard.group.name
                                        readonly property bool on: groupCard.here && root.current.variant === null
                                        readonly property bool member: root.inCollection(groupCard.group.id)
                                        Layout.fillWidth: true
                                        size: KanteButton.Size.Large
                                        // Not in the collection: only the group's name, no
                                        // button to look at (it holds only variants).
                                        emphasis: root.memberEmphasis(on, member, enabled)
                                        enabled: root.composing || !root.collection || member
                                        checkable: root.composing
                                        checked: root.composing && member
                                        icon.name: root.composing && member ? "checkmark" : ""
                                        text: (groupCard.index < 9 ? (groupCard.index + 1) + "  " : "") + groupCard.group.name
                                        Accessible.name: qsTr("Set %1").arg(groupCard.group.name)
                                        QQC2.ToolTip.visible: hovered
                                        QQC2.ToolTip.text: groupCard.group.name + "\n" + (root.composing
                                            ? root.memberTip(member)
                                            : qsTr("Base: %1").arg(root.layers.map(l => l.name + " " + root.describe(root.contentOf({ group: groupCard.group, variant: null }, l.id).content)).join(" · ")))
                                        onClicked: root.composing ? root.toggleMember(groupCard.group.id) : root.setMood(groupCard.group.id)
                                    }
                                }

                                Flow {
                                    Layout.fillWidth: true
                                    Layout.leftMargin: Kirigami.Units.largeSpacing
                                    spacing: Kirigami.Units.smallSpacing
                                    visible: (groupCard.group.variants || []).length > 0

                                    Repeater {
                                        model: (groupCard.group.variants || []).filter(v => root.composing || !root.collection || root.inCollection(v.id))
                                        delegate: KanteButton {
                                            id: variantButton
                                            required property var modelData
                                            focusPolicy: Qt.NoFocus
                                            readonly property var variant: modelData
                                            readonly property bool on: groupCard.here && root.current.variant !== null && root.current.variant.id === variant.id
                                            readonly property bool member: root.inCollection(variant.id)
                                            readonly property var own: root.layers.filter(l => variant.layers && variant.layers[l.id] !== undefined).map(l => l.name)
                                            objectName: "variant:" + groupCard.group.name + "/" + variant.name
                                            width: Kirigami.Units.gridUnit * 10
                                            emphasis: root.memberEmphasis(on, member, true)
                                            checkable: root.composing
                                            checked: root.composing && member
                                            icon.name: root.composing && member ? "checkmark" : ""
                                            text: variant.name
                                            Accessible.name: qsTr("Set %1").arg(groupCard.group.name + " / " + variant.name)
                                            // The whole name: a long one is cut on the tile.
                                            readonly property string tip: variant.name + "\n" + (root.composing ? root.memberTip(member)
                                                : own.length > 0 ? qsTr("Changes %1; the rest from the base").arg(own.join(", "))
                                                : qsTr("Same as the base"))
                                            QQC2.ToolTip.visible: hovered
                                            QQC2.ToolTip.text: tip
                                            onClicked: root.composing ? root.toggleMember(variant.id) : root.setMood(variant.id)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // One-shots: the show's pads, over the mood.
            KanteCard {
                objectName: "oneShots"
                Layout.fillWidth: true
                visible: root.pads.length > 0
                implicitHeight: shotsColumn.implicitHeight + Kirigami.Units.smallSpacing * 2

                ColumnLayout {
                    id: shotsColumn
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.smallSpacing
                    spacing: Kirigami.Units.smallSpacing

                    KanteSectionLabel {
                        text: root.composing ? qsTr("One-shots: click to put in or out") : qsTr("One-shots: over the mood, a click starts or stops")
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: Kirigami.Units.smallSpacing
                        Repeater {
                            model: root.shownPads
                            delegate: KanteButton {
                                required property var modelData
                                focusPolicy: Qt.NoFocus
                                readonly property var cue: modelData.cue
                                readonly property bool runs: root.runningIds.indexOf(cue.id) >= 0
                                readonly property bool member: root.collectionPads.has(cue.id)
                                objectName: "pad:" + cue.name
                                width: Kirigami.Units.gridUnit * 9
                                emphasis: root.memberEmphasis(runs, member, true)
                                checkable: root.composing
                                checked: root.composing && member
                                icon.name: root.composing && member ? "checkmark" : ""
                                text: cue.name !== "" ? cue.name : cue.kind
                                QQC2.ToolTip.visible: hovered
                                QQC2.ToolTip.text: text + (root.composing ? "\n" + root.memberTip(member) : "")
                                Accessible.name: runs ? qsTr("Stop %1").arg(text) : qsTr("Start %1").arg(text)
                                onClicked: root.composing
                                    ? root.toggleMember("", cue.id)
                                    : root.shell.act({ action: "cue_toggle", list: modelData.list.number, cue: cue.id })
                            }
                        }
                    }
                }
            }
        }

        // Inspector: the layers of the mood now and where each comes from.
        KanteCard {
            objectName: "inspector"
            Layout.fillHeight: true
            Layout.preferredWidth: root.inspectorOpen ? Kirigami.Units.gridUnit * 16 : foldInspector.implicitWidth + Kirigami.Units.smallSpacing * 2

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: Kirigami.Units.smallSpacing
                spacing: Kirigami.Units.smallSpacing

                RowLayout {
                    Layout.fillWidth: true
                    KanteToolButton {
                        id: foldInspector
                        focusPolicy: Qt.NoFocus
                        objectName: "foldInspector"
                        icon.name: root.inspectorOpen ? "sidebar-collapse-right" : "sidebar-expand-right"
                        text: root.inspectorOpen ? qsTr("Fold layers") : qsTr("Layers")
                        display: QQC2.AbstractButton.IconOnly
                        QQC2.ToolTip.visible: hovered
                        QQC2.ToolTip.text: text
                        onClicked: root.inspectorOpen = !root.inspectorOpen
                    }
                    KanteSectionLabel {
                        Layout.fillWidth: true
                        visible: root.inspectorOpen
                        text: qsTr("Layers")
                    }
                }

                Repeater {
                    model: root.inspectorOpen ? root.layers : []
                    delegate: ColumnLayout {
                        id: layerRow
                        required property var modelData
                        readonly property var origin: (root.now.layers || []).find(l => l.layer === modelData.id)
                        readonly property var has: root.current ? root.contentOf(root.current, modelData.id) : null
                        objectName: "layer:" + modelData.name
                        /** Where the layer comes from, as the page says it. */
                        readonly property string kind: origin ? origin.origin.kind : ""
                        // A mood removed since (lmix-core may still hold its
                        // layers): no base or variant to name.
                        readonly property string source: kind === "" ? ""
                            : kind === "panic" ? qsTr("Panic")
                            : kind === "cue" ? qsTr("Cue")
                            : has === null ? qsTr("Mood (gone)")
                            : has.own ? root.current.variant.name : qsTr("Base")
                        Layout.fillWidth: true
                        spacing: 0

                        RowLayout {
                            Layout.fillWidth: true
                            QQC2.Label {
                                Layout.fillWidth: true
                                text: layerRow.modelData.name
                                color: KanteStyle.mutedTextColor
                            }
                            KantePill {
                                visible: layerRow.source !== ""
                                text: layerRow.source
                                status: layerRow.kind === "panic" ? KantePill.State.Failed
                                    : layerRow.kind === "cue" ? KantePill.State.Review
                                    : layerRow.has === null ? KantePill.State.Off : KantePill.State.Running
                            }
                        }
                        QQC2.Label {
                            objectName: "layerText:" + layerRow.modelData.name
                            Layout.fillWidth: true
                            elide: Text.ElideMiddle
                            visible: text !== ""
                            text: layerRow.kind === "cue" ? root.cueLabel(layerRow.origin.origin)
                                : layerRow.kind === "panic" ? qsTr("Panic mood")
                                : layerRow.origin && layerRow.has ? root.describe(layerRow.has.content)
                                : layerRow.origin ? "" : qsTr("Off")
                            color: layerRow.has && !layerRow.has.own && root.current.variant ? KanteStyle.mutedTextColor : KanteStyle.textColor
                        }
                    }
                }
                Item { Layout.fillHeight: true }
            }
        }
    }
}
