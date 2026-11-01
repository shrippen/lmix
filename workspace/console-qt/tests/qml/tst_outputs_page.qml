import QtQuick
import QtTest
import Kante
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
        { id: "o2", number: 2, name: "Recording", spec: { kind: "record", folder: "rec" }, status: { state: "stopped" } },
        // STR.5: started 3 s ago, 10 s late.
        { id: "o3", number: 3, name: "Late", spec: { kind: "srt", uri: "srt://:9001", delay_s: 10 },
            status: { state: "running", since_ms: Date.now() - 3000 } }
    ]

    QtObject {
        id: fakeShell
        property var show: ({ outputs: test.outputs, buses: [] })
        property var backend: QtObject {
            property bool connected: true
            property string links: ""
            property string devices: ""
            signal answered(string action, string refusal)
            function runningTime(since) { return "" }
            // What the add dialog on the page reads.
            function choices(list) {
                switch (list) {
                case "card_limits": return JSON.stringify({ sound_delay_ms: 0 })
                case "record_limits": return JSON.stringify({ split_minutes_max: 1440, split_mb_min: 10, split_mb_max: 1048576, mark_text_max: 200 })
                case "stream_limits": return JSON.stringify({ delay_s_max: 60 })
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
        fakeShell.backend.links = ""
        outputs[1].status = { state: "stopped" }
        fakeShell.show = { outputs: outputs.slice(), buses: [] }
    }

    // REC.8: Mark and its note show while a recording runs; the note goes
    // with the mark once, and stays when lmix-core refuses the mark.
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
        compare(note.maximumLength, 200)
        // Cut on whole characters, not inside an emoji.
        note.text = "Q".repeat(199) + "\u{1F600}"
        compare(note.text, "Q".repeat(199))
        note.text = " Q&A "
        button.clicked()
        compare(sent, [{ action: "mark_recording", text: "Q&A" }])
        // Refused: the note stays; taken: it goes.
        fakeShell.backend.answered(JSON.stringify(sent[0]), "no recording runs")
        compare(note.text, " Q&A ")
        fakeShell.backend.answered(JSON.stringify(sent[0]), "")
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

    // STR.5: a delayed stream counts down to on air, in its row and in
    // the detail; a live one shows no delay.
    function test_a_delayed_stream_counts_down() {
        // In the window, not in the (hidden) test case: visibility counts.
        const page = createTemporaryObject(pageComponent, test.parent)
        const delay = findChild(page, "delayDetail")
        verify(!delay.visible, "live")
        page.selectedId = "o3"
        verify(delay.visible)
        verify(delay.value.startsWith("10 s, on air in "), delay.value)
        const left = Number(delay.value.match(/in (\d+) s/)[1])
        verify(left >= 6 && left <= 7, left)
        // The row's time column, in the running time's place.
        tryVerify(() => {
            const labels = []
            collect(page, "elapsed", labels)
            return labels.some(l => /^\u22120:0[67]$/.test(l.text))
        })
    }
    // STR.7: an encoded output's load in the detail, marked while it
    // falls behind (buffer half full or the encoder near real time).
    function test_the_load_shows_and_warns() {
        const page = createTemporaryObject(pageComponent, test.parent)
        const load = findChild(page, "loadDetail")
        page.selectedId = "o3"
        verify(!load.visible, "nothing measured")
        const links = (l) => JSON.stringify({ o3: { output: "o3", kbps: 5000, load: l } })
        fakeShell.backend.links = links({ buffer_ms: 33, buffer_max_ms: 500, frames_dropped: 2,
            encode_ms: 8.5, encoder_percent: 24 })
        verify(load.visible)
        compare(load.value, Texts.loadDetail({ buffer_ms: 33, buffer_max_ms: 500, frames_dropped: 2,
            encode_ms: 8.5, encoder_percent: 24 }))
        verify(load.value.indexOf("frames dropped 2") >= 0, load.value)
        compare(load.valueColor, KanteStyle.textColor)
        fakeShell.backend.links = links({ buffer_ms: 300, buffer_max_ms: 500, frames_dropped: 2,
            encode_ms: null, encoder_percent: null })
        compare(load.valueColor, KanteStyle.warningColor)
        verify(load.value.endsWith("encoder –"), load.value)
        fakeShell.backend.links = links({ buffer_ms: 0, buffer_max_ms: 500, frames_dropped: 0,
            encode_ms: 30, encoder_percent: 95 })
        compare(load.valueColor, KanteStyle.warningColor)
    }

    function collect(item, name, into) {
        for (const child of item.children || []) {
            if (child.objectName === name)
                into.push(child)
            collect(child, name, into)
        }
        if (item.contentItem && item.contentItem !== item)
            collect(item.contentItem, name, into)
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
