import QtQuick
import QtQuick.Shapes
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Guides over a monitor picture (OUT.12): safe-area frames, a crop for
 * another aspect and the thirds. Only in the console's picture, never in
 * an output: the engine does not know them.
 *
 * The picture is fitted into the item as the video is (centred, `aspect`
 * width over height). `crop`: "", "4:3", "1:1" or "9:16"; outside it the
 * picture is dimmed. `frame`: "" none, "ebu" action safe 93 % and title
 * safe 90 % (EBU R 95, SMPTE ST 2046-1; the title frame dashed), "90" or
 * "80" one frame at that share. Frames and thirds lie in the crop when
 * there is one: that is the picture a portrait stream sends, its lower
 * third must stay inside its own title-safe frame. A label names each
 * frame's share at its corner, where the picture is large enough.
 *
 * Every line has a dark twin one pixel lower right, so it shows on a
 * bright picture too. The items are fixed; a resize moves their points.
 */
Item {
    id: guides

    /** Width over height of the picture. */
    property real aspect: 16 / 9
    property string frame: ""
    property string crop: ""
    property bool thirds: false

    /** Device pixels per item pixel: the window's (a QML property since
     *  Qt 6.11), else the screen's; never 0, which would lose every line. */
    readonly property real dpr: pixelRatio(Window.window ? Window.window.devicePixelRatio : undefined, Screen.devicePixelRatio)
    // Read again on a window resize too: it moves a centred monitor, and
    // the lines go on the window's pixel grid (crisp).
    readonly property var shapes: {
        void Window.width
        return layout(width, height, aspect, frame, crop, thirds)
    }
    readonly property bool shown: shapes.frames.length > 0 || shapes.crop !== null || shapes.lines.length > 0
    /** Frame labels only where there is room for them. */
    readonly property bool labelled: shapes.area !== null && shapes.area.width >= Kirigami.Units.gridUnit * 8

    /** The first of `fromWindow`, `fromScreen` above 0, else 1. Pure, for the tests. */
    function pixelRatio(fromWindow, fromScreen) {
        if (fromWindow > 0) {
            return fromWindow
        }
        return fromScreen > 0 ? fromScreen : 1
    }

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
    function inset(r, share, dashed, label) {
        const dx = r.width * (1 - share) / 2
        const dy = r.height * (1 - share) / 2
        return { x: r.x + dx, y: r.y + dy, width: r.width - 2 * dx, height: r.height - 2 * dy, dashed: dashed, label: label }
    }

    /**
     * What to draw in an item of w × h: `picture` (the fitted picture),
     * `crop` (rectangle or null), `area` (the crop, else the picture),
     * `frames` (rectangles in the area, `dashed` or not, with a `label`)
     * and `lines` (the thirds in the area, {x1, y1, x2, y2}). Pure, for
     * the tests.
     */
    function layout(w, h, aspect, frame, crop, thirds) {
        const result = { picture: null, crop: null, area: null, frames: [], lines: [] }
        if (!(w > 0 && h > 0 && aspect > 0)) {
            return result
        }
        const pw = Math.min(w, h * aspect)
        const ph = pw / aspect
        const picture = { x: (w - pw) / 2, y: (h - ph) / 2, width: pw, height: ph }
        result.picture = picture
        const ca = cropAspect(crop)
        // A crop as wide as the picture (9:16 on a portrait format) cuts nothing.
        if (ca > 0 && Math.abs(ca - aspect) / aspect > 0.01) {
            const cw = ca < aspect ? ph * ca : pw
            const ch = ca < aspect ? ph : pw / ca
            result.crop = { x: picture.x + (pw - cw) / 2, y: picture.y + (ph - ch) / 2, width: cw, height: ch }
        }
        const area = result.crop !== null ? result.crop : picture
        result.area = area
        if (frame === "ebu") {
            result.frames.push(inset(area, 0.93, false, "93"))
            result.frames.push(inset(area, 0.90, true, "90"))
        } else if (frame === "90") {
            result.frames.push(inset(area, 0.90, false, "90"))
        } else if (frame === "80") {
            result.frames.push(inset(area, 0.80, false, "80"))
        }
        if (thirds) {
            for (const t of [1 / 3, 2 / 3]) {
                result.lines.push({ x1: area.x + area.width * t, y1: area.y, x2: area.x + area.width * t, y2: area.y + area.height })
                result.lines.push({ x1: area.x, y1: area.y + area.height * t, x2: area.x + area.width, y2: area.y + area.height * t })
            }
        }
        return result
    }

    /** x (or y, `vertical`) in the middle of a device pixel of the window. */
    function crisp(v, vertical) {
        const origin = mapToItem(null, 0, 0)
        const o = vertical ? origin.y : origin.x
        // A hair over, as 400 × 0.1 comes out at 39.999…
        return (Math.floor((v + o) * dpr + 1e-6) + 0.5) / dpr - o
    }

    /** The points around rectangle r, on the pixel grid; [] for none. */
    function outline(r) {
        if (!r) {
            return []
        }
        const x1 = crisp(r.x, false)
        const y1 = crisp(r.y, true)
        const x2 = crisp(r.x + r.width - 1 / dpr, false)
        const y2 = crisp(r.y + r.height - 1 / dpr, true)
        return [Qt.point(x1, y1), Qt.point(x2, y1), Qt.point(x2, y2), Qt.point(x1, y2), Qt.point(x1, y1)]
    }

    readonly property var cropPoints: outline(shapes.crop)
    readonly property var solidPoints: outline(shapes.frames.length > 0 ? shapes.frames[0] : null)
    readonly property var dashedPoints: outline(shapes.frames.length > 1 ? shapes.frames[1] : null)
    readonly property var thirdLines: shapes.lines.map(l => [
        Qt.point(crisp(l.x1, false), crisp(l.y1, true)),
        Qt.point(crisp(l.x2, false), crisp(l.y2, true))
    ])
    readonly property var pic: shapes.picture
    readonly property var cut: shapes.crop

    visible: shown
    Accessible.role: Accessible.Graphic
    Accessible.name: qsTr("Guides")

    // Dimmed outside the crop: what a 9:16 stream (or another crop) loses.
    component Shade: Rectangle {
        objectName: "guideShade"
        color: KanteStyle.scrimColor
        visible: guides.cut !== null && width > 0.5 && height > 0.5
    }
    Shade {
        x: guides.pic ? guides.pic.x : 0
        y: guides.pic ? guides.pic.y : 0
        width: guides.cut ? guides.cut.x - guides.pic.x : 0
        height: guides.pic ? guides.pic.height : 0
    }
    Shade {
        x: guides.cut ? guides.cut.x + guides.cut.width : 0
        y: guides.pic ? guides.pic.y : 0
        width: guides.cut ? guides.pic.x + guides.pic.width - guides.cut.x - guides.cut.width : 0
        height: guides.pic ? guides.pic.height : 0
    }
    Shade {
        x: guides.cut ? guides.cut.x : 0
        y: guides.pic ? guides.pic.y : 0
        width: guides.cut ? guides.cut.width : 0
        height: guides.cut ? guides.cut.y - guides.pic.y : 0
    }
    Shade {
        x: guides.cut ? guides.cut.x : 0
        y: guides.cut ? guides.cut.y + guides.cut.height : 0
        width: guides.cut ? guides.cut.width : 0
        height: guides.cut ? guides.pic.y + guides.pic.height - guides.cut.y - guides.cut.height : 0
    }

    // All lines in one shape: crop and first frame solid, the title frame
    // and the thirds dashed.
    component Lines: Shape {
        id: lines
        property color ink
        width: guides.width
        height: guides.height
        ShapePath {
            strokeColor: lines.ink
            strokeWidth: 1 / guides.dpr
            fillColor: "transparent"
            capStyle: ShapePath.FlatCap
            PathPolyline { path: guides.cropPoints }
        }
        ShapePath {
            strokeColor: lines.ink
            strokeWidth: 1 / guides.dpr
            fillColor: "transparent"
            capStyle: ShapePath.FlatCap
            PathPolyline { path: guides.solidPoints }
        }
        ShapePath {
            strokeColor: lines.ink
            strokeWidth: 1 / guides.dpr
            fillColor: "transparent"
            capStyle: ShapePath.FlatCap
            strokeStyle: ShapePath.DashLine
            dashPattern: [4, 3]
            PathPolyline { path: guides.dashedPoints }
            PathMultiline { paths: guides.thirdLines }
        }
    }
    Lines {
        objectName: "guideShadow"
        x: 1 / guides.dpr
        y: 1 / guides.dpr
        ink: KanteStyle.stageColor
    }
    Lines {
        objectName: "guideLines"
        ink: KanteStyle.stageTextColor
    }

    // The legend: each frame's share, stacked at the top left inside the
    // first (outer) frame.
    Repeater {
        model: guides.labelled ? guides.shapes.frames.length : 0
        delegate: QQC2.Label {
            required property int index
            objectName: "guideLabel"
            readonly property var outer: guides.shapes.frames.length > 0 ? guides.shapes.frames[0] : null
            readonly property var own: guides.shapes.frames[index] ?? null
            x: own ? own.x + KanteStyle.unit(3) : 0
            y: outer ? outer.y + KanteStyle.unit(2) + index * height : 0
            text: own ? qsTr("%1 %", "a safe-area frame: its share of the picture").arg(own.label) : ""
            font: KanteStyle.smallFont
            color: KanteStyle.stageTextColor
            style: Text.Outline
            styleColor: KanteStyle.stageColor
        }
    }
}
