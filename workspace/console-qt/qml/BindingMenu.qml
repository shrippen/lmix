import QtQuick
import QtQuick.Controls as QQC2
import Kante

/**
 * Binding of a title field (docs/DESIGN-DIALOGE.md M11 A): "Not bound",
 * then a section per data source with its columns, each named once, the
 * current row's value on the right as an example; the bound one ticked.
 *   sources  `ShowState.data_sources` (`id`, `name`, `columns`, `current`)
 *   bound    the field's binding `{source, column}`, or null
 * `picked(binding)` with `{source, column}` or null.
 */
QQC2.Menu {
    id: binder

    property var sources: []
    property var bound: null
    signal picked(var binding)

    KantePopupSkin { popup: binder }

    KanteMenuItem {
        text: qsTr("Not bound")
        checkable: true
        checked: binder.bound === null
        onToggled: checked = Qt.binding(() => binder.bound === null)
        onTriggered: binder.picked(null)
    }

    /** The current value of a column, from the live state (rows move on). */
    function example(sourceId, column) {
        const s = sources.find(x => x.id === sourceId)
        const at = s ? s.columns.indexOf(column) : -1
        return at >= 0 && s.current && at < s.current.length ? s.current[at] : ""
    }

    // Made here rather than by a Repeater (see ChoiceMenu); again only
    // when sources or columns change, not with each new row.
    property var made: []
    property string madeFrom: ""
    function rebuild() {
        const shape = sources.map(s => ({ id: s.id, name: s.name, columns: Array.from(s.columns) }))
        const from = JSON.stringify(shape)
        if (from === madeFrom) {
            return
        }
        madeFrom = from
        for (const item of made) {
            binder.removeItem(item)
            item.destroy()
        }
        const list = []
        for (const s of shape) {
            const head = sectionRow.createObject(null, { text: s.name })
            binder.addItem(head)
            list.push(head)
            for (const column of s.columns) {
                const item = columnRow.createObject(null, { sourceId: s.id, column: column })
                binder.addItem(item)
                list.push(item)
            }
        }
        made = list
    }
    onSourcesChanged: rebuild()
    Component.onCompleted: rebuild()
    // Made without a parent (a menu is no item): they go with it.
    Component.onDestruction: made.forEach(item => item.destroy())

    Component {
        id: sectionRow
        KanteMenuSection {
            width: binder.width
        }
    }
    Component {
        id: columnRow
        KanteMenuItem {
            property var sourceId
            property string column
            readonly property bool isBound: binder.bound !== null
                && binder.bound.source === sourceId && binder.bound.column === column
            text: column
            valueText: binder.example(sourceId, column)
            checkable: true
            checked: isBound
            onToggled: checked = Qt.binding(() => isBound)
            onTriggered: binder.picked({ source: sourceId, column: column })
        }
    }
}
