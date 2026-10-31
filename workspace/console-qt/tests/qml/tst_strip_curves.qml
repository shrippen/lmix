import QtQuick
import QtTest
import Kante
import "../../qml"
import "fixtures/state.js" as Data

/**
 * The curves of a channel strip (StripCurves, StripCurve) and the strip's
 * toggles on the Audio page: the maths against what strom's elements do
 * (reference values measured on audiocheblimit and computed by
 * lsp-dsp-units' own curve(), 2026-10-31), the curves drawn only when the
 * strip changes, ON/AFV and Mute sending their actions and following the
 * state.
 */
TestCase {
    id: test
    name: "StripCurves"
    when: windowShown
    // The Audio page takes its inputs only while it is visible.
    visible: true
    width: 1400
    height: 900

    function strip(change) {
        var s = {
            gain_db: 0, pan: 0, delay_ms: 0,
            hpf: { on: false, hz: 80 },
            eq: { on: false, gain_db: [0, 0, 0, 0] },
            gate: { on: false, threshold_db: -40 },
            compressor: { on: false, threshold_db: -20, ratio: 4 }
        }
        if (change) {
            change(s)
        }
        return s
    }

    // --- EQ and high-pass ---------------------------------------------

    function test_bands_peak_at_their_centre() {
        var s = strip(s => { s.eq.on = true; s.eq.gain_db = [6, 0, 0, -12] })
        fuzzyCompare(StripCurves.eqDb(s, 80, 48000), 6, 0.05)
        fuzzyCompare(StripCurves.eqDb(s, 8000, 48000), -12, 0.05)
        // A bell with Q 1 is about an octave wide: two octaves off, little.
        verify(Math.abs(StripCurves.eqDb(s, 1000, 48000)) < 1.5, "1 kHz nearly flat")
        // Half the gain about half an octave from the centre (RBJ, Q 1).
        var half = StripCurves.bellDb(80 * Math.SQRT2, 80, 6, 1, 48000)
        verify(half > 1.5 && half < 4.5, "80·√2 Hz: " + half)
    }

    function test_eq_off_is_flat_whatever_the_gains() {
        var s = strip(s => { s.eq.gain_db = [15, -15, 15, -15] })
        compare(StripCurves.eqDb(s, 80, 48000), 0)
        compare(StripCurves.eqDb(s, 2000, 48000), 0)
    }

    function test_high_pass_matches_audiocheblimit() {
        var s = strip(s => { s.hpf.on = true; s.hpf.hz = 80 })
        // Sines through audiocheblimit mode=high-pass poles=4 cutoff=80.
        fuzzyCompare(StripCurves.eqDb(s, 20, 48000), -53.148, 0.01)
        fuzzyCompare(StripCurves.eqDb(s, 40, 48000), -27.221, 0.01)
        fuzzyCompare(StripCurves.eqDb(s, 70, 48000), -2.856, 0.01)
        fuzzyCompare(StripCurves.eqDb(s, 80, 48000), 0, 0.01)
        fuzzyCompare(StripCurves.eqDb(s, 90, 48000), 0.223, 0.01)
        fuzzyCompare(StripCurves.eqDb(s, 1000, 48000), 0.024, 0.01)
    }

    function test_eq_curve_is_clamped_to_its_scale() {
        var s = strip(s => { s.hpf.on = true; s.hpf.hz = 200 })
        var c = StripCurves.eqCurve(s, 48000, 41)
        compare(c.values.length, 41)
        compare(c.values[0], StripCurves.eqFloorDb, "20 Hz below the scale")
        verify(c.exact[0] < -60, "the exact value stays: " + c.exact[0])
        fuzzyCompare(c.values[40], 0, 0.1)
    }

    // --- Gate and compressor ------------------------------------------

    function test_gate_is_a_hard_step_of_60_db() {
        var s = strip(s => { s.gate.on = true; s.gate.threshold_db = -40 })
        // lsp-dsp-units Gate::curve with zone 1, reduction −60 dB.
        compare(StripCurves.dynamicsDb(s, -41), -101)
        compare(StripCurves.dynamicsDb(s, -40), -100)
        fuzzyCompare(StripCurves.dynamicsDb(s, -39.9), -39.9, 1e-9)
        compare(StripCurves.dynamicsDb(s, -20), -20)
    }

    function test_compressor_matches_lsp_dsp_units() {
        var s = strip(s => { s.compressor.on = true })
        // Compressor::curve, threshold −20 dB, 4:1, knee 0.5 (strom's −6 dB).
        compare(StripCurves.dynamicsDb(s, -30), -30)
        compare(StripCurves.dynamicsDb(s, -20), -20)
        fuzzyCompare(StripCurves.dynamicsDb(s, -19), -19.11, 0.01)
        fuzzyCompare(StripCurves.dynamicsDb(s, -18), -18.43, 0.01)
        fuzzyCompare(StripCurves.dynamicsDb(s, -16.6), -17.83, 0.01)
        // After the knee the line starts anew (lsp-plugins-rs): a step up.
        fuzzyCompare(StripCurves.dynamicsDb(s, -16.4), -16.45, 0.01)
        fuzzyCompare(StripCurves.dynamicsDb(s, -10), -14.85, 0.01)
        fuzzyCompare(StripCurves.dynamicsDb(s, 0), -12.35, 0.01)
    }

    function test_ratio_one_and_off_are_the_identity() {
        var one = strip(s => { s.compressor.on = true; s.compressor.ratio = 1 })
        var off = strip(s => { s.compressor.ratio = 20; s.gate.threshold_db = 0 })
        for (var db = -60; db <= 0; db += 6) {
            fuzzyCompare(StripCurves.dynamicsDb(one, db), db, 1e-9)
            compare(StripCurves.dynamicsDb(off, db), db)
        }
    }

    function test_gate_comes_before_the_compressor() {
        var s = strip(s => { s.gate.on = true; s.compressor.on = true })
        // Gated to −110 dB: far below the compressor's threshold.
        compare(StripCurves.dynamicsDb(s, -50), -110)
        var c = StripCurves.dynamicsCurve(s, 61)
        compare(c.values.length, 61)
        compare(c.values[0], StripCurves.levelFloorDb, "clamped to the scale")
        compare(c.exact[0], -120)
        fuzzyCompare(c.values[60], -12.4, 0.05)
    }

    function test_texts() {
        compare(StripCurves.hzText(80), "80 Hz")
        compare(StripCurves.hzText(1260), "1.3 kHz")
        compare(StripCurves.hzText(20000), "20 kHz")
        compare(StripCurves.dbText(3), "+3.0 dB")
        compare(StripCurves.dbText(-12.5), "−12.5 dB")
        compare(StripCurves.dbText(0), "0.0 dB")
    }

    // --- The component ------------------------------------------------

    Component {
        id: curveView
        StripCurve {
            width: 300
        }
    }

    function test_small_curve_says_what_it_shows() {
        var view = createTemporaryObject(curveView, test, {
            channel: strip(s => { s.hpf.on = true; s.eq.on = true; s.eq.gain_db = [3, 0, 0, 0] })
        })
        verify(view)
        verify(view.active)
        verify(view.summary.indexOf("80 Hz") >= 0, view.summary)
        verify(view.summary.indexOf("+3.0 dB") >= 0, view.summary)
        compare(view.pointText(40), "At 20 kHz: 0.0 dB")
        var off = createTemporaryObject(curveView, test, { channel: strip(), dynamics: true })
        verify(!off.active)
        compare(off.pointText(0), "In −60.0 dB, out −60.0 dB")
    }

    function test_large_curve_has_its_scales() {
        var view = createTemporaryObject(curveView, test, { channel: strip(), detail: true, dynamics: true })
        verify(view)
        waitForRendering(view)
        var texts = []
        function walk(item) {
            if (item.text !== undefined && item.visible) {
                texts.push(String(item.text))
            }
            for (var i = 0; i < item.children.length; i++) {
                walk(item.children[i])
            }
        }
        walk(view)
        verify(texts.indexOf("0 dB") >= 0, "level in, below: " + texts)
        verify(texts.some(t => t.indexOf("60") >= 0 && t.indexOf("dB") > 0), "dB on the left: " + texts)
        verify(texts.some(t => t.indexOf("Level out over level in") >= 0), "caption: " + texts)
    }

    // --- The Audio page -----------------------------------------------

    property var sent: []

    QtObject {
        id: fakeCore
        property string devices: "[]"
        function choices(kind) {
            switch (kind) {
            case "level_limits": return JSON.stringify({ zones: 8 })
            }
            return JSON.stringify({ buses: 8, name_max: 32 })
        }
        function shortName(name, max) {
            return name.slice(0, max)
        }
    }

    ShowState { id: showState }

    QtObject {
        id: fakeShell
        readonly property var show: showState
        readonly property var inputs: showState.inputs || []
        readonly property var levels: QtObject {
            function peak(id) { return undefined }
        }
        readonly property var backend: fakeCore
        function act(action) { test.sent.push(action) }
        function askRemove(kind, item) {}
    }

    Component {
        id: audioPage
        AudioPage {
            width: 1400
            height: 900
            shell: fakeShell
        }
    }

    function find(item, name) {
        if (item.objectName === name) {
            return item
        }
        for (var i = 0; i < item.children.length; i++) {
            var found = find(item.children[i], name)
            if (found) {
                return found
            }
        }
        return null
    }

    /** The StripCurves under `item`. */
    function curvesOf(item, out) {
        if (item.curve !== undefined && item.dynamics !== undefined) {
            out.push(item)
        }
        for (var i = 0; i < item.children.length; i++) {
            curvesOf(item.children[i], out)
        }
        return out
    }

    function loadState() {
        showState.patch([{ p: [], v: JSON.parse(JSON.stringify(Data.state)) }])
    }

    function init() {
        sent = []
        loadState()
    }

    function test_on_afv_switches_and_follows_the_state() {
        var page = createTemporaryObject(audioPage, test)
        verify(page)
        waitForRendering(page)
        var id = Data.state.inputs[0].id
        var follow = find(page, "follow-" + id)
        verify(follow, "ON/AFV of the first strip")
        compare(follow.currentIndex, 0, "ON")
        mouseClick(follow, follow.width - 4, follow.height / 2)
        compare(sent.length, 1)
        compare(sent[0].action, "set_audio_follow")
        compare(sent[0].follow, true)
        compare(sent[0].input, 1)
        // Not confirmed yet: the switch shows the state.
        compare(follow.currentIndex, 0)
        showState.patch([{ p: ["inputs", 0, "audio", "follow_video"], v: true }])
        compare(follow.currentIndex, 1, "AFV once the core says so")
        // Set elsewhere (the narrow mixer's menu): follows too.
        showState.patch([{ p: ["inputs", 0, "audio", "follow_video"], v: false }])
        compare(follow.currentIndex, 0)
        // A click on the chosen one sends nothing.
        mouseClick(follow, 4, follow.height / 2)
        compare(sent.length, 1)
    }

    function test_mute_toggles_and_follows_the_state() {
        var page = createTemporaryObject(audioPage, test)
        verify(page)
        waitForRendering(page)
        var id = Data.state.inputs[0].id
        var mute = find(page, "mute-" + id)
        verify(mute)
        verify(!mute.checked)
        mouseClick(mute)
        compare(sent.length, 1)
        compare(sent[0].action, "set_audio")
        compare(sent[0].on, false)
        verify(!mute.checked, "not before the core says so")
        showState.patch([{ p: ["inputs", 0, "audio", "on"], v: false }])
        verify(mute.checked)
        mouseClick(mute)
        compare(sent[1].on, true)
    }

    function test_curves_are_drawn_only_when_the_strip_changes() {
        var page = createTemporaryObject(audioPage, test)
        verify(page)
        waitForRendering(page)
        var curves = curvesOf(page, [])
        // Two per strip, two more for the selected one.
        compare(curves.length, 2 * Data.state.inputs.length + 2)
        var before = curves.map(c => c.curve)
        // A fader drag: ten volume changes of input 1.
        for (var v = 99; v > 89; v--) {
            showState.patch([{ p: ["inputs", 0, "audio", "volume"], v: v }])
        }
        for (var i = 0; i < curves.length; i++) {
            verify(curves[i].curve === before[i], "curve " + i + " not worked out again")
        }
        // The EQ of input 2 goes on: its curves only.
        showState.patch([{ p: ["inputs", 1, "strip", "eq", "on"], v: true }])
        var id = Data.state.inputs[1].id
        var changed = 0
        for (var k = 0; k < curves.length; k++) {
            if (curves[k].curve !== before[k]) {
                changed++
                compare(curves[k].channel, showState.inputById(id).strip)
            }
        }
        compare(changed, 2, "EQ and dynamics of input 2")
    }

    SignalSpy {
        id: keySpy
        signalName: "selectedKeyChanged"
    }

    function test_a_fader_drag_leaves_the_other_strips_alone() {
        var page = createTemporaryObject(audioPage, test)
        verify(page)
        waitForRendering(page)
        // Nothing chosen: the first strip is the selected one, and its
        // fader moves. Its object is new with every step; the key the
        // strips compare with stays (31.10.2026: every strip looked again).
        compare(page.selectedKey, Data.state.inputs[0].id)
        keySpy.target = page
        keySpy.clear()
        for (var v = 99; v > 89; v--) {
            showState.patch([{ p: ["inputs", 0, "audio", "volume"], v: v }])
        }
        compare(page.selected.audio.volume, 90)
        compare(keySpy.count, 0)
        page.selectedId = Data.state.inputs[1].id
        compare(keySpy.count, 1)
    }

    function test_a_click_on_a_curve_selects_the_strip() {
        var page = createTemporaryObject(audioPage, test)
        verify(page)
        waitForRendering(page)
        var id = Data.state.inputs[2].id
        var eq = find(page, "eq-" + id)
        verify(eq)
        mouseClick(eq)
        compare(page.selectedId, id)
    }
}
