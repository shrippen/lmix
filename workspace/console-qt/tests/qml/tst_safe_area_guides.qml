import QtQuick
import QtTest
import "../../qml"

/**
 * Guides over a monitor (SafeAreaGuides, GuidesPicker; OUT.12): the frames
 * sit at their share of the fitted picture, a crop keeps its aspect and
 * dims the rest, the thirds follow the crop; nothing is drawn without a
 * choice. The picker reports a choice, the setting decides what shows.
 */
TestCase {
    id: test
    name: "SafeAreaGuides"
    when: windowShown
    width: 800
    height: 600

    Component {
        id: guidesComponent
        SafeAreaGuides {
            width: 400
            height: 225
        }
    }

    Component {
        id: pickerComponent
        GuidesPicker {}
    }

    property var changes: []

    function init() {
        changes = []
    }

    function fuzzyRect(actual, x, y, w, h) {
        fuzzyCompare(actual.x, x, 0.001)
        fuzzyCompare(actual.y, y, 0.001)
        fuzzyCompare(actual.width, w, 0.001)
        fuzzyCompare(actual.height, h, 0.001)
    }

    function named(item, name) {
        const found = []
        for (let i = 0; i < item.children.length; i++) {
            if (item.children[i].objectName === name) {
                found.push(item.children[i])
            }
        }
        return found
    }

    function shades(guides) {
        return named(guides, "guideShade").filter(r => r.visible)
    }

    function test_nothing_without_a_choice() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem)
        verify(!guides.shown)
        verify(!guides.visible)
        compare(guides.solidPoints.length, 0)
        compare(shades(guides).length, 0)
    }

    function test_ebu_frames_at_93_and_90_percent() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { frame: "ebu" })
        const s = guides.shapes
        fuzzyRect(s.picture, 0, 0, 400, 225)
        compare(s.frames.length, 2)
        fuzzyRect(s.frames[0], 14, 7.875, 372, 209.25)
        verify(!s.frames[0].dashed)
        fuzzyRect(s.frames[1], 20, 11.25, 360, 202.5)
        verify(s.frames[1].dashed)
        verify(guides.visible)
        compare(guides.solidPoints.length, 5)
        compare(guides.dashedPoints.length, 5)
        compare(guides.cropPoints.length, 0)
        // The legend names both frames.
        tryCompare(named(guides, "guideLabel"), "length", 2)
        compare(named(guides, "guideLabel").map(l => l.text), ["93 %", "90 %"])
    }

    function test_frames_follow_the_fitted_picture() {
        // A 16:9 picture in a taller item sits in its middle.
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { height: 325, frame: "80" })
        fuzzyRect(guides.shapes.picture, 0, 50, 400, 225)
        fuzzyRect(guides.shapes.frames[0], 40, 72.5, 320, 180)
        guides.frame = "90"
        fuzzyRect(guides.shapes.frames[0], 20, 61.25, 360, 202.5)
    }

    function test_a_portrait_crop_dims_the_sides() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { crop: "9:16" })
        const c = guides.shapes.crop
        fuzzyCompare(c.height, 225, 0.001)
        fuzzyCompare(c.width, 225 * 9 / 16, 0.001)
        fuzzyCompare(c.x, (400 - 225 * 9 / 16) / 2, 0.001)
        const dim = shades(guides)
        compare(dim.length, 2)
        fuzzyCompare(dim[0].width, c.x, 0.001)
        fuzzyCompare(dim[1].x, c.x + c.width, 0.001)
    }

    function test_a_wide_crop_on_a_portrait_picture_dims_top_and_bottom() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { aspect: 9 / 16, width: 225, height: 400, crop: "1:1" })
        fuzzyRect(guides.shapes.crop, 0, 87.5, 225, 225)
        const dim = shades(guides)
        compare(dim.length, 2)
        fuzzyCompare(dim[0].height, 87.5, 0.001)
    }

    function test_a_crop_of_the_pictures_own_aspect_cuts_nothing() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { aspect: 9 / 16, width: 225, height: 400, crop: "9:16" })
        compare(guides.shapes.crop, null)
        verify(!guides.shown)
    }

    function test_the_thirds_follow_the_crop() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { thirds: true })
        const lines = guides.shapes.lines
        compare(lines.length, 4)
        fuzzyCompare(lines[0].x1, 400 / 3, 0.001)
        fuzzyCompare(lines[1].y1, 75, 0.001)
        guides.crop = "1:1"
        const inCrop = guides.shapes.lines
        fuzzyCompare(inCrop[0].x1, 87.5 + 75, 0.001)
        fuzzyCompare(inCrop[1].x1, 87.5, 0.001)
        fuzzyCompare(inCrop[1].x2, 87.5 + 225, 0.001)
        compare(guides.thirdLines.length, 4)
        compare(guides.cropPoints.length, 5)
    }

    // A lower third of a portrait stream must stay in the crop's own
    // title-safe frame, not the whole picture's.
    function test_frames_lie_in_the_crop() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { crop: "9:16", frame: "ebu" })
        const c = guides.shapes.crop
        fuzzyRect(guides.shapes.frames[1], c.x + c.width * 0.05, c.height * 0.05, c.width * 0.9, c.height * 0.9)
    }

    // A resize moves the points; the items stay (no rebuild while a
    // monitor's handle is dragged).
    function test_a_resize_keeps_the_items() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { crop: "9:16", frame: "80" })
        const lines = named(guides, "guideLines")[0]
        const before = named(guides, "guideShade")
        const x = guides.solidPoints[0].x
        guides.width = 600
        guides.height = 337.5
        verify(named(guides, "guideLines")[0] === lines)
        const after = named(guides, "guideShade")
        compare(after.length, before.length)
        for (let i = 0; i < after.length; i++) {
            verify(after[i] === before[i])
        }
        verify(guides.solidPoints[0].x > x)
    }

    // Before Qt 6.11 a window has no devicePixelRatio in QML: the
    // screen's counts, and nothing gives 0 (lines would vanish).
    function test_the_pixel_ratio_never_fails() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { frame: "80" })
        compare(guides.pixelRatio(undefined, 1.5), 1.5)
        compare(guides.pixelRatio(2, 1), 2)
        compare(guides.pixelRatio(0, 0), 1)
        compare(guides.pixelRatio(undefined, undefined), 1)
        verify(guides.dpr > 0)
        verify(isFinite(guides.solidPoints[0].x))
    }

    function test_no_labels_on_a_small_picture() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { width: 100, height: 56.25, frame: "ebu" })
        compare(named(guides, "guideLabel").length, 0)
    }

    function test_the_guides_are_drawn_in_the_picture() {
        const guides = createTemporaryObject(guidesComponent, test.Window.contentItem, { frame: "80" })
        waitForRendering(guides)
        const image = grabImage(guides)
        // On the frame's left edge, and a little inside it, where nothing is drawn.
        const ink = image.pixel(40, 112)
        const empty = image.pixel(60, 112)
        verify(!Qt.colorEqual(ink, empty), "the frame is drawn: " + ink + " on " + empty)
        // Its dark twin a pixel to the right, for bright pictures.
        verify(!Qt.colorEqual(image.pixel(41, 112), empty), "the shadow is drawn")
        // The picture's middle stays as it was.
        verify(Qt.colorEqual(image.pixel(200, 112), empty))
    }

    function segments(picker, name) {
        for (let i = 0; i < picker.children.length; i++) {
            if (picker.children[i].objectName === name) {
                return picker.children[i]
            }
        }
        return null
    }

    function test_the_picker_reports_the_setting_decides() {
        const picker = createTemporaryObject(pickerComponent, test.Window.contentItem, { crop: "4:3", monitor: "both" })
        picker.changed.connect((key, value) => test.changes.push([key, value]))
        const frame = segments(picker, "guideFrame")
        const crop = segments(picker, "guideCrop")
        const on = segments(picker, "guideOn")
        compare(frame.currentIndex, 0)
        compare(crop.currentIndex, 1)
        compare(on.currentIndex, 2)
        frame.select(1)
        compare(changes, [["frame", "ebu"]])
        // Not taken yet: no frame stays chosen.
        compare(frame.currentIndex, 0)
        picker.frame = "ebu"
        compare(frame.currentIndex, 1)
        crop.select(3)
        on.select(0)
        compare(changes.slice(1), [["crop", "9:16"], ["monitor", "preview"]])
        const thirds = segments(picker, "guideThirds")
        mouseClick(thirds)
        compare(changes[3], ["thirds", true])
        compare(thirds.checked, false)
    }
}
