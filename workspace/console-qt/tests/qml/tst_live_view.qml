import QtQuick
import QtTest
import "../../qml"

/**
 * A live picture runs only while it is shown: on a page that is hidden
 * it stops after its hold time (a page change in between keeps it), and
 * it asks again once shown. A hidden picture made the window draw each of
 * its frames (31.10.2026).
 */
TestCase {
    id: test
    name: "LiveView"
    when: windowShown
    // The pictures run only while shown.
    visible: true
    width: 400
    height: 300

    property var calls: []

    QtObject {
        id: fakeCore
        function showLive(item, source) { test.calls.push("show " + source) }
        function releaseLive(item) { test.calls.push("release") }
    }

    Component {
        id: page
        Item {
            property alias view: view
            width: 320
            height: 180
            LiveView {
                id: view
                anchors.fill: parent
                core: fakeCore
                source: "program"
                holdMs: 100
            }
        }
    }

    /** The GStreamer item inside, drawn once (the stub's flag). */
    function draw(view) {
        for (var i = 0; i < view.children.length; i++) {
            if (view.children[i].itemInitialized !== undefined) {
                view.children[i].itemInitialized = true
                return
            }
        }
        fail("no video item")
    }

    function init() {
        calls = []
    }

    function test_runs_while_shown() {
        var p = createTemporaryObject(page, test)
        draw(p.view)
        compare(calls, ["show program"])
        p.visible = false
        compare(calls.length, 1, "kept over a page change")
        wait(200)
        compare(calls, ["show program", "release"], "stopped once hidden longer")
        p.visible = true
        compare(calls, ["show program", "release", "show program"])
    }

    function test_a_page_change_keeps_the_picture() {
        var p = createTemporaryObject(page, test)
        draw(p.view)
        p.visible = false
        wait(20)
        p.visible = true
        wait(200)
        compare(calls, ["show program"], "neither stopped nor asked again")
    }

    function test_never_shown_asks_nothing() {
        var p = createTemporaryObject(page, test, { visible: false })
        draw(p.view)
        wait(200)
        compare(calls, [])
        p.visible = true
        compare(calls, ["show program"])
    }

    function test_a_new_source_while_hidden_stops_the_old() {
        var p = createTemporaryObject(page, test)
        draw(p.view)
        p.visible = false
        p.view.source = "preview"
        compare(calls, ["show program", "release"])
        p.visible = true
        compare(calls, ["show program", "release", "show preview"])
    }
}
