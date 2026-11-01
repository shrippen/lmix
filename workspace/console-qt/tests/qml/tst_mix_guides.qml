import QtQuick
import QtQuick.Controls as QQC2
import QtCore
import QtTest
import Kante
import "../../qml"

/**
 * The guides on the Mix page (OUT.12): the saved view settings come back
 * after a start and put the guides on the monitor chosen; the View
 * popover with its guides column stays inside a narrow window (a page in
 * its own window may be 540 px wide). With the real MixPage in an AppShell
 * and the stand-in Core (tests/qml/stubs).
 */
TestCase {
    id: test
    name: "MixGuides"
    when: windowShown
    width: 400
    height: 300

    // The settings as the app keeps them: under an organization (the test
    // runner has none, then nothing is kept), in the test's own folder
    // (scripts/test-qml.sh); set back afterwards for the other tests.
    Component {
        id: savedComponent
        Settings {
            category: "view"
        }
    }
    property var saved: null
    property string organization: ""

    function initTestCase() {
        organization = Qt.application.organization
        Qt.application.organization = "lmix-test"
        saved = savedComponent.createObject(test)
    }

    function cleanupTestCase() {
        saved.destroy()
        Qt.application.organization = organization
    }

    Component {
        id: shellComponent
        AppShell {
            pageEntries: [
                AppShell.PageEntry {
                    pageId: "mix"
                    page: MixPage {}
                }
            ]
        }
    }

    property var shell: null

    function start(width) {
        shell = createTemporaryObject(shellComponent, null, { width: width, height: 800 })
        shell.visible = true
        shell.showPage("mix")
        tryCompare(shell, "currentPage", "mix")
        return shell
    }

    function cleanup() {
        if (shell) {
            shell.quitting = true
            shell.close()
            shell = null
        }
        saved.setValue("guideFrame", "")
        saved.setValue("guideCrop", "")
        saved.setValue("guideThirds", false)
        saved.setValue("guidesOn", "preview")
        saved.sync()
    }

    function test_saved_guides_come_back_on_the_chosen_monitor() {
        saved.setValue("guideFrame", "ebu")
        saved.setValue("guideCrop", "9:16")
        saved.setValue("guidesOn", "program")
        saved.sync()
        start(1400)
        const program = findChild(shell, "programMonitorGuides")
        const preview = findChild(shell, "previewMonitorGuides")
        verify(program && preview)
        compare(program.frame, "ebu")
        compare(program.crop, "9:16")
        tryVerify(() => program.visible)
        verify(!preview.visible)
        compare(program.shapes.frames.length, 2)
        verify(program.shapes.crop !== null)
    }

    function test_the_guides_on_both_monitors() {
        saved.setValue("guidesOn", "both")
        saved.setValue("guideThirds", true)
        saved.sync()
        start(1400)
        tryVerify(() => findChild(shell, "programMonitorGuides").visible)
        verify(findChild(shell, "previewMonitorGuides").visible)
    }

    function test_no_guides_without_a_choice() {
        start(1400)
        const preview = findChild(shell, "previewMonitorGuides")
        verify(preview)
        compare(preview.frame, "")
        verify(!preview.visible)
        verify(!findChild(shell, "programMonitorGuides").visible)
    }

    function popoverIn(width) {
        start(width)
        const popover = findChild(shell, "viewPopover")
        popover.open()
        tryCompare(popover, "opened", true)
        const grid = findChild(shell, "viewGrid")
        waitForRendering(grid)
        return grid
    }

    function verifyInside(grid) {
        const place = grid.mapToItem(null, 0, 0)
        verify(place.x >= 0, "left edge " + place.x)
        verify(place.x + grid.width <= shell.width, "right edge " + (place.x + grid.width) + " in " + shell.width)
        const picker = findChild(shell, "guidesPicker")
        const pickerPlace = picker.mapToItem(null, 0, 0)
        verify(pickerPlace.x + picker.width <= shell.width, "guides end at " + (pickerPlace.x + picker.width))
    }

    function test_the_view_popover_has_three_columns_where_they_fit() {
        const grid = popoverIn(1400)
        compare(grid.columns, 3)
        verifyInside(grid)
    }

    function test_the_view_popover_stays_in_a_narrow_window() {
        const grid = popoverIn(540)
        verifyInside(grid)
        compare(grid.columns, 2)
        // The guides below the two columns.
        const picker = findChild(shell, "guidesPicker")
        verify(picker.y > 0)
    }
}
