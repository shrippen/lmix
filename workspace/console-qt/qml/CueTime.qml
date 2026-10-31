import QtQuick
import QtQuick.Controls as QQC2
import Kante

/**
 * A time of a cue in the list (pre wait, duration, post wait) with its
 * progress (PLAN-SEITEN, click dummy): the set time, or while its part
 * runs the time gone, the field filling as it goes. A part that is over
 * stays full and shows the set time again.
 *
 *   [0:02.0     ]   not running
 *   [▓▓▓0:01.2░░]   running, 60 % filled
 */
Item {
    id: time

    /** The set time; null or 0 shows nothing. */
    property var ms: null
    /** How long the part runs, for the fill; null while unknown. */
    property var length: ms
    /** Time gone in this part, or -1 when it has not begun. */
    property real done: -1
    /** m:ss.s of a time (ShowView.clock). */
    required property var clock

    /** No length known (a sound without duration before it starts):
     *  only the time gone shows. */
    readonly property bool unknown: length === null || length === undefined
    readonly property bool timed: !unknown && length > 0
    readonly property bool runs: done >= 0 && (unknown || done < length)

    implicitHeight: label.implicitHeight
    Accessible.role: Accessible.StaticText
    Accessible.name: label.text

    // The field fills like a running cue in the side bar.
    KanteProgressBar {
        anchors.fill: parent
        visible: time.done >= 0 && time.timed
        value: time.timed ? time.done / time.length : 0
        color: KanteStyle.tint(KanteStyle.positiveTextColor, 0.22)
    }
    QQC2.Label {
        id: label
        anchors.fill: parent
        verticalAlignment: Text.AlignVCenter
        text: time.runs ? time.clock(time.done) : (time.ms ? time.clock(time.ms) : "")
        font.family: KanteStyle.monoFamily
        color: time.runs ? KanteStyle.strongTextColor : KanteStyle.textColor
    }
}
