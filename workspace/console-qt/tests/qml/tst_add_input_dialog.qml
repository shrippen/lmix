import QtQuick
import QtTest
import "../../qml"

/**
 * An SRT input from fields (SRC.12): mode, address, port, latency,
 * passphrase, key length and stream id make the address (Rust puts it
 * together and checks it as the core does: forms::srt_uri, srt_check;
 * here stand-ins), the decoder delay goes as an option; what the check
 * refuses (port, host, a passphrase outside 10–79 bytes) holds the button
 * with a line saying why; the shown address leaves the passphrase out; a
 * replaced input and a pasted srt:// address fill the fields, keeping
 * parameters without a field; a paste that does not read says so; typing
 * an address by hand fills the fields only when the field is left.
 */
TestCase {
    id: test
    name: "AddInputDialog"
    when: windowShown
    width: 1000
    height: 900

    property var added: []
    property var replaced: []
    property var inputs: [
        { id: "cam", number: 1, name: "Camera", source: { kind: "pattern", pattern: "bars" } },
        { id: "far", number: 2, name: "Remote", source: { kind: "srt", decoder_delay_ms: 300,
            uri: "srt://far.example:7001?mode=rendezvous&latency=800&passphrase=0123456789abc&pbkeylen=24&streamid=cam2&poll-timeout=500" } }
    ]

    QtObject {
        id: fakeCore
        property string devices: ""
        function choices(list) {
            switch (list) {
            case "sources": return JSON.stringify(["pattern", "srt", "url"])
            case "patterns": return JSON.stringify(["bars"])
            case "srt_modes": return JSON.stringify(["caller", "listener", "rendezvous"])
            case "srt_limits": return JSON.stringify({ passphrase_min: 10, passphrase_max: 79, key_lengths: [16, 24, 32],
                latency_ms: 125, latency_max_ms: 10000, decoder_delay_max_ms: 10000, stream_id_max: 512 })
            }
            return "[]"
        }
        function refreshLists() {}
        // Stands in for forms::srt_uri: the fields in a fixed order.
        function srtUri(fields) {
            const f = JSON.parse(fields)
            const query = []
            if (f.mode !== "caller") query.push("mode=" + f.mode)
            if (f.latency_ms !== undefined) query.push("latency=" + f.latency_ms)
            if (f.passphrase) query.push("passphrase=" + f.passphrase)
            if (f.key_length) query.push("pbkeylen=" + f.key_length)
            if (f.stream_id) query.push("streamid=" + f.stream_id)
            f.extra.forEach(e => query.push(e[0] + "=" + e[1]))
            return "srt://" + f.host + ":" + f.port + (query.length ? "?" + query.join("&") : "")
        }
        // Stands in for forms::srt_check: the rules the tests here touch.
        // (The real check, with hosts, zones and brackets: forms.rs tests.)
        function srtCheck(fields) {
            const f = JSON.parse(fields)
            const bytes = unescape(encodeURIComponent(f.passphrase || "")).length
            const p = !(f.port >= 1 && f.port <= 65535) ? { problem: "no_port" }
                : f.host === "" && f.mode !== "listener" ? { problem: "needs_host", mode: f.mode }
                : /[:\s\/?#@]/.test(f.host) ? { problem: "bad_host", host: f.host }
                : f.latency_ms > 10000 ? { problem: "latency_too_high", ms: f.latency_ms, max: 10000 }
                : bytes > 0 && (bytes < 10 || bytes > 79) ? { problem: "passphrase_length", bytes: bytes, min: 10, max: 79 }
                : f.key_length && bytes === 0 ? { problem: "key_needs_passphrase" }
                : null
            return p ? JSON.stringify(p) : ""
        }
        // Stands in for forms::srt_fields: knows the two addresses used here.
        function srtFields(uri) {
            if (uri.indexOf("far.example") >= 0)
                return JSON.stringify({ mode: "rendezvous", host: "far.example", port: 7001, latency_ms: 800,
                    passphrase: "0123456789abc", key_length: 24, stream_id: "cam2", extra: [["poll-timeout", "500"]] })
            if (uri === "srt://:9100?mode=listener")
                return JSON.stringify({ mode: "listener", host: "", port: 9100, passphrase: "", stream_id: "" })
            if (uri === "srt://cam:9000")
                return JSON.stringify({ mode: "caller", host: "cam", port: 9000, passphrase: "", stream_id: "" })
            if (uri === "srt://cams:9000")
                return JSON.stringify({ mode: "caller", host: "cams", port: 9000, passphrase: "", stream_id: "" })
            if (uri.indexOf("slow.example") >= 0)
                return JSON.stringify({ mode: "caller", host: "slow.example", port: 9000, latency_ms: 20000, passphrase: "", stream_id: "" })
            return JSON.stringify({ problem: "no_port" })
        }
        function addInput(kind, value, options, name) {
            test.added.push({ kind: kind, value: value, options: JSON.parse(options) })
            return true
        }
        function replaceSource(number, kind, value, options) {
            test.replaced.push({ number: number, kind: kind, value: value, options: JSON.parse(options) })
            return true
        }
    }

    Component {
        id: dialogComponent
        AddInputDialog {
            core: fakeCore
            inputs: test.inputs
        }
    }

    function init() {
        added = []
        replaced = []
    }

    /** The item named `name` below `item`, or null. */
    function named(item, name) {
        if (item.objectName === name)
            return item
        var kids = item.children || []
        if (item.contentItem && kids.indexOf(item.contentItem) < 0)
            kids = kids.concat([item.contentItem])
        for (var i = 0; i < kids.length; i++) {
            var found = named(kids[i], name)
            if (found)
                return found
        }
        return null
    }

    function openSrt(replacing) {
        var dialog = createTemporaryObject(dialogComponent, test, { replacing: replacing || null })
        dialog.open()
        tryCompare(dialog, "opened", true)
        if (!replacing)
            dialog.choose("srt")
        return dialog
    }

    function test_fields_make_the_address_and_the_delay_an_option() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        var port = named(dialog.contentItem, "srtPort")
        var problem = named(dialog.contentItem, "srtProblem")
        verify(host && port && problem)
        verify(host.visible)
        verify(!dialog.acceptEnabled, "no port yet")
        verify(!problem.visible, "nothing wrong on a form not yet filled in")
        verify(!named(dialog.contentItem, "srtAddress").visible)

        port.text = "9000"
        verify(!dialog.acceptEnabled, "a caller needs a host")
        verify(problem.visible)
        verify(problem.text.length > 0)

        host.text = "cam.local"
        named(dialog.contentItem, "srtLatency").value = 400
        named(dialog.contentItem, "srtStreamId").text = "cam1"
        named(dialog.contentItem, "srtDecoderDelay").value = 200
        verify(dialog.acceptEnabled)
        verify(!problem.visible)
        compare(named(dialog.contentItem, "srtAddress").text, "srt://cam.local:9000?latency=400&streamid=cam1")

        dialog.accept()
        compare(added.length, 1)
        compare(added[0].kind, "srt")
        compare(added[0].value, "srt://cam.local:9000?latency=400&streamid=cam1")
        compare(added[0].options.decoder_delay_ms, 200)
    }

    function test_a_listener_needs_no_host() {
        var dialog = openSrt()
        dialog.srtModeIndex = 1
        named(dialog.contentItem, "srtPort").text = "9000"
        verify(dialog.acceptEnabled)
        compare(dialog.value, "srt://:9000?mode=listener", "the default latency stays out")
    }

    function test_a_passphrase_srt_refuses_holds_the_button_and_is_not_shown() {
        var dialog = openSrt()
        named(dialog.contentItem, "srtHost").text = "cam"
        named(dialog.contentItem, "srtPort").text = "9000"
        var pass = named(dialog.contentItem, "srtPassphrase")
        pass.text = "short"
        verify(!dialog.acceptEnabled, "under 10 characters")
        verify(named(dialog.contentItem, "srtProblem").visible)

        pass.text = "long enough secret"
        verify(dialog.acceptEnabled)
        compare(dialog.value, "srt://cam:9000?passphrase=long enough secret")
        var shown = named(dialog.contentItem, "srtAddress").text
        verify(shown.indexOf("secret") < 0, shown)

        // SRT counts bytes: four umlauts are 8, five are 10.
        pass.text = "ääää"
        verify(!dialog.acceptEnabled, "8 bytes")
        verify(named(dialog.contentItem, "srtProblem").text.indexOf("8") >= 0)
        pass.text = "äääää"
        verify(dialog.acceptEnabled, "10 bytes")

        pass.text = ""
        named(dialog.contentItem, "srtKeyLength").currentIndex = 3
        verify(!dialog.acceptEnabled, "a key length needs a passphrase")
    }

    function test_a_replaced_srt_input_fills_the_fields_and_keeps_the_rest() {
        var dialog = openSrt(inputs[1])
        compare(dialog.kind, "srt")
        compare(dialog.srtMode, "rendezvous")
        compare(named(dialog.contentItem, "srtHost").text, "far.example")
        compare(named(dialog.contentItem, "srtPort").text, "7001")
        compare(named(dialog.contentItem, "srtLatency").value, 800)
        compare(named(dialog.contentItem, "srtPassphrase").text, "0123456789abc")
        compare(named(dialog.contentItem, "srtKeyLength").currentIndex, 2, "AES-192")
        compare(named(dialog.contentItem, "srtStreamId").text, "cam2")
        compare(named(dialog.contentItem, "srtDecoderDelay").value, 300)
        verify(dialog.acceptEnabled)

        dialog.accept()
        compare(replaced.length, 1)
        compare(replaced[0].number, 2)
        compare(replaced[0].value, "srt://far.example:7001?mode=rendezvous&latency=800&passphrase=0123456789abc&pbkeylen=24&streamid=cam2&poll-timeout=500")
        compare(replaced[0].options.decoder_delay_ms, 300)
    }

    function test_a_pasted_address_fills_the_fields_and_a_new_dialog_starts_empty() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        // Pasting is an edit by the user in the focused field: textEdited.
        host.forceActiveFocus()
        host.text = "srt://:9100?mode=listener"
        host.textEdited()
        compare(dialog.srtMode, "listener")
        compare(host.text, "")
        compare(named(dialog.contentItem, "srtPort").text, "9100")
        compare(named(dialog.contentItem, "srtLatency").value, 125, "none in the address: the default")

        dialog.close()
        dialog.open()
        tryCompare(dialog, "opened", true)
        compare(named(dialog.contentItem, "srtPort").text, "")
        compare(dialog.srtModeIndex, 0)
    }

    function test_a_host_with_a_port_or_blanks_holds_the_button() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        named(dialog.contentItem, "srtPort").text = "9000"
        for (var bad of ["cam.local:9000", "cam local", "user@cam"]) {
            host.text = bad
            verify(!dialog.acceptEnabled, bad)
            verify(named(dialog.contentItem, "srtProblem").visible, bad)
            verify(host.invalid, bad)
        }
        host.text = "cam.local"
        verify(dialog.acceptEnabled)
    }

    function test_a_paste_that_does_not_read_says_so() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        named(dialog.contentItem, "srtPort").text = "9000"
        host.forceActiveFocus()
        host.text = "srt://cam"
        host.textEdited()
        verify(!dialog.acceptEnabled)
        var line = named(dialog.contentItem, "srtProblem")
        verify(line.visible)
        verify(line.text.indexOf("does not read") >= 0, line.text)
        verify(line.text.indexOf("port") >= 0, line.text)
        compare(host.text, "srt://cam", "stays to be corrected")
    }

    function test_typing_an_address_fills_the_fields_when_the_field_is_left() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        host.forceActiveFocus()
        keyClick(Qt.Key_S)
        var typed = "rt://cam:9000"
        for (var i = 0; i < typed.length; i++)
            keyClick(typed[i])
        compare(host.text, "srt://cam:9000", "typed key by key, nothing taken apart yet")
        compare(named(dialog.contentItem, "srtPort").text, "")
        host.editingFinished()
        compare(host.text, "cam")
        compare(named(dialog.contentItem, "srtPort").text, "9000")
    }

    function test_a_latency_above_the_limit_is_shown_not_cut() {
        var dialog = createTemporaryObject(dialogComponent, test, {
            replacing: { id: "slow", number: 3, name: "Slow", source: { kind: "srt", uri: "srt://slow.example:9000?latency=20000" } }
        })
        dialog.open()
        tryCompare(dialog, "opened", true)
        compare(named(dialog.contentItem, "srtLatency").value, 20000)
        verify(!dialog.acceptEnabled)
        verify(named(dialog.contentItem, "srtProblem").text.indexOf("20000") >= 0)
    }

    function test_a_paste_over_a_selection_fills_the_fields() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        host.forceActiveFocus()
        host.text = "cam.example.org"
        host.textEdited()
        compare(host.text, "cam.example.org")
        // The whole old host selected, an address of the same length pasted.
        host.text = "srt://cams:9000"
        compare(host.text.length, "cam.example.org".length)
        host.textEdited()
        compare(host.text, "cams")
        compare(named(dialog.contentItem, "srtPort").text, "9000")
    }

    function test_typing_over_a_selected_character_fills_nothing() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        host.forceActiveFocus()
        var typed = "srt://cam:9001"
        for (var i = 0; i < typed.length; i++)
            keyClick(typed[i])
        compare(host.text, "srt://cam:9001")
        // A typo fixed by selecting it and typing over it: same length, and
        // the result reads as an address (the stand-in knows srt://cam:9000).
        host.select(host.text.length - 1, host.text.length)
        keyClick(Qt.Key_0)
        compare(host.text, "srt://cam:9000", "nothing taken apart while typing")
        compare(named(dialog.contentItem, "srtPort").text, "")
    }

    function test_a_core_refusal_names_the_srt_problem() {
        compare(Texts.message(JSON.stringify({ error: { error: "bad_srt",
            problem: { problem: "passphrase_length", bytes: 8, min: 10, max: 79 } } })),
            "SRT: A passphrase has 10 to 79 bytes, this one 8; umlauts and other special characters count twice or more.")
    }

    function test_a_corrected_paste_over_a_refused_one_fills_the_fields() {
        var dialog = openSrt()
        var host = named(dialog.contentItem, "srtHost")
        host.forceActiveFocus()
        host.text = "srt://camz:9001"
        host.textEdited()
        verify(named(dialog.contentItem, "srtProblem").text.indexOf("does not read") >= 0)
        // All selected, a corrected address of the same length pasted: an
        // address before too, but more than one character is new.
        host.text = "srt://cams:9000"
        host.textEdited()
        compare(host.text, "cams")
        compare(named(dialog.contentItem, "srtPort").text, "9000")
    }
}
