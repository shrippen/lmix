pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * A channel strip's EQ or dynamics as a curve (StripCurves has the
 * maths). Small (`detail: false`): a line in the strip, its values and
 * the value under the pointer in a tooltip, a click emits `activated`.
 * Large (`detail: true`): the chart with its scales, dB on the left,
 * frequency or level in below, and a read-out on hover.
 *
 * Worked out only when `channel` or the rate change: a state update
 * keeps the strip's object while its values stay (StatePatch), so a
 * fader move draws nothing anew.
 */
Item {
    id: view

    /** `strip` of an input (lmix-types ChannelStrip), or null. */
    property var channel: null
    /** The show's sample rate (Hz). */
    property int sampleRate: 48000
    /** false: high-pass and EQ over frequency; true: gate and compressor,
     *  level out over level in. */
    property bool dynamics: false
    property bool detail: false
    signal activated()

    readonly property int count: detail ? 121 : 41
    readonly property var curve: dynamics ? StripCurves.dynamicsCurve(channel, count)
        : StripCurves.eqCurve(channel, sampleRate, count)
    /** Whether a section of the curve is on (else it is flat or straight). */
    readonly property bool active: channel !== null && (dynamics
        ? (channel.gate.on || channel.compressor.on)
        : (channel.hpf.on || channel.eq.on))
    readonly property real floorDb: dynamics ? StripCurves.levelFloorDb : StripCurves.eqFloorDb
    // KanteLineChart takes a top of 0 as "the largest value": a hair
    // above it keeps the dynamics scale at 0 dB (todo.md).
    readonly property real ceilingDb: dynamics ? 1e-6 : StripCurves.eqCeilingDb

    /** What the curve shows, in words. */
    readonly property string summary: {
        if (!active) {
            return dynamics ? qsTr("Gate and compressor off") : qsTr("High-pass and EQ off")
        }
        var parts = []
        if (dynamics) {
            if (channel.gate.on) {
                parts.push(qsTr("Gate closes at %1").arg(StripCurves.dbText(channel.gate.threshold_db)))
            }
            if (channel.compressor.on) {
                parts.push(qsTr("Compressor from %1 at %2 : 1").arg(StripCurves.dbText(channel.compressor.threshold_db))
                    .arg(channel.compressor.ratio.toFixed(1)))
            }
        } else {
            if (channel.hpf.on) {
                parts.push(qsTr("High-pass at %1").arg(StripCurves.hzText(channel.hpf.hz)))
            }
            if (channel.eq.on) {
                parts.push(qsTr("EQ %1").arg(StripCurves.bandHz.map((hz, i) =>
                    StripCurves.hzText(hz) + " " + StripCurves.dbText(channel.eq.gain_db[i])).join(" · ")))
            }
        }
        return parts.join("\n")
    }

    /** The point `i` in words: where it is and what the curve does there. */
    function pointText(i) {
        if (i < 0 || i >= curve.exact.length) {
            return ""
        }
        if (dynamics) {
            var level = StripCurves.levels(count)[i]
            return qsTr("In %1, out %2").arg(StripCurves.dbText(level)).arg(StripCurves.dbText(curve.exact[i]))
        }
        return qsTr("At %1: %2").arg(StripCurves.hzText(StripCurves.frequencies(count)[i])).arg(StripCurves.dbText(curve.exact[i]))
    }

    implicitWidth: detail ? Kirigami.Units.gridUnit * 16 : Kirigami.Units.gridUnit * 5
    implicitHeight: detail ? large.implicitHeight : Kirigami.Units.gridUnit
    Accessible.role: Accessible.Chart
    Accessible.name: (dynamics ? qsTr("Dynamics") : qsTr("EQ")) + ": " + summary

    // Small: the line alone. Loaded, so 40 strips do not carry a hidden
    // large chart each.
    Loader {
        id: small
        active: !view.detail
        anchors.fill: parent
        sourceComponent: KanteSparkline {
            values: view.curve.values
            minValue: view.floorDb
            maxValue: view.ceilingDb
            colorIndex: view.dynamics ? 1 : 3
            opacity: view.active ? 1 : 0.4
        }
    }
    HoverHandler {
        id: hover
        enabled: !view.detail
    }
    TapHandler {
        enabled: !view.detail
        onTapped: view.activated()
    }
    QQC2.ToolTip.visible: hover.hovered
    QQC2.ToolTip.text: hover.hovered && small.item
        ? view.summary + "\n" + view.pointText(small.item.nearest(hover.point.position.x))
        : ""

    // Large: scales and a read-out.
    Loader {
        id: large
        active: view.detail
        width: parent.width
        sourceComponent: ColumnLayout {
            spacing: 0

            QQC2.Label {
                Layout.fillWidth: true
                text: view.dynamics ? qsTr("Level out over level in (dB)") : qsTr("Gain (dB) over frequency (Hz)")
                color: KanteStyle.mutedTextColor
                font: KanteStyle.labelFont()
                elide: Text.ElideRight
            }
            KanteLineChart {
                id: chart
                Layout.fillWidth: true
                Layout.preferredHeight: Kirigami.Units.gridUnit * 7
                series: view.detail ? [view.curve.values] : []
                minValue: view.floorDb
                maxValue: view.ceilingDb
                axis: true
                unit: " dB"
                colorIndex: view.dynamics ? 1 : 3
                opacity: view.active ? 1 : 0.5
                labels: {
                    if (!view.detail) {
                        return []
                    }
                    var out = []
                    var at = view.dynamics ? StripCurves.levels(view.count) : StripCurves.frequencies(view.count)
                    for (var i = 0; i < at.length; i++) {
                        var where = view.dynamics ? qsTr("in %1").arg(StripCurves.dbText(at[i])) : StripCurves.hzText(at[i])
                        // Off the scale: the value itself beside the place.
                        out.push(view.curve.exact[i] === view.curve.values[i] ? where
                            : where + " (" + StripCurves.dbText(view.curve.exact[i]) + ")")
                    }
                    return out
                }
            }
            // The scale below: frequency (log) or level in.
            Item {
                id: scale
                Layout.fillWidth: true
                implicitHeight: tickMetrics.height + Kirigami.Units.smallSpacing
                readonly property var ticks: view.dynamics
                    ? [[-60, "−60"], [-45, "−45"], [-30, "−30"], [-15, "−15"], [0, "0 dB"]]
                    : [[20, "20"], [100, "100"], [1000, "1k"], [10000, "10k"], [20000, "20k Hz"]]
                Text {
                    id: tickMetrics
                    visible: false
                    text: "0"
                    font: KanteStyle.monoFont(KanteStyle.labelFont().pointSize, false)
                }
                Repeater {
                    model: view.detail ? scale.ticks : []
                    delegate: Text {
                        required property var modelData
                        readonly property real share: view.dynamics
                            ? (modelData[0] - StripCurves.levelFloorDb) / -StripCurves.levelFloorDb
                            : Math.log(modelData[0] / 20) / Math.log(1000)
                        readonly property real at: chart.padLeft + chart.mark / 2 + share * (chart.width - chart.padLeft - chart.mark)
                        x: Math.max(0, Math.min(scale.width - width, at - width / 2))
                        y: Kirigami.Units.smallSpacing
                        text: modelData[1]
                        color: KanteStyle.mutedTextColor
                        font: tickMetrics.font
                    }
                }
            }
        }
    }
}
