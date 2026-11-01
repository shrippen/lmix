import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * A layout: background input and up to eight boxes. Boxes are placed by
 * dragging them on a 16:9 area (corner handle: size; arrow keys: move,
 * with Shift: size) or from preset places; each can be framed, cropped
 * (zoom into its input) and locked in place. Shift+click chooses more
 * boxes, which then move together. New layouts get a name. Changed
 * live while open (a cue swaps boxes): Save sends only what was changed
 * here and keeps the rest as it is now; removed meanwhile: Save adds it
 * as a new layout.
 */
KanteDialog {
    id: dialog

    required property var act
    required property var inputs
    /** Layout being edited; `null` adds one. Set before `open()`. */
    property var layout: null
    /** `layouts` of the show state: the edited one as it is now. */
    property var layouts: []

    /** Inputs with a picture. */
    readonly property var pictures: inputs.filter(i => i.source.kind !== "audio_device")
    // The boxes' choices change only with a name or number, followed while
    // the dialog is open: the inputs change with every fader move.
    property var pictureNames: []
    function syncPictureNames() {
        const names = pictures.map(i => i.number + " " + i.name)
        if (JSON.stringify(names) !== JSON.stringify(pictureNames)) {
            pictureNames = names
        }
    }
    onPicturesChanged: if (visible) syncPictureNames()
    onAboutToShow: syncPictureNames()
    readonly property int maxBoxes: 8
    readonly property int frameWidth: 4
    /** Positions snap to this fraction of the picture. */
    readonly property real grid: 0.01
    /** Smallest box side, fraction of the picture. */
    readonly property real minSide: 0.05

    /** Working copy: [{ input, rect: {x,y,w,h}, framed, crop: {left,top,right,bottom}, locked, hidden }]. */
    property var boxes: []
    /** The box the crop and pixel values are for; -1 none. */
    property int selected: -1
    /** Indices of all chosen boxes (LAY.6), `selected` among them. */
    property var chosen: []
    /** How far the box being dragged has moved, in area pixels: the other
     *  chosen boxes follow it. */
    property point dragDelta: Qt.point(0, 0)

    function isChosen(index) {
        return chosen.indexOf(index) >= 0
    }
    /** Selects box `index` alone, or with `add` adds it to (or takes it
     *  from) the chosen boxes. */
    function select(index, add) {
        if (!add || index < 0) {
            selected = index
            chosen = index >= 0 ? [index] : []
            return
        }
        if (isChosen(index) && chosen.length > 1) {
            chosen = chosen.filter(i => i !== index)
            if (selected === index)
                selected = chosen[chosen.length - 1]
        } else {
            if (!isChosen(index))
                chosen = chosen.concat([index])
            selected = index
        }
    }
    /** Chosen boxes that may move. */
    function movable() {
        return chosen.filter(i => i < boxes.length && !boxes[i].locked)
    }

    /** Places a box can take, as fractions of the picture. */
    readonly property var places: [
        { label: qsTr("Top right quarter"), rect: { x: 0.5, y: 0, w: 0.5, h: 0.5 } },
        { label: qsTr("Top left quarter"), rect: { x: 0, y: 0, w: 0.5, h: 0.5 } },
        { label: qsTr("Bottom right quarter"), rect: { x: 0.5, y: 0.5, w: 0.5, h: 0.5 } },
        { label: qsTr("Bottom left quarter"), rect: { x: 0, y: 0.5, w: 0.5, h: 0.5 } },
        { label: qsTr("Left half"), rect: { x: 0, y: 0.25, w: 0.5, h: 0.5 } },
        { label: qsTr("Right half"), rect: { x: 0.5, y: 0.25, w: 0.5, h: 0.5 } },
        { label: qsTr("Small, top right"), rect: { x: 0.7, y: 0.05, w: 0.25, h: 0.25 } },
        { label: qsTr("Small, bottom right"), rect: { x: 0.7, y: 0.7, w: 0.25, h: 0.25 } },
        { label: qsTr("Small, bottom left"), rect: { x: 0.05, y: 0.7, w: 0.25, h: 0.25 } }
    ]

    function placeOf(rect) {
        var near = (a, b) => Math.abs(a - b) < 0.005
        return places.findIndex(p => near(p.rect.x, rect.x) && near(p.rect.y, rect.y) && near(p.rect.w, rect.w) && near(p.rect.h, rect.h))
    }

    function pictureIndex(id) {
        return pictures.findIndex(i => i.id === id)
    }

    function inputLabel(id) {
        var input = pictures.find(i => i.id === id)
        return input ? input.number + " " + input.name : qsTr("Empty")
    }

    function snap(v) {
        return Math.round(v / grid) * grid
    }

    /** Replaces box `index` with `changes` merged in. */
    function edit(index, changes) {
        var list = boxes.slice()
        list[index] = Object.assign({}, list[index], changes)
        boxes = list
    }

    /** Bounds of the chosen boxes that may move: {x0, y0, x1, y1}. */
    function groupBounds() {
        const rects = movable().map(i => boxes[i].rect)
        return {
            x0: Math.min(...rects.map(r => r.x)), y0: Math.min(...rects.map(r => r.y)),
            x1: Math.max(...rects.map(r => r.x + r.w)), y1: Math.max(...rects.map(r => r.y + r.h))
        }
    }
    /** Moves the chosen boxes that are not locked by (dx, dy), together
     *  kept inside the picture: one step. All by the same amount, so the
     *  gaps between them stay (no snapping per box). */
    function moveChosen(dx, dy) {
        const moving = movable()
        if (moving.length === 0)
            return
        const b = groupBounds()
        dx = Math.min(Math.max(dx, -b.x0), 1 - b.x1)
        dy = Math.min(Math.max(dy, -b.y0), 1 - b.y1)
        // Rounded to 1/10000: no float dust from repeated steps.
        const tidy = v => Math.round(v * 10000) / 10000
        const list = boxes.slice()
        for (const i of moving) {
            const r = list[i].rect
            list[i] = Object.assign({}, list[i], { rect: { x: tidy(r.x + dx), y: tidy(r.y + dy), w: r.w, h: r.h } })
        }
        boxes = list
    }

    /** How near (of the picture) a box edge or centre has to come to catch. */
    readonly property real catchDistance: 0.015
    /** Action-safe margin, as the titles keep it (5 %). */
    readonly property real safe: 0.05

    /** Where box `index` (width `w`) would start along one axis to catch
     *  `pos` on an edge or centre of the picture, the safe area or another
     *  box (LAY.6); `pos` when nothing is near. `axis` "x" or "y". */
    function caught(index, pos, w, axis) {
        const targets = [0, 0.5, 1, safe, 1 - safe]
        // Boxes moving along do not catch.
        const moving = isChosen(index) ? movable() : [index]
        boxes.forEach((b, i) => {
            if (i === index || moving.indexOf(i) >= 0)
                return
            const start = axis === "x" ? b.rect.x : b.rect.y
            const size = axis === "x" ? b.rect.w : b.rect.h
            targets.push(start, start + size, start + size / 2)
        })
        let best = pos, distance = catchDistance
        for (const t of targets) {
            // The box's start, end and centre on the target.
            for (const p of [t, t - w, t - w / 2]) {
                if (Math.abs(p - pos) < distance) {
                    distance = Math.abs(p - pos)
                    best = p
                }
            }
        }
        return best
    }

    /** Moves box `index` to (x, y): catching edges and centres, else on
     *  the 1 % grid; `free` (Ctrl held) exactly where it was let go. The
     *  other chosen boxes move by as much. */
    function placeAt(index, x, y, free) {
        const r = boxes[index].rect
        const along = (v, w, axis) => {
            if (free)
                return v
            const c = caught(index, v, w, axis)
            return c === v ? snap(v) : c
        }
        const cx = along(x, r.w, "x"), cy = along(y, r.h, "y")
        if (isChosen(index) && movable().length > 1) {
            moveChosen(cx - r.x, cy - r.y)
            return
        }
        edit(index, { rect: { x: Math.min(Math.max(cx, 0), 1 - r.w), y: Math.min(Math.max(cy, 0), 1 - r.h), w: r.w, h: r.h } })
    }

    // Undo and redo (LAY.6): every change of the boxes is a step.
    property var history: []
    property int historyAt: -1
    property bool restoring: false
    /** The boxes and the background as one step. */
    function snapshot() {
        return JSON.stringify({ boxes: boxes, background: background.currentIndex })
    }
    function record() {
        if (restoring)
            return
        const step = snapshot()
        // A drag caught back where it was is no step.
        if (history[historyAt] === step)
            return
        const steps = history.slice(0, historyAt + 1)
        steps.push(step)
        history = steps.slice(-100)
        historyAt = history.length - 1
    }
    onBoxesChanged: record()
    function undo() { stepTo(historyAt - 1) }
    function redo() { stepTo(historyAt + 1) }
    function stepTo(at) {
        if (at < 0 || at >= history.length)
            return
        restoring = true
        historyAt = at
        const step = JSON.parse(history[at])
        boxes = step.boxes
        background.currentIndex = step.background
        select(Math.min(selected, boxes.length - 1), false)
        restoring = false
    }
    /** Output pixels of a fraction (1920×1080 picture). */
    function px(v, axis) { return Math.round(v * (axis === "x" ? 1920 : 1080)) }

    /** Sizes box `index` from its top left corner, kept inside the picture. */
    function sizeTo(index, w, h) {
        var r = boxes[index].rect
        var fit = (v, room) => Math.min(Math.max(snap(v), minSide), room)
        edit(index, { rect: { x: r.x, y: r.y, w: fit(w, 1 - r.x), h: fit(h, 1 - r.y) } })
    }

    function cropValid(c) {
        return 1 - c.left - c.right >= 0.05 && 1 - c.top - c.bottom >= 0.05
    }

    readonly property bool valid: boxes.every(b => b.input !== null && cropValid(b.crop))

    title: layout ? qsTr("Layout %1 %2").arg(layout.number).arg(layout.name) : qsTr("New layout")
    modal: true
    width: Kirigami.Units.gridUnit * 40
    standardButtons: QQC2.Dialog.Ok | QQC2.Dialog.Cancel
    acceptText: gone ? qsTr("Save as new layout") : layout ? qsTr("Save layout") : qsTr("Add layout")
    // KanteDialog has a child of its own (KanteScope), so the popup does not
    // size itself to this one.
    contentHeight: column.implicitHeight

    acceptEnabled: dialog.valid

    onOpened: {
        nameField.text = ""
        load(layout)
    }

    /** The layout as it was when the dialog took it: what Save compares
     *  the working copy with. */
    property var base: null
    /** The edited layout as it is now; null when adding or gone. */
    readonly property var live: visible ? liveNow() : null
    function liveNow() {
        return base ? layouts.find(l => l.id === base.id) || null : null
    }
    /** The live layout has other boxes or a background than when it was
     *  taken (a cue, shortcut or another console changed it). */
    readonly property bool changedMeanwhile: live !== null && !sameLayout(live, base)
    /** Same boxes in the same order: as many, each in its place with its
     *  frame and lock. Changes on air (swap_boxes, set_box_input,
     *  show_box) keep that; another console saving other boxes (one
     *  removed and one added, reordered) does not. */
    function sameBoxes(a, b) {
        if (a.boxes.length !== b.boxes.length)
            return false
        return a.boxes.every((box, i) => {
            const p = fromSpec(box), q = fromSpec(b.boxes[i])
            return ["rect", "framed", "locked"].every(part => samePart(p, q, part))
        })
    }
    /** Changed meanwhile so that the edits cannot be laid over it (see
     *  `mix`): Save replaces it with what the dialog shows. */
    readonly property bool replacesLive: changedMeanwhile && mix(chosenBackground()) === null
    /** Removed meanwhile (another console, an undo): nothing to save to,
     *  Save adds the working copy as a new layout. */
    readonly property bool gone: visible && base !== null && live === null

    /** Takes `l` (null: a new layout) into the dialog, dropping what was changed here. */
    function load(l) {
        base = l
        // Combo index 0 is "none": inputs start at 1.
        background.currentIndex = l && l.background ? pictureIndex(l.background) + 1 : 0
        boxes = l ? l.boxes.map(fromSpec) : []
        select(boxes.length > 0 ? 0 : -1, false)
        history = [snapshot()]
        historyAt = 0
    }
    function reload() {
        if (live)
            load(live)
    }

    // What a box consists of, in parts that change on their own. The crop
    // goes with the input (swap_boxes moves both, set_box_input clears it).
    function samePart(a, b, part) {
        const same = (p, q, keys) => keys.every(k => p[k] === q[k])
        switch (part) {
        case "picture": return a.input === b.input && same(a.crop, b.crop, ["left", "top", "right", "bottom"])
        case "rect": return same(a.rect, b.rect, ["x", "y", "w", "h"])
        default: return a[part] === b[part]
        }
    }
    readonly property var parts: ({ picture: ["input", "crop"], rect: ["rect"], framed: ["framed"], locked: ["locked"], hidden: ["hidden"] })
    function sameLayout(a, b) {
        if ((a.background || null) !== (b.background || null) || a.boxes.length !== b.boxes.length)
            return false
        return a.boxes.every((box, i) => {
            const p = fromSpec(box), q = fromSpec(b.boxes[i])
            return Object.keys(parts).every(part => samePart(p, q, part))
        })
    }

    function chosenBackground() {
        return background.currentIndex > 0 ? pictures[background.currentIndex - 1].id : null
    }

    /** The working copy over the live layout: a part of a box changed here
     *  wins, every other part is taken as it is now. Boxes are matched by
     *  their place in the list. Null when that cannot be done: other boxes
     *  elsewhere (see `sameBoxes`), or a mix that would show an input
     *  twice (lmix-core refuses that). */
    function mix(bg) {
        const current = liveNow()
        if (!current || !sameBoxes(current, base))
            return null
        const was = base.boxes.map(fromSpec), now = current.boxes.map(fromSpec)
        const wasBg = base.background || null
        const result = {
            background: bg === wasBg ? current.background || null : bg,
            boxes: boxes.map(b => {
                if (b.origin < 0)
                    return b
                const out = Object.assign({}, b)
                for (const part in parts) {
                    if (samePart(b, was[b.origin], part))
                        parts[part].forEach(k => out[k] = now[b.origin][k])
                }
                return out
            })
        }
        const used = result.boxes.map(b => b.input).concat([result.background]).filter(id => id !== null)
        return used.every((id, i) => used.indexOf(id) === i) ? result : null
    }
    /** What Save sends: the mix, else the working copy as it is. */
    function merged(bg) {
        return mix(bg) || { background: bg, boxes: boxes }
    }

    /** A box of a layout as the dialog holds it. */
    function fromSpec(b, origin) {
        return {
            /** Index in the layout it came from; -1 added here. */
            origin: origin === undefined ? -1 : origin,
            input: b.input,
            rect: Object.assign({}, b.rect),
            framed: b.border_px > 0,
            crop: Object.assign({ left: 0, top: 0, right: 0, bottom: 0 }, b.crop || {}),
            locked: b.locked === true,
            hidden: b.hidden === true
        }
    }

    function removeBox(index) {
        var list = boxes.slice()
        list.splice(index, 1)
        boxes = list
        select(Math.min(selected, list.length - 1), false)
    }

    function addBox() {
        // New boxes start small in the top right, a step lower each.
        var n = boxes.length
        var y = Math.min(0.05 + 0.1 * n, 0.7)
        boxes = boxes.concat([{
            origin: -1,
            input: null,
            rect: { x: 0.7, y: y, w: 0.25, h: 0.25 },
            framed: false,
            crop: { left: 0, top: 0, right: 0, bottom: 0 },
            locked: false,
            hidden: false
        }])
        select(n, false)
    }

    /** `name`, or "name (2)", "name (3)" … when another layout has it
     *  (as lmix-core names a second input; without case). */
    function freeName(name) {
        const taken = layouts.map(l => l.name.toLowerCase())
        let free = name
        for (let n = 2; taken.indexOf(free.toLowerCase()) >= 0; n++)
            free = name + " (" + n + ")"
        return free
    }

    onAccepted: {
        var bg = chosenBackground()
        var result = merged(bg)
        var specs = result.boxes.map(b => ({
            rect: b.rect,
            input: b.input,
            border_px: b.framed ? frameWidth : 0,
            crop: b.crop,
            locked: b.locked,
            hidden: b.hidden
        }))
        if (layout && !liveNow())
            // Removed while open: the work is kept as a new layout.
            act({ action: "add_layout", name: freeName(base.name), background: bg, boxes: specs })
        else if (layout)
            // By id: a number would hit another layout once one before it
            // is removed while the dialog is open.
            act({ action: "set_layout", layout: String(base.id), background: result.background, boxes: specs })
        else
            act({ action: "add_layout", name: nameField.text.trim() || null, background: bg, boxes: specs })
    }

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        // Live operation changes layouts while one is edited here: say so.
        KanteCallout {
            objectName: "changedHint"
            Layout.fillWidth: true
            visible: dialog.changedMeanwhile || dialog.gone
            kind: dialog.gone || dialog.replacesLive ? KanteCallout.Kind.Warn : KanteCallout.Kind.Info
            text: dialog.gone ? qsTr("This layout was removed while the dialog was open. Save it as a new layout to keep what is shown here.")
                : dialog.replacesLive
                ? qsTr("This layout was changed elsewhere while the dialog was open. Saving replaces it with what is shown here.")
                : qsTr("This layout was changed while the dialog was open. Saving keeps those changes wherever you changed nothing here.")
            KanteButton {
                visible: !dialog.gone
                text: qsTr("Reload")
                onClicked: dialog.reload()
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: dialog.layout === null
            KanteSectionLabel { text: qsTr("Name") }
            KanteTextField {
                id: nameField
                Layout.fillWidth: true
                placeholderText: qsTr("optional")
            }
        }

        RowLayout {
            Layout.fillWidth: true
            KanteSectionLabel { text: qsTr("Background") }
            QQC2.ComboBox {
                id: background
                Layout.fillWidth: true
                model: [qsTr("Black")].concat(dialog.pictureNames)
                onActivated: dialog.record()
                KanteFieldSkin { control: background }
            }
        }

        // The picture, 16:9: boxes bottom to top.
        Rectangle {
            id: area
            Layout.fillWidth: true
            Layout.preferredHeight: width * 9 / 16
            color: background.currentIndex > 0 ? KanteStyle.sunkenColor : "black"
            border.color: KanteStyle.frameColor
            focus: true
            activeFocusOnTab: true
            Accessible.role: Accessible.Pane
            Accessible.name: qsTr("Layout area: arrow keys move the chosen boxes, with Shift they size the selected one")

            QQC2.Label {
                anchors.centerIn: parent
                visible: dialog.boxes.length === 0
                color: KanteStyle.mutedTextColor
                text: qsTr("Add a box, then drag it into place")
            }

            // Undo and redo of the boxes are the area's own: taken before a
            // shortcut of the application (the Undo of a removal's toast).
            Keys.onShortcutOverride: event => event.accepted = event.matches(StandardKey.Undo) || event.matches(StandardKey.Redo)
            Keys.onPressed: event => {
                if (event.matches(StandardKey.Undo)) {
                    dialog.undo()
                    event.accepted = true
                    return
                }
                if (event.matches(StandardKey.Redo)) {
                    dialog.redo()
                    event.accepted = true
                    return
                }
                var i = dialog.selected
                if (i < 0)
                    return
                var dx = event.key === Qt.Key_Left ? -dialog.grid : event.key === Qt.Key_Right ? dialog.grid : 0
                var dy = event.key === Qt.Key_Up ? -dialog.grid : event.key === Qt.Key_Down ? dialog.grid : 0
                if (dx === 0 && dy === 0)
                    return
                var r = dialog.boxes[i].rect
                if (event.modifiers & Qt.ShiftModifier) {
                    if (!dialog.boxes[i].locked)
                        dialog.sizeTo(i, r.w + dx, r.h + dy)
                } else {
                    dialog.moveChosen(dx, dy)
                }
                event.accepted = true
            }

            Repeater {
                model: dialog.boxes

                Rectangle {
                    id: boxItem
                    required property var modelData
                    required property int index
                    // A hidden box (LAY.2) stays editable, pale.
                    opacity: modelData.hidden ? 0.4 : 1
                    readonly property bool isSelected: dialog.selected === index
                    readonly property bool isChosen: dialog.chosen.indexOf(index) >= 0
                    /** Moves along while another chosen box is dragged. */
                    readonly property bool follows: isChosen && !modelData.locked && !dragArea.drag.active
                    /** Dragged with others: the whole group stays in the picture. */
                    readonly property bool leadsGroup: isChosen && !modelData.locked && dialog.movable().length > 1
                    readonly property var bounds: leadsGroup ? dialog.groupBounds() : null
                    readonly property var crop: modelData.crop
                    x: modelData.rect.x * area.width + (follows ? dialog.dragDelta.x : 0)
                    y: modelData.rect.y * area.height + (follows ? dialog.dragDelta.y : 0)
                    width: modelData.rect.w * area.width
                    height: modelData.rect.h * area.height
                    z: index
                    color: Qt.rgba(KanteStyle.accentColor.r, KanteStyle.accentColor.g, KanteStyle.accentColor.b, isSelected ? 0.55 : isChosen ? 0.45 : 0.3)
                    border.width: modelData.framed ? 3 : 1
                    border.color: modelData.framed ? "white" : KanteStyle.frameColor

                    // Chosen: a focus outline around the box.
                    Rectangle {
                        anchors.fill: parent
                        anchors.margins: -3
                        visible: boxItem.isChosen
                        color: "transparent"
                        border.width: 2
                        border.color: KanteStyle.focusColor
                    }

                    QQC2.Label {
                        anchors.centerIn: parent
                        width: parent.width - 8
                        horizontalAlignment: Text.AlignHCenter
                        elide: Text.ElideRight
                        text: (boxItem.index + 1) + ": " + dialog.inputLabel(boxItem.modelData.input)
                            + (boxItem.crop.left + boxItem.crop.top + boxItem.crop.right + boxItem.crop.bottom > 0 ? qsTr(" (cropped)") : "")
                            + (boxItem.modelData.locked ? qsTr(" (locked)") : "")
                            + (boxItem.modelData.hidden ? qsTr(" (hidden)") : "")
                    }

                    // Dragging moves the item itself; the box changes on
                    // release (changing `boxes` rebuilds the delegates).
                    MouseArea {
                        id: dragArea
                        anchors.fill: parent
                        cursorShape: boxItem.modelData.locked ? Qt.ArrowCursor : Qt.SizeAllCursor
                        drag.target: boxItem.modelData.locked ? null : boxItem
                        readonly property var r: boxItem.modelData.rect
                        readonly property var b: boxItem.bounds
                        drag.minimumX: b ? (r.x - b.x0) * area.width : 0
                        drag.minimumY: b ? (r.y - b.y0) * area.height : 0
                        drag.maximumX: b ? (r.x + 1 - b.x1) * area.width : area.width - boxItem.width
                        drag.maximumY: b ? (r.y + 1 - b.y1) * area.height : area.height - boxItem.height
                        drag.threshold: 2
                        property bool shifted: false
                        onPressed: mouse => {
                            shifted = (mouse.modifiers & Qt.ShiftModifier) !== 0
                            // Shift: one more (or one fewer); a chosen box keeps the others to drag them along.
                            if (shifted)
                                dialog.select(boxItem.index, true)
                            else if (boxItem.isChosen)
                                dialog.selected = boxItem.index
                            else
                                dialog.select(boxItem.index, false)
                            area.forceActiveFocus()
                        }
                        onPositionChanged: {
                            // Taken out with Shift: dragged alone.
                            if (drag.active && boxItem.isChosen)
                                dialog.dragDelta = Qt.point(boxItem.x - boxItem.modelData.rect.x * area.width,
                                    boxItem.y - boxItem.modelData.rect.y * area.height)
                        }
                        onReleased: mouse => {
                            dialog.dragDelta = Qt.point(0, 0)
                            const moved = drag.active || boxItem.x !== boxItem.modelData.rect.x * area.width || boxItem.y !== boxItem.modelData.rect.y * area.height
                            if (!boxItem.modelData.locked && moved)
                                dialog.placeAt(boxItem.index, boxItem.x / area.width, boxItem.y / area.height,
                                    (mouse.modifiers & Qt.ControlModifier) !== 0)
                            else if (!moved && !shifted)
                                // A plain click on one of several: that box alone.
                                dialog.select(boxItem.index, false)
                        }
                        onCanceled: {
                            // Grab taken away: everything back where it was.
                            dialog.dragDelta = Qt.point(0, 0)
                            dialog.boxes = dialog.boxes.slice()
                        }
                    }

                    // Size handle, bottom right.
                    Rectangle {
                        width: Kirigami.Units.gridUnit * 0.8
                        height: width
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        visible: boxItem.isSelected && !boxItem.modelData.locked
                        color: KanteStyle.focusColor
                        MouseArea {
                            anchors.fill: parent
                            anchors.margins: -4
                            cursorShape: Qt.SizeFDiagCursor
                            property point start
                            property size origin
                            property bool free: false
                            onPressed: mouse => {
                                start = mapToItem(area, mouse.x, mouse.y)
                                origin = Qt.size(boxItem.width, boxItem.height)
                            }
                            onPositionChanged: mouse => {
                                var p = mapToItem(area, mouse.x, mouse.y)
                                var minPx = dialog.minSide * area.width
                                var w = Math.max(origin.width + p.x - start.x, minPx)
                                // Shift: free; otherwise the box keeps the picture's 16:9.
                                free = (mouse.modifiers & Qt.ShiftModifier) !== 0
                                var h = free ? Math.max(origin.height + p.y - start.y, dialog.minSide * area.height) : w * area.height / area.width
                                boxItem.width = Math.min(w, area.width - boxItem.x)
                                boxItem.height = Math.min(h, area.height - boxItem.y)
                            }
                            onReleased: dialog.sizeTo(boxItem.index, boxItem.width / area.width, boxItem.height / area.height)
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            QQC2.Label {
                Layout.fillWidth: true
                font.family: KanteStyle.monoFamily
                color: KanteStyle.mutedTextColor
                readonly property var r: dialog.selected >= 0 && dialog.selected < dialog.boxes.length ? dialog.boxes[dialog.selected].rect : null
                text: r ? qsTr("Box %1: %2, %3 px · %4 × %5 px (1920 × 1080)").arg(dialog.selected + 1)
                    .arg(dialog.px(r.x, "x")).arg(dialog.px(r.y, "y")).arg(dialog.px(r.w, "x")).arg(dialog.px(r.h, "y")) : ""
                QQC2.ToolTip.visible: posArea.containsMouse
                QQC2.ToolTip.text: qsTr("Boxes catch edges, the centre, the safe area and other boxes; hold Ctrl to place freely. Shift+click chooses more boxes to move together.")
                MouseArea { id: posArea; anchors.fill: parent; hoverEnabled: true; acceptedButtons: Qt.NoButton }
            }
            KanteButton {
                text: qsTr("Undo")
                enabled: dialog.historyAt > 0
                onClicked: dialog.undo()
            }
            KanteButton {
                text: qsTr("Redo")
                enabled: dialog.historyAt < dialog.history.length - 1
                onClicked: dialog.redo()
            }
        }

        KanteSectionLabel { text: qsTr("Boxes (bottom to top)") }
        Repeater {
            model: dialog.boxes

            RowLayout {
                id: boxRow
                required property var modelData
                required property int index
                Layout.fillWidth: true

                QQC2.RadioButton {
                    checked: dialog.selected === boxRow.index
                    text: boxRow.index + 1
                    Accessible.name: qsTr("Select box %1").arg(boxRow.index + 1)
                    onClicked: dialog.select(boxRow.index, false)
                }
                QQC2.ComboBox {
                    id: inputBox
                    Layout.fillWidth: true
                    model: [qsTr("Choose input")].concat(dialog.pictureNames)
                    currentIndex: boxRow.modelData.input ? dialog.pictureIndex(boxRow.modelData.input) + 1 : 0
                    Accessible.name: qsTr("Input of box %1").arg(boxRow.index + 1)
                    onActivated: index => dialog.edit(boxRow.index, { input: index > 0 ? dialog.pictures[index - 1].id : null })
                    KanteFieldSkin { control: inputBox }
                }
                QQC2.ComboBox {
                    id: placeBox
                    Layout.preferredWidth: Kirigami.Units.gridUnit * 10
                    model: [qsTr("Own place")].concat(dialog.places.map(p => p.label))
                    currentIndex: dialog.placeOf(boxRow.modelData.rect) + 1
                    enabled: !boxRow.modelData.locked
                    Accessible.name: qsTr("Place of box %1").arg(boxRow.index + 1)
                    onActivated: index => {
                        if (index > 0)
                            dialog.edit(boxRow.index, { rect: Object.assign({}, dialog.places[index - 1].rect) })
                    }
                    KanteFieldSkin { control: placeBox }
                }
                QQC2.CheckBox {
                    id: frame
                    text: qsTr("Frame")
                    checked: boxRow.modelData.framed
                    onToggled: dialog.edit(boxRow.index, { framed: checked })
                    KanteCheckSkin { control: frame }
                }
                QQC2.CheckBox {
                    id: lock
                    text: qsTr("Lock")
                    checked: boxRow.modelData.locked
                    Accessible.name: qsTr("Lock box %1 in place").arg(boxRow.index + 1)
                    onToggled: dialog.edit(boxRow.index, { locked: checked })
                    KanteCheckSkin { control: lock }
                }
                // LAY.2: kept in the layout, out of the picture.
                QQC2.CheckBox {
                    id: visibleBox
                    text: qsTr("Visible")
                    checked: !boxRow.modelData.hidden
                    Accessible.name: qsTr("Show box %1 in the picture").arg(boxRow.index + 1)
                    onToggled: dialog.edit(boxRow.index, { hidden: !checked })
                    KanteCheckSkin { control: visibleBox }
                }
                KanteButton {
                    text: qsTr("Remove")
                    onClicked: dialog.removeBox(boxRow.index)
                }
            }
        }

        // Crop of the selected box: how much of its input is hidden at each
        // edge; the rest fills the box.
        RowLayout {
            Layout.fillWidth: true
            visible: dialog.selected >= 0 && dialog.selected < dialog.boxes.length

            KanteSectionLabel { text: qsTr("Crop") }
            Repeater {
                model: [
                    { edge: "left", label: qsTr("Left") },
                    { edge: "top", label: qsTr("Top") },
                    { edge: "right", label: qsTr("Right") },
                    { edge: "bottom", label: qsTr("Bottom") }
                ]

                RowLayout {
                    id: cropField
                    required property var modelData
                    QQC2.Label { text: cropField.modelData.label }
                    QQC2.SpinBox {
                        id: cropSpin
                        Layout.preferredWidth: Kirigami.Units.gridUnit * 4.5
                        from: 0
                        to: 90
                        stepSize: 5
                        editable: true
                        value: dialog.selected >= 0 && dialog.selected < dialog.boxes.length
                            ? Math.round(dialog.boxes[dialog.selected].crop[cropField.modelData.edge] * 100) : 0
                        textFromValue: v => v + " %"
                        valueFromText: text => parseInt(text) || 0
                        Accessible.name: qsTr("Crop %1, percent").arg(cropField.modelData.label)
                        onValueModified: {
                            var crop = Object.assign({}, dialog.boxes[dialog.selected].crop)
                            crop[cropField.modelData.edge] = value / 100
                            dialog.edit(dialog.selected, { crop: crop })
                        }
                        KanteFieldSkin { control: cropSpin }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            QQC2.Label {
                Layout.fillWidth: true
                color: KanteStyle.negativeTextColor
                wrapMode: Text.WordWrap
                text: dialog.boxes.some(b => b.input === null) ? qsTr("Every box needs an input.")
                    : dialog.boxes.some(b => !dialog.cropValid(b.crop)) ? qsTr("A crop must leave some of the input visible.") : ""
            }
            KanteButton {
                text: qsTr("+ Box")
                enabled: dialog.boxes.length < dialog.maxBoxes
                onClicked: dialog.addBox()
            }
        }
    }
}
