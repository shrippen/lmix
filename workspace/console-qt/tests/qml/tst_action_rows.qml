import QtQuick
import QtTest
import "../../qml"

/**
 * Action rows (shortcuts, triggers, cues, moods): up to sixteen, as
 * lmix-core takes; eight show, then one more each time the last shown is
 * used, so a list set over the API comes back whole. The dialogs that
 * hold them stay inside the window: taller, the rows scroll.
 */
TestCase {
    id: test
    name: "ActionRows"
    when: windowShown
    width: 800
    height: 600

    Component {
        id: rowsComponent
        ActionRows {
            inputs: []
            outputs: []
        }
    }

    Component {
        id: listDialogComponent
        ActionListDialog {
            inputs: []
            outputs: []
        }
    }
    Component {
        id: shortcutDialogComponent
        ShortcutEditDialog {
            act: () => {}
            core: null
            inputs: []
            outputs: []
        }
    }

    function sixteen() {
        const actions = []
        for (let i = 0; i < 16; i++) {
            actions.push({ action: "cut" })
        }
        return actions
    }

    function test_sixteen_rows_scroll_in_the_dialogs() {
        const list = createTemporaryObject(listDialogComponent, test)
        list.actions = sixteen()
        list.open()
        tryVerify(() => list.opened)
        const shortcut = createTemporaryObject(shortcutDialogComponent, test)
        shortcut.shortcut = { id: "s", label: "Cuts", key: null, midi: null, web: true, actions: sixteen() }
        shortcut.open()
        tryVerify(() => shortcut.opened)
        for (const d of [list, shortcut]) {
            const scroll = findChild(d.contentItem, "actionScroll")
            tryVerify(() => scroll.contentHeight > scroll.height, 1000, "the rows scroll")
            verify(d.height <= test.height, "inside the window: " + d.height)
        }
        list.close()
        shortcut.close()
    }

    // Recording a key: Tab moves the focus on and is never the key.
    function test_tab_moves_on_while_a_key_is_recorded() {
        const shortcut = createTemporaryObject(shortcutDialogComponent, test)
        shortcut.shortcut = { id: "s", label: "Cut", key: null, midi: null, web: true, actions: [] }
        shortcut.open()
        tryVerify(() => shortcut.opened)
        const key = findChild(shortcut.contentItem, "shortcutKeyButton")
        verify(key !== null)
        key.clicked()
        verify(key.listening)
        verify(key.activeFocus)
        keyClick(Qt.Key_Tab)
        compare(key.sequence, "")
        verify(!key.activeFocus, "the focus moved on")
        verify(!key.listening)
        key.clicked()
        keyClick(Qt.Key_Tab, Qt.ShiftModifier)
        compare(key.sequence, "")
        verify(!key.activeFocus, "and back")
        key.clicked()
        keyClick(Qt.Key_F5)
        compare(key.sequence, "F5")
        shortcut.close()
    }

    // EXT.6: an OSC message has a row of its own; the targets offered are
    // the OSC ones, a target this machine lacks stays as it was.
    function test_osc_messages_edit_in_their_row() {
        const rows = createTemporaryObject(rowsComponent, test, {
            targets: [
                { name: "Haus", kind: "http" },
                { name: "Pult", kind: "osc_tcp" },
                { name: "Licht", kind: "osc_udp" }
            ]
        })
        compare(rows.oscTargets, ["Pult", "Licht"])
        const go = { action: "osc_send", target: "Licht", address: "/eos/go", args: "1 \"Szene 2\"" }
        const gone = { action: "osc_send", target: "Alt", address: "/x" }
        rows.load([go, gone])
        compare(rows.error, "")
        compare(rows.collect(), [go, { action: "osc_send", target: "Alt", address: "/x", args: "" }])

        // A new row picks the first OSC target; the address must be one.
        const row = findChild(rows, "actionRowRepeater").itemAt(2)
        row.kindIndex = rows.kinds.findIndex(k => k.kind === "osc")
        compare(row.osc().target, "Pult")
        verify(rows.error !== "", "no address yet")
        for (const bad of ["/eos/cue/1 fire", "/a#b", "/a,b", "/grün", "eos"]) {
            row.setOsc({ target: "Pult", address: bad, args: "" })
            verify(rows.error !== "", bad)
        }
        row.setOsc({ target: "Pult", address: "/eos/cue/1/fire", args: "" })
        compare(rows.error, "")
        compare(rows.collect()[2], { action: "osc_send", target: "Pult", address: "/eos/cue/1/fire", args: "" })
    }

    // The choice is a name: targets added or removed while the dialog is
    // open never move it to another desk.
    function test_osc_target_holds_when_the_list_changes() {
        const rows = createTemporaryObject(rowsComponent, test, {
            targets: [{ name: "Pult", kind: "osc_udp" }, { name: "Licht", kind: "osc_udp" }]
        })
        rows.load([{ action: "osc_send", target: "Licht", address: "/go", args: "" }])
        const row = findChild(rows, "actionRowRepeater").itemAt(0)
        rows.targets = [{ name: "A", kind: "osc_tcp" }, { name: "Pult", kind: "osc_udp" }, { name: "Licht", kind: "osc_udp" }]
        compare(row.osc().target, "Licht")
        rows.targets = [{ name: "Pult", kind: "osc_udp" }]
        compare(row.osc().target, "Licht", "kept, though gone")
        compare(row.oscChoices, ["Licht", "Pult"])
        // Picking one in the list holds the same way.
        const box = findChild(row, "oscTargetBox")
        box.activated(1)
        compare(row.osc().target, "Pult")
        rows.targets = [{ name: "B", kind: "osc_udp" }, { name: "Pult", kind: "osc_udp" }]
        compare(row.osc().target, "Pult")
        compare(box.currentText, "Pult")
    }

    // Without OSC targets the row cannot be saved.
    function test_osc_needs_a_target() {
        const rows = createTemporaryObject(rowsComponent, test)
        rows.load([])
        const row = findChild(rows, "actionRowRepeater").itemAt(0)
        row.kindIndex = rows.kinds.findIndex(k => k.kind === "osc")
        row.setOsc({ target: "", address: "/go", args: "" })
        verify(rows.error.indexOf("target") >= 0, rows.error)
    }

    function test_sixteen_actions_come_back_whole() {
        const rows = createTemporaryObject(rowsComponent, test)
        compare(rows.shown, 8)
        const actions = []
        for (let i = 0; i < 12; i++) {
            actions.push({ action: "cut" })
        }
        rows.load(actions)
        compare(rows.filled, 12)
        compare(rows.shown, 13, "the used ones and one more")
        compare(rows.collect().length, 12)
        for (let i = 12; i < 16; i++) {
            actions.push({ action: "cut" })
        }
        rows.load(actions)
        compare(rows.shown, 16)
        compare(rows.collect().length, 16)
    }
}
