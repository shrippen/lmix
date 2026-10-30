import QtQuick

/**
 * The show state (lmix ShowState) as one property per key, so a binding
 * that reads `show.program` runs again only when the program changes, not
 * with every fader move (29.10.2026). `patch` takes Core.statePatched's
 * operations and sets only the keys they touch; StatePatch keeps the
 * unchanged parts below them too. A key of ShowState missing here is an
 * error (the test state_keys_are_declared in src/state_patch.rs).
 */
QtObject {
    id: state

    /** The whole state as a plain object, the base of the next patch. */
    property var whole: null

    function patch(ops) {
        whole = StatePatch.apply(whole, ops)
        var touched = {}
        for (var i = 0; i < ops.length; i++) {
            if (ops[i].p.length === 0) {
                touched = whole
                break
            }
            touched[ops[i].p[0]] = true
        }
        for (var key in touched) {
            if (key in state) {
                state[key] = whole[key]
            } else {
                console.warn("show state key not in ShowState.qml:", key)
            }
        }
        if (touched.inputs) {
            followInputs()
        }
    }

    // Per input, so a binding on one input runs again only when that one
    // changes: `inputs` itself is new with every fader step of any input,
    // and with 31 inputs every tile, strip and button looked again.

    /** Ids of the inputs in their order; anew only when inputs come, go or move. */
    property var inputIds: []
    /** Input id → cell (`value`: the input); anew together with `inputIds`. */
    property var cells: ({})
    property Component cellComponent: Component {
        QtObject {
            property var value
        }
    }

    /** The input with this id, or undefined. */
    function inputById(id) {
        var cell = cells[id]
        return cell ? cell.value : undefined
    }

    function followInputs() {
        var list = inputs || []
        var ids = list.map(i => i.id)
        if (ids.length !== inputIds.length || ids.some((id, i) => id !== inputIds[i])) {
            var next = {}
            for (var i = 0; i < ids.length; i++) {
                next[ids[i]] = cells[ids[i]] || cellComponent.createObject(state)
            }
            for (var gone in cells) {
                if (!(gone in next)) {
                    cells[gone].destroy()
                }
            }
            for (var j = 0; j < list.length; j++) {
                next[ids[j]].value = list[j]
            }
            cells = next
            inputIds = ids
            return
        }
        // Unchanged inputs are the same objects (StatePatch): no change.
        for (var k = 0; k < list.length; k++) {
            cells[ids[k]].value = list[k]
        }
    }

    property var api_version
    property var path
    property var inputs
    property var preview
    property var program
    property var transitions
    property var quick_play
    property var fade_to_black
    property var slots
    property var outputs
    property var overlays
    property var overlay_last
    property var overlay_channels
    property var overlay_preview
    property var t_bar
    property var buses
    property var master
    property var format
    property var multiview
    property var sample_rate
    property var phones
    property var default_delay_ms
    property var looks_available
    property var layouts
    property var layout_templates
    property var shortcuts
    property var cue_lists
    property var running_cues
    property var go_guard_ms
    property var go_rearm
    property var panic_ms
    property var script_folder
    property var older_format
    property var cue_warnings
    property var targets
    property var feeds
    property var zones
    property var stages
    property var locked
    property var io_locked
    property var cards_on
    property var midi_tbar
    property var restorable
    property var graphics
    property var datasets
    property var data_sources
    property var dynamic_inputs
    property var dynamic_values
}
