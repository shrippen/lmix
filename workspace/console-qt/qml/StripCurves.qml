pragma Singleton
import QtQuick

/**
 * The static curves of a channel strip (lmix-types ChannelStrip), worked
 * out the way strom's audio mixer applies it: lmix-core sends the strip
 * as `chN_*` properties (strip_properties in lmix-core/src/engine/
 * strom.rs), and strom's builtin.mixer builds per channel
 * gain → hpf → gate → comp → eq (backend/src/blocks/builtin/mixer/
 * builder.rs, elements.rs), with its default DSP backend "rust"
 * (lsp-plugins-rs v1.3.1).
 *
 * - High-pass: `audiocheblimit`, high-pass, 4 poles, Chebyshev type 1
 *   with its default ripple of 0.25 dB. The magnitude here is the
 *   analog prototype through the bilinear transform; it matches the
 *   element to 0.001 dB (measured with sines at 20 Hz … 10 kHz,
 *   2026-10-31).
 * - EQ: `lsp-rs-equalizer`, four peaking (bell) biquads after the Audio
 *   EQ Cookbook, at 80 Hz, 400 Hz, 2 kHz and 8 kHz with Q 1 (strom's
 *   defaults; lmix sends only the gains): exact at the show's rate.
 * - Gate: `lsp-rs-gate` with open = close threshold (no hysteresis) and
 *   its default reduction of −60 dB: a hard step.
 * - Compressor: `lsp-rs-compressor`, downward, knee −6 dB (0.5 linear)
 *   as strom sets it. lsp-plugins-rs places the knee from the threshold
 *   to 1.5 × the threshold (+3.5 dB) and the line after it starts anew
 *   at the knee's end, so the curve steps up by ~1.4 dB there at 4:1:
 *   drawn as it is (checked against lsp-dsp-units' own `curve()`,
 *   2026-10-31; todo.md).
 *
 * Only the static (steady) curves: attack, release and the detector's
 * smoothing are left out. A section that is off is flat (EQ) or the
 * identity (dynamics), like the engine, which passes it through.
 */
