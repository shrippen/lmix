import QtQuick
import QtTest
import "../../qml"

/**
 * Editing a layout while it changes live (a cue swaps its boxes, a
 * shortcut gives a box another input, another console saves it): Save
 * sends only what was changed in the dialog and keeps the rest as it is
 * now; a layout whose boxes were added or removed elsewhere shows a notice
 * with Reload.
 */
TestCase {
    id: test
    name: "LayoutDialog"
    when: windowShown
    width: 1200
    height: 900

    property var sent: []
    property var layouts: []

    readonly property var inputs: [
        { id: "a", number: 1, name: "Wide", source: { kind: "test" } },
        { id: "b", number: 2, name: "Host", source: { kind: "test" } },
        { id: "c", number: 3, name: "Guest", source: { kind: "test" } },
        { id: "d", number: 4, name: "Slides", source: { kind: "test" } }
    ]

    function box(input, x, crop) {
        return { input: input, rect: { x: x, y: 0.5, w: 0.25, h: 0.25 }, border_px: 0,
            crop: crop || { left: 0, top: 0, right: 0, bottom: 0 }, locked: false, hidden: false }
    }
    function interview(boxes, background) {
        return { id: "l1", number: 1, name: "Interview", background: background === undefined ? "a" : background, boxes: boxes }
    }

    Component {
        id: dialogComponent
        LayoutDialog {
            act: a => test.sent.push(a)
            inputs: test.inputs
            layouts: test.layouts
        }
    }

    function init() {
        sent = []
        layouts = [interview([box("b", 0.1, { left: 0.1, top: 0, right: 0, bottom: 0 }), box("c", 0.6)])]
    }

    function openOn(layout) {
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.layout = layout
        dialog.open()
        tryCompare(dialog, "opened", true)
        return dialog
    }

    function hint(dialog) {
        return findChild(dialog.contentItem, "changedHint")
    }

    function test_save_keeps_a_swap_made_while_open() {
        const dialog = openOn(layouts[0])
        // Moved here, box 1.
        dialog.edit(0, { rect: { x: 0.2, y: 0.5, w: 0.25, h: 0.25 } })
        // Meanwhile a cue swaps the boxes: the crop goes with the input.
        layouts = [interview([box("c", 0.1), box("b", 0.6, { left: 0.1, top: 0, right: 0, bottom: 0 })])]
        verify(hint(dialog).visible)
        compare(hint(dialog).text, "This layout was changed while the dialog was open. Saving keeps those changes wherever you changed nothing here.")
        dialog.accept()
        compare(sent.length, 1)
        const boxes = sent[0].boxes
        compare(sent[0].action, "set_layout")
        compare(boxes.map(b => b.input), ["c", "b"])
        compare(boxes[0].rect.x, 0.2)
        compare(boxes[1].rect.x, 0.6)
        compare(boxes[0].crop.left, 0)
        compare(boxes[1].crop.left, 0.1)
    }

    function test_own_changes_win_over_live_ones_on_the_same_thing() {
        const dialog = openOn(layouts[0])
        dialog.edit(1, { input: "d", hidden: true })
        // Meanwhile: box 2 gets another input, box 1 is shown and hidden
        // by a shortcut, the background changes.
        const live = interview([Object.assign(box("b", 0.1, { left: 0.1, top: 0, right: 0, bottom: 0 }), { hidden: true }), box("a", 0.6)], null)
        layouts = [live]
        dialog.accept()
        const boxes = sent[0].boxes
        compare(boxes.map(b => b.input), ["b", "d"])
        compare(boxes.map(b => b.hidden), [true, true])
        compare(sent[0].background, null)
    }

    function test_a_removed_box_keeps_the_others_live_changes() {
        const dialog = openOn(layouts[0])
        dialog.removeBox(0)
        layouts = [interview([box("b", 0.1), box("d", 0.6)])]
        dialog.accept()
        compare(sent[0].boxes.map(b => b.input), ["d"])
    }

    // lmix-core takes an input once per layout: such a mix is not sent.
    function test_a_mix_with_an_input_twice_sends_what_is_shown() {
        const dialog = openOn(layouts[0])
        dialog.edit(0, { input: "d" })
        layouts = [interview([box("b", 0.1), box("d", 0.6)])]
        dialog.accept()
        compare(sent[0].boxes.map(b => b.input), ["d", "c"])
    }

    function test_unchanged_layout_saves_as_edited_without_notice() {
        const dialog = openOn(layouts[0])
        dialog.edit(0, { framed: true })
        layouts = [interview(layouts[0].boxes)]
        verify(!hint(dialog).visible)
        dialog.accept()
        compare(sent[0].boxes.map(b => b.border_px), [4, 0])
    }

    function test_boxes_added_elsewhere_show_a_notice_with_reload() {
        const dialog = openOn(layouts[0])
        dialog.edit(0, { framed: true })
        layouts = [interview([box("b", 0.1), box("c", 0.6), box("d", 0.3)])]
        verify(hint(dialog).visible)
        compare(hint(dialog).text, "This layout was changed elsewhere while the dialog was open. Saving replaces it with what is shown here.")
        dialog.reload()
        compare(dialog.boxes.length, 3)
        compare(dialog.boxes[0].framed, false)
        verify(!hint(dialog).visible)
        dialog.accept()
        compare(sent[0].boxes.map(b => b.input), ["b", "c", "d"])
    }

    function test_a_new_layout_is_sent_as_it_is() {
        const dialog = openOn(null)
        verify(!hint(dialog).visible)
        dialog.addBox()
        dialog.edit(0, { input: "b" })
        dialog.accept()
        compare(sent[0].action, "add_layout")
        compare(sent[0].boxes.map(b => b.input), ["b"])
    }
}
