import QtQuick
import "."

/**
 * Timing spread (Kante 1.23, for Kontra): how early or late notes were played, as a
 * histogram in milliseconds. A KanteBarChart configured for bins: the count axis on the
 * left, bin edges in ms below (unit at the right end), a solid zero line and a dashed
 * mean line, both labelled, and the key under the chart: "◂ early" left, "late ▸" right.
 *
 *   counts      notes per bin, from the earliest bin
 *   firstBinMs  lower edge of the first bin (e.g. −100)
 *   binMs       width of a bin (e.g. 10)
 *   meanMs      mean deviation; NaN for none
 */
Item {
    id: hist

    property var counts: []
    property real firstBinMs: -100
    property real binMs: 10
    property real meanMs: NaN
    property string unit: "ms"
    property string earlyText: "früh"
    property string lateText: "spät"
    property string meanText: "Mittel"
    property string countText: "%1 Noten"
    property string accessibleName: "Timing"

    readonly property alias chart: bars
    readonly property int total: {
        var s = 0
        for (var i = 0; counts && i < counts.length; i++) s += Number(counts[i]) || 0
        return s
    }

    function ms(v) {
        var r = Math.round(v)
        return (r < 0 ? "−" : r > 0 ? "+" : "") + Math.abs(r)
    }
    function edges() {
        var n = counts ? counts.length : 0
        if (n === 0) return []
        // Every k-th edge on a multiple of k bins, at most about seven labels.
        var k = Math.max(1, Math.ceil((n + 1) / 7))
        var out = []
        for (var i = 0; i <= n; i++) {
            var v = firstBinMs + i * binMs
            var steps = Math.round(v / binMs)
            out.push(steps % k === 0 ? ms(v) : "")
        }
        return out
    }
    function binLabels() {
        var out = []
        for (var i = 0; counts && i < counts.length; i++) {
            var a = firstBinMs + i * binMs
            out.push(ms(a) + "…" + ms(a + binMs) + " " + unit)
        }
        return out
    }
    function markerList() {
        var out = []
        var n = counts ? counts.length : 0
        var zero = (0 - firstBinMs) / binMs
        if (zero >= 0 && zero <= n) out.push({ at: zero, label: "0", dashed: false })
        if (!isNaN(meanMs)) {
            out.push({ at: Math.max(0, Math.min(n, (meanMs - firstBinMs) / binMs)), label: meanText + " " + ms(meanMs) + " " + unit, dashed: true })
        }
        return out
    }

    implicitWidth: KanteStyle.unit(420)
    implicitHeight: KanteStyle.unit(200)

    Accessible.role: Accessible.Chart
    Accessible.name: accessibleName
    Accessible.description: countText.arg(total)
        + (isNaN(meanMs) ? "" : ", " + meanText + " " + ms(meanMs) + " " + unit
           + (Math.round(meanMs) < 0 ? " (" + earlyText + ")" : Math.round(meanMs) > 0 ? " (" + lateText + ")" : ""))

    KanteBarChart {
        id: bars
        width: hist.width
        height: hist.height - key.height - KanteStyle.unit(4)
        values: (hist.counts || []).map(function (v) { return Number(v) || 0 })
        labels: hist.binLabels()
        edgeLabels: hist.edges()
        edgeUnit: hist.unit
        markers: hist.markerList()
        axis: true
        wholeSteps: true
        barFill: 0.9
        Accessible.ignored: true
    }

    Item {
        id: key
        anchors.bottom: parent.bottom
        x: bars.padLeft
        width: hist.width - bars.padLeft
        height: Math.max(earlyRow.height, lateRow.height)
        Row {
            id: earlyRow
            spacing: KanteStyle.unit(4)
            KanteNoteMark { noteState: "early"; color: KanteStyle.mutedTextColor; anchors.verticalCenter: parent.verticalCenter }
            Text { text: hist.earlyText; color: KanteStyle.mutedTextColor; font: KanteStyle.labelFont() }
        }
        Row {
            id: lateRow
            anchors.right: parent.right
            spacing: KanteStyle.unit(4)
            Text { text: hist.lateText; color: KanteStyle.mutedTextColor; font: KanteStyle.labelFont() }
            KanteNoteMark { noteState: "late"; color: KanteStyle.mutedTextColor; anchors.verticalCenter: parent.verticalCenter }
        }
    }
}
