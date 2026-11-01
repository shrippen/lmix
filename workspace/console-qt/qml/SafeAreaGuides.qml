import QtQuick
import QtQuick.Shapes
import Kante

/**
 * Guides over a monitor picture (OUT.12): safe-area frames, a crop for
 * another aspect and the thirds. Only in the console's picture, never in
 * an output: the engine does not know them.
 *
 * The picture is fitted into the item as the video is (centred, `aspect`
 * width over height). `frame`: "" none, "ebu" action safe 93 % and title
 * safe 90 % (EBU R 95, SMPTE ST 2046-1; the title frame dashed), "90" or
 * "80" one frame at that share. `crop`: "", "4:3", "1:1" or "9:16"; outside
 * it the picture is dimmed. `thirds`: lines at a third, inside the crop
 * when there is one.
 */
Item {
    id: guides

    /** Width over height of the picture. */
    property real aspect: 16 / 9
    property string frame: ""
    property string crop: ""
    property bool thirds: false

    readonly property var shapes: layout(width, height, aspect, frame, crop, thirds)
    readonly property bool shown: shapes.frames.length > 0 || shapes.crop !== null || shapes.lines.length > 0

    /** Width over height of a crop name, 0 for none. */
    function cropAspect(name) {
        const parts = String(name).split(":")
        if (parts.length !== 2) {
            return 0
        }
        const w = Number(parts[0])
        const h = Number(parts[1])
        return w > 0 && h > 0 ? w / h : 0
    }

    /** A rectangle `share` of r, centred in it. */
    function inset(r, share, dashed) {
        const dx = r.width * (1 - share) / 2
        const dy = r.height * (1 - share) / 2
        return { x: r.x + dx, y: r.y + dy, width: r.width - 2 * dx, height: r.height - 2 * dy, dashed: dashed }
    }

    /**
     * What to draw in an item of w × h: `picture` (the fitted picture),
     * `frames` (rectangles, `dashed` or not), `crop` (rectangle or null)
     * and `lines` ({x1, y1, x2, y2}). Pure, for the tests.
     */
    function layout(w, h, aspect, frame, crop, thirds) {
        const result = { picture: null, frames: [], crop: null, lines: [] }
        if (!(w > 0 && h > 0 && aspect > 0)) {
            return result
        }
        const pw = Math.min(w, h * aspect)
        const ph = pw / aspect
        const picture = { x: (w - pw) / 2, y: (h - ph) / 2, width: pw, height: ph }
        result.picture = picture
        if (frame === "ebu") {
            result.frames.push(inset(picture, 0.93, false))
            result.frames.push(inset(picture, 0.90, true))
        } else if (frame === "90") {
            result.frames.push(inset(picture, 0.90, false))
        } else if (frame === "80") {
            result.frames.push(inset(picture, 0.80, false))
        }
        const ca = cropAspect(crop)
        // A crop as wide as the picture (9:16 on a portrait format) cuts nothing.
        if (ca > 0 && Math.abs(ca - aspect) / aspect > 0.01) {
            const cw = ca < aspect ? ph * ca : pw
            const ch = ca < aspect ? ph : pw / ca
            result.crop = { x: picture.x + (pw - cw) / 2, y: picture.y + (ph - ch) / 2, width: cw, height: ch }
        }
        if (thirds) {
            const r = result.crop !== null ? result.crop : picture
            for (const t of [1 / 3, 2 / 3]) {
                result.lines.push({ x1: r.x + r.width * t, y1: r.y, x2: r.x + r.width * t, y2: r.y + r.height })
                result.lines.push({ x1: r.x, y1: r.y + r.height * t, x2: r.x + r.width, y2: r.y + r.height * t })
            }
        }
        return result
    }

    /** On the pixel grid, for a sharp 1 px line. */
    function crisp(v) {
        return Math.round(v) + 0.5
    }

    visible: shown
    Accessible.role: Accessible.Graphic
    Accessible.name: qsTr("Guides")

    // Dimmed outside the crop: the parts of the picture a 9:16 stream (or
    // another crop) loses. Only one pair is ever not empty.
    Repeater {
        model: {
            const c = guides.shapes.crop
            const p = guides.shapes.picture
            if (c === null || p === null) {
                return []
            }
            return [
                { x: p.x, y: p.y, width: c.x - p.x, height: p.height },
                { x: c.x + c.width, y: p.y, width: p.x + p.width - c.x - c.width, height: p.height },
                { x: c.x, y: p.y, width: c.width, height: c.y - p.y },
                { x: c.x, y: c.y + c.height, width: c.width, height: p.y + p.height - c.y - c.height }
            ].filter(r => r.width > 0.5 && r.height > 0.5)
        }
        delegate: Rectangle {
            required property var modelData
            objectName: "guideShade"
            x: modelData.x
            y: modelData.y
            width: modelData.width
            height: modelData.height
            color: KanteStyle.stageColor
            opacity: 0.55
        }
    }

    // Frames (the crop, the safe areas) and the thirds, in the stage's ink.
    Repeater {
        model: {
            const paths = []
            const rect = (r, dashed) => ({
                dashed: dashed,
                points: [
                    Qt.point(guides.crisp(r.x), guides.crisp(r.y)),
                    Qt.point(guides.crisp(r.x + r.width), guides.crisp(r.y)),
                    Qt.point(guides.crisp(r.x + r.width), guides.crisp(r.y + r.height)),
                    Qt.point(guides.crisp(r.x), guides.crisp(r.y + r.height)),
                    Qt.point(guides.crisp(r.x), guides.crisp(r.y))
                ]
            })
            if (guides.shapes.crop !== null) {
                paths.push(rect(guides.shapes.crop, false))
            }
            for (const f of guides.shapes.frames) {
                paths.push(rect(f, f.dashed))
            }
            for (const l of guides.shapes.lines) {
                paths.push({ dashed: true, points: [Qt.point(guides.crisp(l.x1), guides.crisp(l.y1)), Qt.point(guides.crisp(l.x2), guides.crisp(l.y2))] })
            }
            return paths
        }
        delegate: Shape {
            required property var modelData
            objectName: "guidePath"
            anchors.fill: parent
            ShapePath {
                strokeColor: KanteStyle.stageTextColor
                strokeWidth: 1
                fillColor: "transparent"
                capStyle: ShapePath.FlatCap
                strokeStyle: modelData.dashed ? ShapePath.DashLine : ShapePath.SolidLine
                dashPattern: [4, 3]
                PathPolyline {
                    path: modelData.points
                }
            }
        }
    }
}
