pragma ComponentBehavior: Bound
import QtQuick
import "."

/**
 * Split handle (Kante 1.43, from lmix): the grip between two panes that sets a size by
 * dragging, such as the height of the monitors over the inputs or the width of a side column.
 * The app binds `value` to its stored size and writes it back in `moved`.
 *   orientation   Qt.Vertical: a horizontal bar dragged up and down (sets a height);
 *                 Qt.Horizontal: an upright bar dragged sideways (sets a width)
 *   value         the size it sets, in px
 *   from, to      limits of value
 *   defaultValue  a double click (or Delete) goes back to it; NaN: no reset
 *   inverted      the pane grows against the drag (a column on the right: dragging left widens it)
 *   step          keyboard step: arrows step, Page Up / Down four steps, Home / End the limits
 *   moved(value)  every change by drag, key or double click, already within the limits
 * Kante and Kante Light: three short lines in the muted text colour on a 1 px rule; on
 * hover, while dragged and with keyboard focus they turn the focus colour. System: the same
 * in the platform's colours. Screen readers get a slider with the value in px.
 */
Item {
    id: handle

    property int orientation: Qt.Vertical
    property real value: 0
    property real from: 0
    property real to: 100000
    property real defaultValue: NaN
    property bool inverted: false
    property real step: KanteStyle.unit(16)
    property string accessibleName: ""
    signal moved(real value)

    readonly property bool vertical: orientation === Qt.Vertical
    readonly property bool lit: drag.active || hover.hovered || activeFocus

    function setValue(v) {
        const c = Math.max(from, Math.min(to, v))
        if (c !== value) {
            moved(c)
        }
    }

    implicitWidth: vertical ? KanteStyle.unit(120) : KanteStyle.unit(12)
    implicitHeight: vertical ? KanteStyle.unit(12) : KanteStyle.unit(120)
    activeFocusOnTab: true

    Accessible.role: Accessible.Slider
    Accessible.name: accessibleName
    Accessible.focusable: true
    Accessible.onIncreaseAction: setValue(value + step)
    Accessible.onDecreaseAction: setValue(value - step)

    Keys.onPressed: (event) => {
        // Up and Left move the bar towards the start; the pane before it shrinks unless inverted.
        let d = 0
        switch (event.key) {
        case Qt.Key_Up:
        case Qt.Key_Left:
            d = -step
            break
        case Qt.Key_Down:
        case Qt.Key_Right:
            d = step
            break
        case Qt.Key_PageUp:
            d = -4 * step
            break
        case Qt.Key_PageDown:
            d = 4 * step
            break
        case Qt.Key_Home:
            handle.setValue(handle.inverted ? handle.to : handle.from)
            event.accepted = true
            return
        case Qt.Key_End:
            handle.setValue(handle.inverted ? handle.from : handle.to)
            event.accepted = true
            return
        case Qt.Key_Delete:
            if (!isNaN(handle.defaultValue)) {
                handle.setValue(handle.defaultValue)
            }
            event.accepted = true
            return
        default:
            return
        }
        handle.setValue(handle.value + (handle.inverted ? -d : d))
        event.accepted = true
    }

    Rectangle {
        anchors.centerIn: parent
        width: handle.vertical ? parent.width : 1
        height: handle.vertical ? 1 : parent.height
        color: KanteStyle.ruleColor
    }

    // The grip: three short lines across the drag direction, on the ground so the rule pauses there.
    Rectangle {
        id: grip
        anchors.centerIn: parent
        width: handle.vertical ? KanteStyle.unit(36) : KanteStyle.unit(8)
        height: handle.vertical ? KanteStyle.unit(8) : KanteStyle.unit(36)
        color: KanteStyle.backgroundColor

        Grid {
            anchors.centerIn: parent
            rows: handle.vertical ? 3 : 1
            columns: handle.vertical ? 1 : 3
            spacing: KanteStyle.unit(2)
            Repeater {
                model: 3
                Rectangle {
                    width: handle.vertical ? KanteStyle.unit(28) : 1
                    height: handle.vertical ? 1 : KanteStyle.unit(28)
                    color: handle.lit ? KanteStyle.focusColor : KanteStyle.mutedTextColor
                }
            }
        }
    }

    // Keyboard focus: a frame around the grip.
    Rectangle {
        visible: handle.activeFocus
        anchors.fill: grip
        anchors.margins: -KanteStyle.unit(2)
        color: "transparent"
        border.color: KanteStyle.focusColor
        border.width: 1
    }

    HoverHandler {
        id: hover
        cursorShape: handle.vertical ? Qt.SizeVerCursor : Qt.SizeHorCursor
    }

    DragHandler {
        id: drag

        property real startValue: 0

        target: null
        xAxis.enabled: !handle.vertical
        yAxis.enabled: handle.vertical
        cursorShape: handle.vertical ? Qt.SizeVerCursor : Qt.SizeHorCursor
        onActiveChanged: {
            if (active) {
                startValue = handle.value
                handle.forceActiveFocus()
            }
        }
        onActiveTranslationChanged: {
            if (!active) {
                return
            }
            const d = handle.vertical ? activeTranslation.y : activeTranslation.x
            handle.setValue(startValue + (handle.inverted ? -d : d))
        }
    }

    // A double click goes back to defaultValue. Counted here: the tap count of TapHandler
    // stays 1 when the press of the second click comes as a double-click event.
    TapHandler {
        property real lastTap: 0
        onTapped: {
            const now = Date.now()
            const second = now - lastTap <= Qt.styleHints.mouseDoubleClickInterval
            lastTap = second ? 0 : now
            if (second && !isNaN(handle.defaultValue)) {
                handle.setValue(handle.defaultValue)
            }
        }
    }
}
