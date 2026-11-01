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
