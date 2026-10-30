import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import org.kde.kirigami as Kirigami
import Kante

/**
 * Effect and length of a transition button or of quick play, on one level
 * (docs/DESIGN-DIALOGE.md M08 A): the effects as a list, a direction as
 * ◀ ▶ behind its name, the lengths as segments under it.
 *   heading    "Button 1 · Effect"
 *   effects    `[{value, label}]` (Texts.choices("effects", …))
 *   effect     the current effect; `duration` the current length in ms
 *   durations  the lengths to offer, in ms
 *   stinger    offer the stinger (transition buttons; quick play has none)
 * `effectPicked(value)` and `lengthPicked(ms)` on a choice,
 * `stingerRequested()` for "Stinger…" (clip and cut point first).
 * Left and Right on a highlighted direction or length row choose there.
 */
QQC2.Menu {
    id: menu

    property string heading: ""
    property var effects: []
    property string effect: ""
    property int duration: 0
    property var durations: []
    property bool stinger: true
    signal effectPicked(string value)
    signal lengthPicked(int ms)
    signal stingerRequested()

    KantePopupSkin { popup: menu }

    /** The effects as rows: a direction pair (`slide_left`, `slide_right`)
     *  is one row named without its arrow. */
    readonly property var rows: {
        const out = []
        for (const e of effects) {
            if (e.value === "stinger" && !stinger) {
                continue
            }
            const pair = e.value.match(/^(.*)_(left|right)$/)
            if (!pair) {
                out.push({ value: e.value, label: e.label, directions: [] })
            } else if (pair[2] === "left") {
                out.push({ value: pair[1], label: e.label.replace(/\s*[◀▶]\s*$/, ""), directions: [e.value, pair[1] + "_right"] })
            }
        }
        return out
    }

    KanteMenuSection {
        id: head
        width: parent ? parent.width : implicitWidth
        text: menu.heading
        rule: false
    }

    // Made here rather than by a Repeater, as in ChoiceMenu: Breeze's
    // MenuItem calls RegExp(), which a Repeater's model data shadows.
    property var made: []
    property string madeFrom: ""
    function rebuild() {
        const from = JSON.stringify(rows)
        if (from === madeFrom) {
            return
        }
        madeFrom = from
        for (const item of made) {
            menu.removeItem(item)
            item.destroy()
        }
        const list = []
        // Under the heading: the items declared here may not all be in yet.
        let at = 0
        while (at < menu.count && menu.itemAt(at) !== head) {
            at++
        }
        rows.forEach((row, i) => {
            const kind = row.directions.length > 0 ? directionRow
                : row.value === "stinger" ? stingerRow : plainRow
            const item = kind.createObject(null, { row: row })
            menu.insertItem(at + 1 + i, item)
            list.push(item)
        })
        made = list
    }
    onRowsChanged: rebuild()
    Component.onCompleted: rebuild()
    // Made without a parent (a menu is no item): they go with it.
    Component.onDestruction: made.forEach(item => item.destroy())

    Component {
        id: plainRow
        KanteMenuItem {
            property var row
            text: row.label
            checkable: true
            // `menu` is gone first when the menu goes. The state decides,
            // not the click: a click toggles, the binding comes back (the
            // effect may stay the same, or the core refuse).
            checked: !!menu && menu.effect === row.value
            onToggled: checked = Qt.binding(() => !!menu && menu.effect === row.value)
            onTriggered: menu.effectPicked(row.value)
        }
    }
    Component {
        id: stingerRow
        KanteMenuItem {
            property var row
            text: qsTr("Stinger…")
            checkable: true
            checked: !!menu && menu.effect === "stinger"
            onToggled: checked = Qt.binding(() => !!menu && menu.effect === "stinger")
            onTriggered: menu.stingerRequested()
        }
    }
    Component {
        id: directionRow
        DirectionItem {}
    }

    KanteMenuSection {
        width: parent ? parent.width : implicitWidth
        text: qsTr("Length")
    }
    LengthItem {}

    /** One effect in both directions: ◀ ▶ behind the name; a click on the
     *  row keeps the current direction (left when it is another effect). */
    component DirectionItem: QQC2.MenuItem {
        id: direction
        property var row: ({ value: "", label: "", directions: ["", ""] })
        readonly property int current: menu ? row.directions.indexOf(menu.effect) : -1
        checkable: true
        checked: current >= 0
        onToggled: checked = Qt.binding(() => current >= 0)
        text: row.label
        Accessible.name: row.label
        Accessible.description: current === 1 ? qsTr("To the right") : current === 0 ? qsTr("To the left") : ""
        onTriggered: menu.effectPicked(row.directions[Math.max(0, current)])
        Keys.onLeftPressed: pick(0)
        Keys.onRightPressed: pick(1)
        function pick(side) {
            menu.effectPicked(row.directions[side])
            menu.close()
        }
        contentItem: RowLayout {
            spacing: Kirigami.Units.smallSpacing * 2
            Item {
                Layout.preferredWidth: direction.indicator ? direction.indicator.width : 0
            }
            QQC2.Label {
                Layout.fillWidth: true
                text: direction.text
                font: direction.font
                color: direction.highlighted ? Kirigami.Theme.highlightedTextColor : Kirigami.Theme.textColor
            }
            Repeater {
                model: [{ arrow: "◀", name: qsTr("To the left") }, { arrow: "▶", name: qsTr("To the right") }]
                KanteToolButton {
                    required property var modelData
                    required property int index
                    text: modelData.arrow
                    checkable: true
                    checked: direction.current === index
                    onToggled: checked = Qt.binding(() => direction.current === index)
                    focusPolicy: Qt.NoFocus
                    // As high as the other rows.
                    Layout.preferredHeight: Kirigami.Units.iconSizes.small + Kirigami.Units.smallSpacing
                    Layout.preferredWidth: Kirigami.Units.iconSizes.small + Kirigami.Units.smallSpacing * 2
                    padding: 0
                    Accessible.name: direction.text + " " + modelData.name
                    onClicked: direction.pick(index)
                }
            }
        }
    }

    /** The lengths as segments; Left and Right move to the next one. */
    component LengthItem: QQC2.MenuItem {
        id: length
        objectName: "lengths"
        readonly property int current: menu.durations.indexOf(menu.duration)
        text: qsTr("Length")
        Accessible.description: Texts.label("durations", menu.duration)
        // The row is no choice of its own: the segments are.
        onTriggered: {}
        Keys.onLeftPressed: step(-1)
        Keys.onRightPressed: step(1)
        function step(by) {
            const next = Math.max(0, Math.min(menu.durations.length - 1, current + by))
            if (next !== current) {
                menu.lengthPicked(menu.durations[next])
            }
        }
        contentItem: RowLayout {
            spacing: Kirigami.Units.smallSpacing
            KanteSegmented {
                id: lengths
                model: menu.durations.map(ms => String(ms))
                tooltips: menu.durations.map(ms => Texts.label("durations", ms))
                currentIndex: length.current
                activeFocusOnTab: false
                // The state decides: a click asks, the binding comes back.
                onActivated: index => {
                    menu.lengthPicked(menu.durations[index])
                    currentIndex = Qt.binding(() => length.current)
                }
            }
            QQC2.Label {
                text: qsTr("ms", "milliseconds")
                font: KanteStyle.active ? KanteStyle.monoFont(KanteStyle.smallFont.pointSize) : KanteStyle.smallFont
                color: KanteStyle.mutedTextColor
            }
        }
    }
}
