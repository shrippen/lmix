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

    property var sent: []
    property var outputs: [
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
            function choices(list) {
                switch (list) {
                case "card_limits": return JSON.stringify({ sound_delay_ms: 0 })
                case "record_limits": return JSON.stringify({ split_minutes_max: 1440, split_mb_min: 10, split_mb_max: 1048576, mark_text_max: 200 })
                }
                return "[]"
            }
            function refreshLists() {}
        }
        property bool confirmStop: true
        property var inputs: []
        function act(action) { test.sent.push(action) }
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

    function init() {
        sent = []
        outputs[1].status = { state: "stopped" }
        fakeShell.show = { outputs: outputs.slice(), buses: [] }
    }

    // REC.8: Mark and its note show while a recording runs; the note goes
    // with the mark once.
    function test_mark_writes_the_note_once() {
        const page = createTemporaryObject(pageComponent, test)
        // The window's header shows the tools (AppShell).
        page.toolsItem.parent = test.Window.contentItem
        const button = findChild(page.toolsItem, "markButton")
        const note = findChild(page.toolsItem, "markText")
        verify(!button.visible)
        outputs[1].status = { state: "running", since_ms: 0 }
        fakeShell.show = { outputs: outputs.slice(), buses: [] }
        tryVerify(() => button.visible)
        note.text = " Q&A "
        button.clicked()
        compare(sent, [{ action: "mark_recording", text: "Q&A" }])
        compare(note.text, "")
        button.clicked()
        compare(sent[1], { action: "mark_recording" })
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
