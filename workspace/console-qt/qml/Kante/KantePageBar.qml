import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts
import "."

/**
 * Page bar at the foot of an app window (like the page bar of DaVinci Resolve): one tab per
 * page, the app shows the page of `currentIndex`. `model` is a list of labels (mono uppercase,
 * as Kante labels), `tooltips` an optional list of hints. The current tab is raised on the card
 * colour with a marker line in the focus colour along its top edge; hover is the tint role.
 *
 * `busy` (optional list of bools) puts a dot on a tab: something runs on that page while
 * another one is shown. `busyKinds` (optional, KantePageBar.Kind per tab) colours it: Normal
 * the accent (its text-safe shade, so the dot reads on Leinen too), Info, Warning, Error;
 * Warning and Error draw a diamond instead of a round dot, so the kind is not colour alone,
 * and screen readers hear `kindNames` as the tab's description.
 *
 * Items declared inside the bar go to its right end (`trailing`, e.g. a small program picture
 * and a "Panic" button); `leading` takes items for the left end. The tabs scroll sideways when
 * they do not fit and the current tab is kept in view. Left / Right (Home / End) move the page
 * while the bar has focus; `activated(index)` fires on every change made in the bar.
 */
FocusScope {
    id: bar

    enum Kind {
        Normal,
        Info,
        Warning,
        Error
    }

    property var model: []
    property var tooltips: []
    property var busy: []
    property var busyKinds: []
    /** Words for the busy kinds (screen readers), in Kind order; an app sets its translations once. */
    property var kindNames: ["Läuft", "Info", "Warnung", "Fehler"]
    property int currentIndex: 0
    signal activated(int index)

    /** Items at the right end of the bar (the default property). */
    default property alias trailing: trailingRow.data
    /** Items at the left end of the bar. */
    property alias leading: leadingRow.data

    readonly property int count: rep.count

    implicitHeight: KanteStyle.heightMedium
    implicitWidth: leadingRow.implicitWidth + row.implicitWidth + trailingRow.implicitWidth + 4 * KanteStyle.unit(8)
    activeFocusOnTab: true

    Accessible.role: Accessible.PageTabList

    Keys.onLeftPressed: select(Math.max(0, currentIndex - 1))
    Keys.onRightPressed: select(Math.min(model.length - 1, currentIndex + 1))
    Keys.onPressed: function (event) {
        if (event.key === Qt.Key_Home) {
            select(0)
            event.accepted = true
        } else if (event.key === Qt.Key_End) {
            select(model.length - 1)
            event.accepted = true
        }
    }

    function select(i) {
        if (i >= 0 && i < model.length && i !== currentIndex) {
            currentIndex = i
            activated(i)
        }
    }

    /** Whether page i shows the busy dot. */
    function isBusy(i) {
        return !!busy && busy.length > i && !!busy[i]
    }
    /** The busy kind of page i (KantePageBar.Kind; Normal when not set). */
    function kindOf(i) {
        let k = busyKinds && busyKinds.length > i ? Number(busyKinds[i]) : KantePageBar.Kind.Normal
        return k >= KantePageBar.Kind.Normal && k <= KantePageBar.Kind.Error ? k : KantePageBar.Kind.Normal
    }
    /** Colour of a busy kind. */
    function kindColor(kind) {
        switch (kind) {
        case KantePageBar.Kind.Info: return KanteStyle.infoColor
        case KantePageBar.Kind.Warning: return KanteStyle.warningColor
        case KantePageBar.Kind.Error: return KanteStyle.negativeTextColor
        }
        return KanteStyle.accentTextColor
    }

    // Scrolls the current tab into view, within the content: a tab area not yet laid out (width
    // 0 or below) scrolls nothing, and once the tabs fit the scroll goes back to 0. It also runs
    // when the tab area grows, so a scroll aimed at the narrow area (still animating) is replaced.
    function ensureVisible() {
        let t = rep.count > 0 ? rep.itemAt(currentIndex) : null
        if (!t || flick.width <= 0) {
            return
        }
        let x = flick.contentX
        if (t.x < x) {
            x = t.x
        } else if (t.x + t.width > x + flick.width) {
            x = t.x + t.width - flick.width
        }
        flick.contentX = Math.max(0, Math.min(x, flick.contentWidth - flick.width))
    }
    // After the layout: the Row places its tabs in the polish step, so x is only right later.
    function queueEnsureVisible() {
        Qt.callLater(ensureVisible)
    }
    onCurrentIndexChanged: queueEnsureVisible()

    // Ground: sunken, with a rule along the top edge towards the page above.
    Rectangle {
        anchors.fill: parent
        color: KanteStyle.sunkenColor
    }
    Rectangle {
        width: parent.width
        height: 1
        color: KanteStyle.ruleColor
    }

    RowLayout {
        id: leadingRow
        anchors.left: parent.left
        anchors.leftMargin: children.length > 0 ? KanteStyle.unit(8) : 0
        anchors.verticalCenter: parent.verticalCenter
        spacing: KanteStyle.unit(8)
    }

    Flickable {
        id: flick
        anchors.left: leadingRow.right
        anchors.leftMargin: leadingRow.children.length > 0 ? KanteStyle.unit(8) : 0
        anchors.right: trailingRow.left
        anchors.rightMargin: KanteStyle.unit(8)
        height: parent.height
        contentWidth: row.implicitWidth
        contentHeight: height
        flickableDirection: Flickable.HorizontalFlick
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentWidth > width
        clip: true
        // Animate only the scrolls of ensureVisible(); a drag or flick follows the pointer directly.
        Behavior on contentX {
            enabled: !flick.dragging && !flick.flicking
            NumberAnimation { duration: KanteStyle.duration; easing.type: Easing.OutCubic }
        }
        // The tab area changes with the bar and with the leading and trailing items.
        onWidthChanged: bar.queueEnsureVisible()

        Row {
            id: row
            height: parent.height
            // Tabs placed or resized (model, font).
            onWidthChanged: bar.queueEnsureVisible()

            Repeater {
                id: rep
                model: bar.model
                delegate: Item {
                    id: tab
                    required property int index
                    required property var modelData
                    readonly property bool current: bar.currentIndex === index
                    readonly property string tip: bar.tooltips && bar.tooltips.length > index ? String(bar.tooltips[index]) : ""
                    readonly property bool busy: bar.isBusy(index)
                    readonly property int kind: bar.kindOf(index)
                    readonly property color ink: current || hover.hovered ? KanteStyle.strongTextColor : KanteStyle.mutedTextColor
                    readonly property bool inView: x + width > flick.contentX && x < flick.contentX + flick.width
                    width: content.implicitWidth + KanteStyle.unit(32)
                    height: row.height

                    // Hover: the tint role, square.
                    Rectangle {
                        anchors.fill: parent
                        visible: !tab.current && hover.hovered
                        color: KanteStyle.tint1Color
                    }
                    // The raised current tab. Shapes outside the flickable are still painted by
                    // the software renderer (it ignores the clip for them): draw only in view.
                    KantePolygon {
                        objectName: "kantePageTab"
                        anchors.fill: parent
                        visible: tab.current && tab.inView
                        fillColor: KanteStyle.cardColor
                        cutTopRight: KanteStyle.chamferSmall
                    }
                    // Marker line along the top edge of the current tab.
                    Rectangle {
                        objectName: "kantePageMarker"
                        visible: tab.current
                        width: parent.width - KanteStyle.chamferSmall
                        height: Math.max(2, KanteStyle.unit(3))
                        color: KanteStyle.focusColor
                    }
                    // Keyboard focus: a frame in the focus colour around the current tab, cut as the tab.
                    KantePolygon {
                        objectName: "kantePageFocus"
                        anchors.fill: parent
                        visible: tab.current && tab.inView && bar.activeFocus
                        strokeColor: KanteStyle.focusColor
                        strokeWidth: 1
                        cutTopRight: KanteStyle.chamferSmall
                    }

                    RowLayout {
                        id: content
                        anchors.centerIn: parent
                        spacing: KanteStyle.unit(6)
                        Text {
                            text: tab.modelData
                            font: KanteStyle.labelFont()
                            color: tab.ink
                            Behavior on color { ColorAnimation { duration: KanteStyle.durationFast } }
                        }
                        // Busy dot: round for Normal and Info, a diamond for Warning and Error.
                        Rectangle {
                            objectName: "kantePageBusy"
                            visible: tab.busy
                            readonly property bool diamond: tab.kind === KantePageBar.Kind.Warning || tab.kind === KantePageBar.Kind.Error
                            Layout.preferredWidth: KanteStyle.unit(diamond ? 6 : 7)
                            Layout.preferredHeight: Layout.preferredWidth
                            Layout.alignment: Qt.AlignVCenter
                            radius: diamond ? 0 : width / 2
                            rotation: diamond ? 45 : 0
                            color: bar.kindColor(tab.kind)
                        }
                    }

                    HoverHandler { id: hover }
                    TapHandler { onTapped: bar.select(tab.index) }
                    QQC2.ToolTip.text: tab.tip
                    QQC2.ToolTip.visible: hover.hovered && tab.tip.length > 0
                    QQC2.ToolTip.delay: 500
                    Accessible.role: Accessible.PageTab
                    Accessible.name: String(tab.modelData)
                    Accessible.description: tab.busy ? String(bar.kindNames && bar.kindNames.length > tab.kind ? bar.kindNames[tab.kind] : "") : tab.tip
                    Accessible.selected: tab.current
                    Accessible.selectable: true
                }
            }
        }
    }

    RowLayout {
        id: trailingRow
        anchors.right: parent.right
        anchors.rightMargin: children.length > 0 ? KanteStyle.unit(8) : 0
        anchors.verticalCenter: parent.verticalCenter
        spacing: KanteStyle.unit(8)
    }
}
