import QtQuick
import "."

/**
 * Text with readings over the kanji (furigana, Kante 1.25, for Kaiwa; web: `.ruby`). Qt Text
 * has no ruby, so the text is a Flow of pieces: a word with a reading is one piece (reading
 * above, base below), every other character is a piece of its own, so the line wraps
 * anywhere in Japanese text. The room for the readings stays reserved in every density,
 * so switching never moves the lines. A reading wider than its word may reach half a
 * reading character over each neighbour when both neighbours have no reading, as in Japanese
 * typesetting, instead of opening a gap; next to another reading it never overlaps. Readings take the muted text colour, new words too
 * (decision D-1).
 *
 *   segments  [{text: "駅", reading: "えき", isNew: false}, {text: "までは"}, …]
 *   markup    the same as a string: "{駅|えき}までは{歩|ある|new}いて{五分|ごふん}です。"
 *   density   All (every reading), New (only isNew), None (none)
 */
Flow {
    id: ruby

    enum Density {
        All,
        New,
        None
    }

    property var segments: []
    property string markup: ""
    property int density: KanteRubyText.Density.All
    property font font: KanteStyle.defaultFont
    property color color: KanteStyle.textColor
    property color readingColor: KanteStyle.mutedTextColor
    /** Reading size as a share of the base size. */
    property real readingScale: 0.5

    /** The plain base text (what a screen reader reads, what a copy takes). */
    readonly property string text: {
        var s = ""
        for (var i = 0; i < pieces.length; i++) s += pieces[i].text
        return s
    }

    /** "{漢字|かんじ}" or "{漢字|かんじ|new}" around words with a reading; anything else is plain text. */
    function parse(m) {
        var out = []
        var re = /\{([^|}]+)\|([^|}]*)(\|new)?\}/g
        var last = 0, hit
        while ((hit = re.exec(m)) !== null) {
            if (hit.index > last) out.push({ text: m.slice(last, hit.index) })
            out.push({ text: hit[1], reading: hit[2], isNew: hit[3] !== undefined })
            last = re.lastIndex
        }
        if (last < m.length) out.push({ text: m.slice(last) })
        return out
    }

    // Plain runs split into single characters, so Flow can break between them.
    readonly property var pieces: {
        var src = markup.length > 0 ? parse(markup) : (segments || [])
        var out = []
        for (var i = 0; i < src.length; i++) {
            var s = src[i]
            if (s.reading && s.reading.length > 0) {
                out.push({ text: s.text, reading: s.reading, isNew: !!s.isNew })
            } else {
                var chars = Array.from(s.text || "")
                for (var c = 0; c < chars.length; c++) out.push({ text: chars[c], reading: "", isNew: false })
            }
        }
        // Whether the neighbours have no reading (a long reading may reach over them).
        for (var k = 0; k < out.length; k++) {
            out[k].openBefore = k > 0 && out[k - 1].reading.length === 0
            out[k].openAfter = k < out.length - 1 && out[k + 1].reading.length === 0
        }
        return out
    }

    readonly property font readingFont: Qt.font({
        family: font.family,
        pointSize: Math.max(6, font.pointSize * readingScale)
    })

    spacing: 0

    Accessible.role: Accessible.StaticText
    Accessible.name: text

    Repeater {
        model: ruby.pieces
        delegate: Item {
            id: piece
            required property var modelData
            readonly property bool shown: modelData.reading.length > 0
                && (ruby.density === KanteRubyText.Density.All
                    || (ruby.density === KanteRubyText.Density.New && modelData.isNew))
            readonly property real reach: readingText.text.length > 0 ? readingText.implicitWidth / Math.max(1, readingText.text.length) : 0
            // Centred, so it reaches out on both sides alike: only when both neighbours are free, half a reading character each.
            readonly property real allowance: modelData.openBefore && modelData.openAfter ? reach : 0
            implicitWidth: Math.max(baseText.implicitWidth, modelData.reading.length > 0 ? readingText.implicitWidth - allowance : 0)
            implicitHeight: readingText.implicitHeight + baseText.implicitHeight
            width: implicitWidth
            height: implicitHeight
            Text {
                id: readingText
                // Always laid out, so every line keeps the same height in every density.
                anchors.horizontalCenter: parent.horizontalCenter
                text: piece.modelData.reading.length > 0 ? piece.modelData.reading : " "
                opacity: piece.shown ? 1 : 0
                font: ruby.readingFont
                color: ruby.readingColor
                Accessible.ignored: true
            }
            Text {
                id: baseText
                anchors.horizontalCenter: parent.horizontalCenter
                y: readingText.implicitHeight
                text: piece.modelData.text
                font: ruby.font
                color: ruby.color
                Accessible.ignored: true
            }
        }
    }
}
