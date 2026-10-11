import QtQuick
import QtQuick.Layouts
import QtQuick.Controls as QQC2
import org.kde.kirigami as Kirigami
import Kante

/**
 * Data sources (DAT.1–5): CSV, JSON or text files or addresses whose
 * selected row feeds title fields. A click on a row selects it; previous / next, or moving on by
 * itself every few seconds, looping if wanted. lmix-core reads a file
 * again when it changes, an address every 10 s.
 */
KanteDialog {
    id: dialog

    required property var act
    /** `ShowState.data_sources`. */
    required property var sources
    /** lmix-core's address: the rows come from `/api/data/<source>`. */
    required property string coreUrl
    /** Rows of the shown source (lmix `DataTable`). */
    property var table: ({ columns: [], rows: [], revision: -1, source: "" })

    /** Fetch the rows again when the source or its revision changed. */
    function loadTable() {
        const s = dialog.source
        if (!s || !dialog.visible || (dialog.table.source === s.id && dialog.table.revision === s.revision))
            return
        const request = new XMLHttpRequest()
        request.onreadystatechange = function () {
            if (request.readyState !== XMLHttpRequest.DONE || request.status !== 200)
                return
            const t = JSON.parse(request.responseText)
            t.source = s.id
            dialog.table = t
        }
        request.open("GET", dialog.coreUrl + "/api/data/" + s.id)
        request.send()
    }
    onSourceChanged: loadTable()
    onOpened: loadTable()
    readonly property var revision: source ? source.revision : -1
    onRevisionChanged: loadTable()

    property int current: 0
    readonly property var source: sources.length > 0 ? sources[Math.min(current, sources.length - 1)] : null
    /** Kinds of source, as lmix names them. */
    readonly property var kinds: [
        { kind: "csv", label: qsTr("CSV table") },
        { kind: "json", label: qsTr("JSON") },
        { kind: "text", label: qsTr("Text, a row per line") },
        { kind: "feed", label: qsTr("RSS / Atom feed") },
        { kind: "xml", label: qsTr("XML") }
    ]
    /** Kind of source guessed from its name (the operator can change it);
     * CSV also for a Google Sheet's CSV export. */
    function kindOf(file) {
        const name = file.trim().toLowerCase().split("?")[0]
        if (name.endsWith(".json")) return "json"
        if (name.endsWith(".txt")) return "text"
        if (name.endsWith(".rss") || name.endsWith(".atom") || /(rss|feed|atom)/.test(name)) return "feed"
        if (name.endsWith(".xml")) return "xml"
        return "csv"
    }
    /** Auto-advance choices in seconds; 0 off. */
    readonly property var intervals: [0, 3, 5, 10, 15, 30, 60]

    title: qsTr("Data sources")
    modal: true
    width: Kirigami.Units.gridUnit * 44
    standardButtons: QQC2.Dialog.Close
    contentHeight: column.implicitHeight

    ColumnLayout {
        id: column
        width: parent.width
        spacing: Kirigami.Units.smallSpacing

        RowLayout {
            Layout.fillWidth: true
            QQC2.ComboBox {
                id: sourceBox
                Layout.fillWidth: true
                visible: dialog.sources.length > 0
                model: dialog.sources.map(s => s.number + " " + s.name)
                currentIndex: dialog.current
                onActivated: index => dialog.current = index
                KanteFieldSkin { control: sourceBox }
            }
            KanteButton {
                text: qsTr("Remove")
                visible: dialog.source !== null
                onClicked: dialog.act({ action: "remove_data_source", source: dialog.source.number })
            }
        }

        RowLayout {
            Layout.fillWidth: true
            KanteTextField {
                id: path
                Layout.fillWidth: true
                placeholderText: qsTr("File (relative to the show file) or http(s) address")
                onTextEdited: kindBox.currentIndex = dialog.kinds.findIndex(k => k.kind === dialog.kindOf(text))
            }
            QQC2.ComboBox {
                id: kindBox
                model: dialog.kinds.map(k => k.label)
                KanteFieldSkin { control: kindBox }
            }
            QQC2.CheckBox {
                id: header
                text: qsTr("First row names columns")
                checked: true
                visible: dialog.kinds[kindBox.currentIndex].kind === "csv"
                KanteCheckSkin { control: header }
            }
            KanteTextField {
                id: xmlRows
                visible: dialog.kinds[kindBox.currentIndex].kind === "xml"
                placeholderText: qsTr("Rows, e.g. scores/game")
            }
            KanteButton {
                text: qsTr("+ Source")
                enabled: path.text.trim() !== "" && (dialog.kinds[kindBox.currentIndex].kind !== "xml" || xmlRows.text.trim() !== "")
                onClicked: {
                    const file = path.text.trim()
                    const kind = dialog.kinds[kindBox.currentIndex].kind
                    const spec = { kind: kind, path: file }
                    if (kind === "csv") spec.header = header.checked
                    if (kind === "xml") spec.rows = xmlRows.text.trim()
                    dialog.act({ action: "add_data_source", spec: spec })
                    dialog.current = dialog.sources.length
                    path.text = ""
                }
            }
        }

        QQC2.Label {
            Layout.fillWidth: true
            visible: dialog.source === null
            wrapMode: Text.WordWrap
            text: qsTr("No data sources yet. A data source is a table (CSV, JSON, a feed, …) whose selected row fills title fields, e.g. names and roles of guests.")
        }

        KanteCallout {
            Layout.fillWidth: true
            visible: dialog.source !== null && !!dialog.source.error
            kind: KanteCallout.Kind.Danger
            title: qsTr("The file cannot be read")
            text: dialog.source && dialog.source.error ? dialog.source.error : ""
        }

        QQC2.ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.preferredHeight: Math.min(rows.implicitHeight, Kirigami.Units.gridUnit * 16)
            visible: dialog.source !== null && dialog.source.row_count > 0

            ColumnLayout {
                id: rows
                width: scroll.availableWidth
                spacing: 0

                // Column names.
                RowLayout {
                    Layout.fillWidth: true
                    QQC2.Label { Layout.preferredWidth: Kirigami.Units.gridUnit * 2; text: "" }
                    Repeater {
                        model: dialog.source ? dialog.source.columns : []
                        QQC2.Label {
                            required property var modelData
                            Layout.fillWidth: true
                            Layout.preferredWidth: 1
                            text: modelData
                            font.family: KanteStyle.monoFamily
                            color: KanteStyle.mutedTextColor
                            elide: Text.ElideRight
                        }
                    }
                }

                Repeater {
                    model: dialog.source && dialog.table.source === dialog.source.id ? dialog.table.rows : []

                    Rectangle {
                        id: rowEntry
                        required property var modelData
                        required property int index
                        readonly property bool selected: dialog.source.row === index + 1
                        Layout.fillWidth: true
                        implicitHeight: cells.implicitHeight + Kirigami.Units.smallSpacing
                        color: selected ? KanteStyle.selectionColor : "transparent"

                        MouseArea {
                            anchors.fill: parent
                            onClicked: dialog.act({ action: "data_row", source: dialog.source.number, row: rowEntry.index + 1 })
                        }
                        RowLayout {
                            id: cells
                            anchors.fill: parent
                            anchors.leftMargin: Kirigami.Units.smallSpacing
                            QQC2.Label {
                                Layout.preferredWidth: Kirigami.Units.gridUnit * 2
                                text: (rowEntry.selected ? "▶ " : "") + (rowEntry.index + 1)
                                font.family: KanteStyle.monoFamily
                            }
                            Repeater {
                                model: rowEntry.modelData
                                QQC2.Label {
                                    required property var modelData
                                    Layout.fillWidth: true
                                    Layout.preferredWidth: 1
                                    text: modelData
                                    elide: Text.ElideRight
                                }
                            }
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            visible: dialog.source !== null

            KanteButton {
                text: qsTr("Previous")
                enabled: dialog.source !== null && dialog.source.row > 1
                onClicked: dialog.act({ action: "data_previous", source: dialog.source.number })
            }
            KanteButton {
                text: qsTr("Next")
                emphasis: KanteButton.Emphasis.Primary
                enabled: dialog.source !== null && (dialog.source.row < dialog.source.row_count || dialog.source.loop_rows)
                onClicked: dialog.act({ action: "data_next", source: dialog.source.number })
            }
            Item { Layout.fillWidth: true }
            QQC2.Label { text: qsTr("Move on") }
            QQC2.ComboBox {
                id: autoBox
                model: dialog.intervals.map(s => s === 0 ? qsTr("off") : qsTr("every %1 s").arg(s))
                currentIndex: dialog.source && dialog.source.auto_ms
                    ? Math.max(0, dialog.intervals.indexOf(dialog.source.auto_ms / 1000)) : 0
                onActivated: index => dialog.act({ action: "set_data_auto", source: dialog.source.number,
                    interval_ms: dialog.intervals[index] === 0 ? null : dialog.intervals[index] * 1000,
                    loop_rows: loopBox.checked })
                KanteFieldSkin { control: autoBox }
            }
            QQC2.CheckBox {
                id: loopBox
                text: qsTr("Loop")
                checked: dialog.source !== null && dialog.source.loop_rows
                onToggled: dialog.act({ action: "set_data_auto", source: dialog.source.number,
                    interval_ms: dialog.source.auto_ms || null, loop_rows: checked })
                KanteCheckSkin { control: loopBox }
            }
        }
    }
}