QtObject {
    /** Centres of the EQ bands, Hz (strom's chN_eqM_freq defaults). */
    readonly property var bandHz: [80, 400, 2000, 8000]
    /** Q of every band (strom's default). */
    readonly property real bandQ: 1
    /** Pass-band ripple of audiocheblimit, dB (its default). */
    readonly property real hpfRippleDb: 0.25
    /** What the closed gate takes off, dB (lsp-rs-gate's default). */
    readonly property real gateReductionDb: -60
    /** Knee of the compressor as strom sets it, dB (chN_comp_knee). */
    readonly property real kneeDb: -6

    /** Range of the EQ curves, dB. */
    readonly property real eqFloorDb: -18
    readonly property real eqCeilingDb: 18
    /** Range of the dynamics curves (level in and out), dB. */
    readonly property real levelFloorDb: -60

    /** Gain of one peaking biquad at `hz`, dB. */
    function bellDb(hz, centreHz, gainDb, q, rate) {
        if (gainDb === 0) {
            return 0
        }
        var w0 = 2 * Math.PI * centreHz / rate
        var alpha = Math.sin(w0) / (2 * q)
        var a = Math.pow(10, gainDb / 40)
        var c = Math.cos(w0)
        var b0 = 1 + alpha * a, b1 = -2 * c, b2 = 1 - alpha * a
        var a0 = 1 + alpha / a, a1 = -2 * c, a2 = 1 - alpha / a
        var w = 2 * Math.PI * hz / rate
        var cw = Math.cos(w), c2w = Math.cos(2 * w)
        var num = b0 * b0 + b1 * b1 + b2 * b2 + 2 * (b0 * b1 + b1 * b2) * cw + 2 * b0 * b2 * c2w
        var den = a0 * a0 + a1 * a1 + a2 * a2 + 2 * (a0 * a1 + a1 * a2) * cw + 2 * a0 * a2 * c2w
        return 10 * Math.log10(num / den)
    }

    /** Gain of the high-pass at `hz` with its cutoff at `cutoffHz`, dB. */
    function highPassDb(hz, cutoffHz, rate) {
        var eps2 = Math.pow(10, hpfRippleDb / 10) - 1
        var x = Math.tan(Math.PI * cutoffHz / rate) / Math.tan(Math.PI * hz / rate)
        var x2 = x * x
        var t = 8 * x2 * x2 - 8 * x2 + 1
        return 10 * Math.log10((1 + eps2) / (1 + eps2 * t * t))
    }

    /** What high-pass and EQ of `strip` do at `hz`, dB. */
    function eqDb(strip, hz, rate) {
        var db = 0
        if (!strip) {
            return db
        }
        if (strip.hpf && strip.hpf.on) {
            db += highPassDb(hz, strip.hpf.hz, rate)
        }
        if (strip.eq && strip.eq.on) {
            for (var i = 0; i < bandHz.length; i++) {
                db += bellDb(hz, bandHz[i], strip.eq.gain_db[i] || 0, bandQ, rate)
            }
        }
        return db
    }

    /** Level after the gate of `strip` for a level `inDb` going in, dB. */
    function gateDb(strip, inDb) {
        if (!strip || !strip.gate || !strip.gate.on) {
            return inDb
        }
        // At the threshold itself the gate is still closed (x <= start).
        return inDb <= strip.gate.threshold_db ? inDb + gateReductionDb : inDb
    }

    /** Level after the compressor of `strip` for `inDb` going in, dB. */
    function compressorDb(strip, inDb) {
        if (!strip || !strip.compressor || !strip.compressor.on) {
            return inDb
        }
        var start = strip.compressor.threshold_db
        var width = 20 * Math.log10(1 + Math.pow(10, kneeDb / 20))
        var end = start + width
        var slope = 1 / Math.max(1, strip.compressor.ratio) - 1
        if (inDb <= start) {
            return inDb
        }
        if (inDb >= end) {
            return inDb + slope * (inDb - end)
        }
        var d = inDb - start
        return inDb + slope / (2 * width) * d * d
    }

    /** Level out of gate and compressor for `inDb` in, dB. */
    function dynamicsDb(strip, inDb) {
        return compressorDb(strip, gateDb(strip, inDb))
    }

    /** `count` frequencies from 20 Hz to 20 kHz, evenly on a log scale. */
    function frequencies(count) {
        var out = []
        for (var i = 0; i < count; i++) {
            out.push(20 * Math.pow(1000, i / (count - 1)))
        }
        return out
    }

    /** `count` levels from levelFloorDb to 0 dB, evenly. */
    function levels(count) {
        var out = []
        for (var i = 0; i < count; i++) {
            out.push(levelFloorDb - levelFloorDb * i / (count - 1))
        }
        return out
    }

    function tenth(v) {
        return Math.round(v * 10) / 10
    }

    /** The EQ curve of `strip` at `frequencies(count)`: `values` clamped
     *  to the scale and rounded to 0.1 dB for drawing, `exact` unclamped. */
    function eqCurve(strip, rate, count) {
        var values = [], exact = []
        var hz = frequencies(count)
        for (var i = 0; i < hz.length; i++) {
            var db = tenth(eqDb(strip, hz[i], rate))
            exact.push(db)
            values.push(Math.max(eqFloorDb, Math.min(eqCeilingDb, db)))
        }
        return { values: values, exact: exact }
    }

    /** The transfer curve of `strip` at `levels(count)`, as eqCurve. */
    function dynamicsCurve(strip, count) {
        var values = [], exact = []
        var ins = levels(count)
        for (var i = 0; i < ins.length; i++) {
            var db = tenth(dynamicsDb(strip, ins[i]))
            exact.push(db)
            values.push(Math.max(levelFloorDb, Math.min(0, db)))
        }
        return { values: values, exact: exact }
    }

    /** "80 Hz", "1.3 kHz", "20 kHz". */
    function hzText(hz) {
        return hz < 1000 ? Math.round(hz) + " Hz"
            : (hz < 10000 ? (hz / 1000).toFixed(1) : Math.round(hz / 1000)) + " kHz"
    }

    /** "+3.0 dB", "−12.5 dB", "0.0 dB" (a real minus sign). */
    function dbText(db) {
        var t = Math.abs(db).toFixed(1)
        return (db > 0.04 ? "+" : (db < -0.04 ? "−" : "")) + t + " dB"
    }
}
