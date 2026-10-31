import QtQuick
import QtTest
import "../../qml"

/**
 * The Outputs page shows the selected output in detail: `selectedId`
 * picks it (a click on its row); Settings… there opens its settings.
 */
TestCase {
    id: test
    name: "OutputsPage"
    when: windowShown
    width: 1200
    height: 700

    readonly property var outputs: [
        { id: "o1", number: 1, name: "Stream", spec: { kind: "srt", uri: "srt://:9000" }, status: { state: "stopped" } },
        { id: "o2", number: 2, name: "Recording", spec: { kind: "record", folder: "/tmp/rec" }, status: { state: "stopped" } }
    ]

    QtObject {
        id: fakeShell
        property var show: ({ outputs: test.outputs, buses: [] })
        property var backend: QtObject {
            property bool connected: true
            property string links: ""
            property string devices: ""
            function runningTime(since) { return "" }
            // What the add dialog on the page reads.
            function choices(list) { return list === "card_limits" ? JSON.stringify({ sound_delay_ms: 0 }) : "[]" }
            function refreshLists() {}
        }
        property bool confirmStop: true
        property var inputs: []
        function act(action) {}
        function askRemove(kind, item) {}
        function inputById(id) { return null }
    }

    Component {
        id: pageComponent
        OutputsPage {
            shell: fakeShell
            width: test.width
            height: test.height
        }
    }

    function test_selected_id_picks_the_detail() {
        const page = createTemporaryObject(pageComponent, test)
        compare(page.selected.id, "o1", "the first by default")
        page.selectedId = "o2"
        compare(page.selected.id, "o2")
        compare(page.selected.name, "Recording")
    }

    function test_settings_opens_the_selected_ones() {
        const page = createTemporaryObject(pageComponent, test)
        page.selectedId = "o2"
        const button = findChild(page, "settings")
        verify(button.enabled)
        button.clicked()
        const dialog = findChild(page, "outputDialog")
        compare(dialog.editId, "o2")
        tryCompare(dialog, "opened", true)
        compare(dialog.title, "Output settings")
    }
}
