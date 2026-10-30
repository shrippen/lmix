import QtQuick
import QtTest
import "../../qml"

/**
 * Editing an output (AddOutputDialog's edit mode, M07 Settings…): the
 * form holds its settings, Save sends `update_output` and, only when the
 * sound changed, `set_output_bus`; while it sends the fields are locked
 * with a line saying so; a new output afterwards starts empty again.
 */
TestCase {
    id: test
    name: "AddOutputDialog"
    when: windowShown
    width: 1000
    height: 800

    property var added: []
    property var updates: []

    QtObject {
        id: fakeCore
        property string devices: ""
        function choices(list) {
            switch (list) {
            case "outputs": return JSON.stringify(["srt", "rtmp", "record", "ndi", "card", "window", "fullscreen", "drm"])
            case "containers": return JSON.stringify(["mkv", "mp4"])
            case "card_modes": return JSON.stringify(["1080p25", "1080i50"])
            case "card_channels": return JSON.stringify(["2", "8", "16"])
            case "card_limits": return JSON.stringify({ sound_delay_ms: 500 })
            }
            return "[]"
        }
        function refreshLists() {}
        function addOutput(form) {
            test.added.push(JSON.parse(form))
            return true
        }
        // The bridge builds the action; here it shows the form it got.
        function updateOutputAction(output, form) {
            return JSON.stringify({ action: "update_output", output: output, form: JSON.parse(form) })
        }
    }

    readonly property var buses: [{ id: "b1", name: "In-Ear" }, { id: "b2", name: "Stream" }]
    readonly property var inputs: [{ id: "i1", number: 1, name: "Cam" }, { id: "i2", number: 2, name: "Slides" }]
    property var outputs: []
    function output(state) {
        return {
            id: "o1", number: 1, name: "CDN", bus: "b2",
            spec: { kind: "srt", uri: "srt://cdn:9000", video_kbps: 8000,
                encoding: { codec: "h265", rate: "cbr", keyframe_ms: 4000 } },
            source: { from: "input", input: "i2" },
            status: state === "running" ? { state: "running", since_ms: 0 } : { state: state }
        }
    }

    Component {
        id: dialogComponent
        AddOutputDialog {
            core: fakeCore
            inputs: test.inputs
            buses: test.buses
            outputs: test.outputs
            onUpdateRequested: (before, actions) => test.updates.push({ before: before, actions: actions })
        }
    }

    function init() {
        added = []
        updates = []
        outputs = [output("stopped")]
    }

    function opened(dialog) {
        tryCompare(dialog, "opened", true)
        return dialog
    }

    function test_the_form_holds_the_outputs_settings() {
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.edit("o1")
        opened(dialog)
        compare(dialog.title, "Output settings")
        compare(dialog.acceptText, "Save")
        const form = JSON.parse(dialog.form())
        compare(form.kind, "srt")
        compare(form.text, "srt://cdn:9000")
        compare(form.video_kbps, 8000)
        compare(form.encoding, { codec: "h265", rate: "cbr", keyframe_ms: 4000 })
        compare(form.source, "i2")
        compare(form.name, "CDN")
        compare(form.bus, "b2")
        verify(dialog.acceptEnabled)
    }

    function test_save_sends_the_update_and_the_bus_only_when_it_changed() {
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.edit("o1")
        opened(dialog)
        dialog.accept()
        compare(updates.length, 1)
        compare(updates[0].before.name, "CDN")
        compare(updates[0].actions.length, 1, "same sound: no set_output_bus")
        compare(updates[0].actions[0].action, "update_output")
        compare(updates[0].actions[0].output, "o1")
        compare(added.length, 0)

        // Back to the source's own sound: the bus goes with its own action.
        outputs = [Object.assign(output("stopped"), { bus: "b1" })]
        dialog.edit("o1")
        opened(dialog)
        dialog.soundIndex = 0
        dialog.accept()
        compare(updates[1].actions[1], { action: "set_output_bus", output: "o1", bus: null })
    }

    function test_a_sending_output_is_shown_locked() {
        outputs = [output("running")]
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.edit("o1")
        opened(dialog)
        const fields = findChild(dialog.contentItem, "fields")
        const hint = findChild(dialog.contentItem, "lockedHint")
        verify(dialog.locked)
        compare(fields.enabled, false)
        compare(hint.visible, true)
        compare(dialog.acceptEnabled, false)
        // The settings stay readable.
        compare(JSON.parse(dialog.form()).text, "srt://cdn:9000")
        dialog.accept()
        compare(updates.length, 0)

        // Waiting for the cards locks it too; stopped elsewhere, it opens.
        outputs = [output("cards_off")]
        verify(dialog.locked)
        outputs = [output("stopped")]
        compare(fields.enabled, true)
        compare(hint.visible, false)
        verify(dialog.acceptEnabled)
    }

    function test_an_output_removed_meanwhile_closes_it() {
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.edit("o1")
        opened(dialog)
        outputs = []
        tryCompare(dialog, "visible", false)
        compare(updates.length, 0)
    }

    function test_a_new_output_after_editing_starts_empty() {
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.edit("o1")
        opened(dialog)
        dialog.reject()
        tryCompare(dialog, "visible", false)
        dialog.add()
        opened(dialog)
        compare(dialog.title, "Add output")
        const form = JSON.parse(dialog.form())
        compare(form.text, "")
        compare(form.name, "")
        compare(form.source, "program")
        compare(form.bus, "")
        compare(form.video_kbps, 6000)
        compare(form.encoding.codec, "h264")
    }
}
