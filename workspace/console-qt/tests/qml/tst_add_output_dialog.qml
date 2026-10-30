import QtQuick
import QtTest
import "../../qml"

/**
 * Editing an output (AddOutputDialog's edit mode, M07 Settings…): the
 * form holds its settings, Save hands on one `update_output` with the
 * sound in it; while it sends, waits for the cards or fails back the
 * fields are locked with a line saying so; a source input or card that is
 * gone stays chosen; a new output afterwards starts empty again.
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
    readonly property string cardDevices: JSON.stringify([{ id: "0", name: "DeckLink 1", kind: "card" }, { id: "1", name: "DeckLink 2", kind: "card" }])
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
            onUpdateRequested: (before, action) => test.updates.push({ before: before, action: action })
        }
    }

    function init() {
        added = []
        updates = []
        outputs = [output("stopped")]
        fakeCore.devices = ""
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

    function test_save_sends_one_update_with_the_sound() {
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.edit("o1")
        opened(dialog)
        dialog.accept()
        compare(updates.length, 1)
        compare(updates[0].before.name, "CDN")
        compare(updates[0].action.action, "update_output")
        compare(updates[0].action.output, "o1")
        compare(updates[0].action.form.bus, "b2")
        compare(added.length, 0)

        // Back to the source's own sound: in the same action.
        dialog.edit("o1")
        opened(dialog)
        dialog.soundIndex = 0
        dialog.accept()
        compare(updates.length, 2)
        compare(updates[1].action.form.bus, "")
    }

    // Saving only a new name keeps what is gone as it was.
    function test_a_missing_source_or_card_stays_chosen() {
        outputs = [Object.assign(output("failed"), { source: { from: "input", input: "gone" } })]
        const dialog = createTemporaryObject(dialogComponent, test)
        dialog.edit("o1")
        opened(dialog)
        compare(JSON.parse(dialog.form()).source, "gone")
        dialog.reject()
        tryCompare(dialog, "visible", false)

        fakeCore.devices = fakeCore.cardDevices
        outputs = [Object.assign(output("stopped"), {
            spec: { kind: "card", device: 7, mode: "1080i50" }, source: { from: "program" } })]
        dialog.edit("o1")
        opened(dialog)
        compare(JSON.parse(dialog.form()).text, "7", "not the first card")
        compare(dialog.cardChoices[dialog.cardChoices.length - 1].name, "Card 8 (not found)")
        // A listed card is picked as it is.
        dialog.reject()
        tryCompare(dialog, "visible", false)
        outputs = [Object.assign(output("stopped"), {
            spec: { kind: "card", device: 1, mode: "1080i50" }, source: { from: "program" } })]
        dialog.edit("o1")
        opened(dialog)
        compare(JSON.parse(dialog.form()).text, "1")
        compare(dialog.cardChoices.length, 2)
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

        // Waiting for the cards locks it too, and a failback about to
        // start it again; failed with nothing retrying, it is open.
        outputs = [output("cards_off")]
        verify(dialog.locked)
        outputs = [Object.assign(output("failed"), { failback: true })]
        verify(dialog.locked)
        outputs = [output("failed")]
        verify(!dialog.locked)
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
